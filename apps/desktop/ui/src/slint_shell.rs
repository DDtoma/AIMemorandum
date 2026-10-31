use crate::{
    ItemFilter, OutputRoute, ScreenshotOutputRouter, UiError, UiIntent, UiUpdate, UiWorker,
    ViewModel,
};
use aimemorandum_core::SubmitKey;
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

slint::include_modules!();

macro_rules! wire {
    ($sender:expr, || $body:expr) => {{
        let sender = $sender.clone();
        move || {
            let _ = sender.send($body);
        }
    }};
    ($sender:expr, |$($arg:ident),*| $body:expr) => {{
        let sender = $sender.clone();
        move |$($arg),*| {
            let _ = sender.send($body);
        }
    }};
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    ShowCapture,
    ToggleCapture,
    Screenshot,
    SaveClipboard,
    ShowItems,
    ShowSettings,
    CompleteItem(String),
    Quit,
}

pub struct SlintShell {
    pub capture: CapturePanel,
    pub items: ItemsWindow,
    pub reminder: ReminderPopup,
    pub settings: Settings,
    pub screenshot: ScreenshotOverlay,
    sender: crate::UiIntentSender,
    updates: std::sync::mpsc::Receiver<UiUpdate>,
    screenshot_output_router: Option<Box<dyn ScreenshotOutputRouter>>,
}

impl SlintShell {
    pub fn new<C: crate::Controller + 'static>(controller: C) -> Result<Self, UiError> {
        let (sender, updates) = UiWorker::spawn(controller).into_parts();
        let capture = CapturePanel::new().map_err(platform_error)?;
        let items = ItemsWindow::new().map_err(platform_error)?;
        let reminder = ReminderPopup::new().map_err(platform_error)?;
        let settings = Settings::new().map_err(platform_error)?;
        let screenshot = ScreenshotOverlay::new().map_err(platform_error)?;

        capture.on_draft_changed(wire!(&sender, |text| {
            UiIntent::CaptureDraftChanged(text.to_string())
        }));
        capture.on_submit(wire!(&sender, || UiIntent::SubmitCapture));
        capture.on_capture_key({
            let sender = sender.clone();
            move |shift, marked, numpad| {
                let _ = sender.send(UiIntent::CaptureKey {
                    key: if numpad {
                        SubmitKey::NumpadEnter
                    } else {
                        SubmitKey::Enter
                    },
                    shift,
                    marked,
                });
            }
        });
        capture.on_clipboard(wire!(&sender, || UiIntent::CaptureClipboard));
        capture.on_paths_dropped(wire!(&sender, |paths| {
            UiIntent::DropImages(
                paths
                    .iter()
                    .map(|path| PathBuf::from(path.as_str()))
                    .collect(),
            )
        }));
        capture.on_show_items({
            let items = items.as_weak();
            let sender = sender.clone();
            move || {
                if let Some(items) = items.upgrade() {
                    let _ = items.show();
                }
                let _ = sender.send(UiIntent::Refresh);
            }
        });
        let show_settings = {
            let settings = settings.as_weak();
            move || {
                if let Some(settings) = settings.upgrade() {
                    let _ = settings.show();
                }
            }
        };
        capture.on_show_settings(show_settings.clone());

        items.on_refresh(wire!(&sender, || UiIntent::Refresh));
        items.on_select(wire!(&sender, |id| UiIntent::SelectItem(id.to_string())));
        items.on_filter_changed(wire!(&sender, |filter| {
            UiIntent::SetFilter(match filter {
                1 => ItemFilter::Active,
                2 => ItemFilter::Waiting,
                3 => ItemFilter::Completed,
                _ => ItemFilter::All,
            })
        }));
        items.on_detail_changed(wire!(&sender, |title, body_md| {
            UiIntent::DetailDraftChanged {
                title: title.to_string(),
                body_md: body_md.to_string(),
            }
        }));
        items.on_save(wire!(&sender, || UiIntent::SaveSelected));
        items.on_complete(wire!(&sender, |id| {
            UiIntent::CompleteItem(id.to_string())
        }));
        items.on_reopen(wire!(&sender, |id| {
            UiIntent::ReopenItem(id.to_string())
        }));
        items.on_remove(wire!(&sender, |id| {
            UiIntent::DeleteItem(id.to_string())
        }));
        items.on_remind(wire!(&sender, |id| {
            UiIntent::RemindItemInTwoHours(id.to_string())
        }));
        items.on_open_attachment(wire!(&sender, |item_id, attachment_id| {
            UiIntent::OpenAttachment {
                item_id: item_id.to_string(),
                attachment_id: attachment_id.to_string(),
            }
        }));
        items.on_show_settings(show_settings);

        reminder.on_complete(wire!(&sender, || UiIntent::CompleteReminder));
        reminder.on_snooze(wire!(&sender, || UiIntent::SnoozeReminder));
        reminder.on_dismiss(wire!(&sender, || UiIntent::DismissReminder));

        settings.on_settings_changed(wire!(&sender, |server_url, code, name| {
            UiIntent::SettingsChanged {
                server_url: server_url.to_string(),
                enrollment_code: code.to_string(),
                device_name: name.to_string(),
            }
        }));
        settings.on_register(wire!(&sender, || UiIntent::Register));
        settings.on_autostart_changed(wire!(&sender, |enabled| UiIntent::SetAutostart(enabled)));
        settings.on_close_requested({
            let settings = settings.as_weak();
            move || {
                if let Some(settings) = settings.upgrade() {
                    let _ = settings.hide();
                }
            }
        });

        screenshot.on_started(wire!(&sender, |x, y| {
            UiIntent::ScreenshotStarted {
                x: x as f64,
                y: y as f64,
            }
        }));
        screenshot.on_moved(wire!(&sender, |x, y| {
            UiIntent::ScreenshotMoved {
                x: x as f64,
                y: y as f64,
            }
        }));
        screenshot.on_finished(wire!(&sender, |x, y| {
            UiIntent::ScreenshotFinished {
                x: x as f64,
                y: y as f64,
            }
        }));
        screenshot.on_cancelled(wire!(&sender, || UiIntent::ScreenshotCancelled));

        Ok(Self {
            capture,
            items,
            reminder,
            settings,
            screenshot,
            sender,
            updates,
            screenshot_output_router: None,
        })
    }

    /// Installs platform-specific routing without leaking native backends into shared UI.
    pub fn with_screenshot_output_router(
        mut self,
        router: Box<dyn ScreenshotOutputRouter>,
    ) -> Self {
        self.screenshot_output_router = Some(router);
        self
    }

    pub fn sender(&self) -> &crate::UiIntentSender {
        &self.sender
    }

    pub fn run(self) -> Result<(), UiError> {
        self.run_inner(None, true, Box::new(|_| {}))
    }

    pub fn run_with_events(
        self,
        events: Receiver<ShellEvent>,
        show_capture: bool,
        observer: impl FnMut(&ViewModel) + 'static,
    ) -> Result<(), UiError> {
        self.run_inner(Some(events), show_capture, Box::new(observer))
    }

    fn run_inner(
        self,
        mut events: Option<Receiver<ShellEvent>>,
        show_capture: bool,
        mut observer: Box<dyn FnMut(&ViewModel)>,
    ) -> Result<(), UiError> {
        let Self {
            capture,
            items,
            reminder,
            settings,
            screenshot,
            sender,
            updates,
            screenshot_output_router,
        } = self;
        if show_capture {
            capture.show().map_err(platform_error)?;
        }
        let timer = Timer::default();
        let mut last_view: Option<ViewModel> = None;
        let mut screenshot_image = CachedScreenshot::default();
        let mut screenshot_shown = false;
        let mut screenshot_route_pending = false;
        let mut ticks = 0_u64;
        let mut quitting = false;
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            if screenshot_route_pending {
                let display = last_view.as_ref().and_then(|view| view.screenshot.display);
                let routed = match (screenshot_output_router.as_ref(), display) {
                    (Some(router), Some(display)) => router.route(screenshot.window(), display),
                    (None, _) => Err("截图输出路由器不可用".to_owned()),
                    (_, None) => Err("截图缺少显示器几何信息".to_owned()),
                };
                match routed {
                    Ok(OutputRoute::Pending) => {}
                    Ok(OutputRoute::Routed) => {
                        screenshot_route_pending = false;
                        screenshot.set_output_ready(true);
                    }
                    Err(error) => {
                        screenshot_route_pending = false;
                        screenshot_failed(&screenshot, &sender, error);
                    }
                }
            }
            if let Some(events) = events.as_mut() {
                while let Ok(event) = events.try_recv() {
                    match event {
                        ShellEvent::ShowCapture => {
                            let _ = capture.show();
                        }
                        ShellEvent::ToggleCapture => {
                            if capture.window().is_visible() {
                                let _ = capture.hide();
                            } else {
                                let _ = capture.show();
                            }
                        }
                        ShellEvent::Screenshot => {
                            let _ = capture.hide();
                            let _ = items.hide();
                            let _ = reminder.hide();
                            let _ = settings.hide();
                            let screenshot_sender = sender.clone();
                            Timer::single_shot(Duration::from_millis(150), move || {
                                let _ = screenshot_sender.send(UiIntent::BeginScreenshot);
                            });
                        }
                        ShellEvent::SaveClipboard => {
                            let _ = capture.show();
                            let _ = sender.send(UiIntent::CaptureClipboard);
                        }
                        ShellEvent::ShowItems => {
                            let _ = items.show();
                            let _ = sender.send(UiIntent::Refresh);
                        }
                        ShellEvent::ShowSettings => {
                            let _ = settings.show();
                        }
                        ShellEvent::CompleteItem(id) => {
                            let _ = sender.send(UiIntent::CompleteItem(id));
                        }
                        ShellEvent::Quit => {
                            if !quitting {
                                quitting = true;
                                let _ = capture.hide();
                                let _ = items.hide();
                                let _ = reminder.hide();
                                let _ = settings.hide();
                                let _ = screenshot.hide();
                                if sender.send(UiIntent::Shutdown).is_err() {
                                    let _ = slint::quit_event_loop();
                                }
                            }
                        }
                    }
                }
            }
            let mut latest = None;
            let mut shutdown_finished = false;
            while let Ok(update) = updates.try_recv() {
                shutdown_finished |= update.shutdown_finished;
                latest = Some(update);
            }
            if shutdown_finished {
                let _ = slint::quit_event_loop();
            }
            if let Some(update) = latest {
                if last_view.as_ref() != Some(&update.view) || update.error.is_some() {
                    last_view = Some(update.view.clone());
                    observer(&update.view);
                    apply_update(
                        ShellWindows {
                            capture: &capture,
                            items: &items,
                            reminder: &reminder,
                            settings: &settings,
                            screenshot: &screenshot,
                        },
                        update,
                        &mut screenshot_image,
                        &mut screenshot_shown,
                        &mut screenshot_route_pending,
                        &sender,
                        screenshot_output_router.is_some(),
                    );
                }
            }
            ticks += 1;
            if ticks.is_multiple_of(20) {
                let _ = sender.send(UiIntent::Refresh);
            }
            if ticks.is_multiple_of(100) {
                let _ = sender.send(UiIntent::CheckReminders);
            }
        });
        slint::run_event_loop().map_err(platform_error)
    }
}

pub fn run_slint<C: crate::Controller + 'static>(controller: C) -> Result<(), UiError> {
    SlintShell::new(controller)?.run()
}

fn platform_error(error: slint::PlatformError) -> UiError {
    UiError::Platform(error.to_string())
}

struct ShellWindows<'a> {
    capture: &'a CapturePanel,
    items: &'a ItemsWindow,
    reminder: &'a ReminderPopup,
    settings: &'a Settings,
    screenshot: &'a ScreenshotOverlay,
}

fn apply_update(
    windows: ShellWindows<'_>,
    update: UiUpdate,
    screenshot_image: &mut CachedScreenshot,
    screenshot_shown: &mut bool,
    screenshot_route_pending: &mut bool,
    sender: &crate::UiIntentSender,
    route_screenshot_output: bool,
) {
    let ShellWindows {
        capture,
        items,
        reminder,
        settings,
        screenshot,
    } = windows;
    let UiUpdate { view, error, .. } = update;
    let screenshot_active = view.screenshot.active || view.screenshot.start_x.is_some();
    capture.set_draft(view.capture.draft.into());
    capture.set_message(view.capture.message.unwrap_or_default().into());
    capture.set_error(view.capture.error.as_deref().unwrap_or_default().into());

    let rows: Vec<ItemRowData> = view
        .items
        .rows
        .into_iter()
        .map(|row| ItemRowData {
            id: row.id.into(),
            title: row.title.into(),
            state: row.state.into(),
            waiting_reason: row.waiting_reason.into(),
            updated_at: row.updated_at.into(),
            synchronized: row.synchronized,
        })
        .collect();
    items.set_rows(ModelRc::new(VecModel::from(rows)));
    items.set_filter(view.items.filter as i32);
    items.set_summary(
        format!(
            "共 {} 条 · 待办 {} 条 · 等待 {} 条 · 已完成 {} 条 · 待同步 {} · 冲突 {}",
            view.items.total_count,
            view.items.active_count,
            view.items.waiting_count,
            view.items.completed_count,
            view.items.pending_count,
            view.items.conflict_count,
        )
        .into(),
    );
    if let Some(selected) = view.items.selected {
        let attachments: Vec<AttachmentData> = selected
            .attachments
            .into_iter()
            .map(|attachment| AttachmentData {
                id: attachment.id.into(),
                mime: attachment.mime.into(),
            })
            .collect();
        items.set_attachments(ModelRc::new(VecModel::from(attachments)));
        items.set_selected_id(selected.id.into());
        items.set_selected_title(selected.title.into());
        items.set_selected_body(selected.body_md.into());
        items.set_selected_state(selected.state.into());
        items.set_dirty(selected.dirty);
        items.set_synchronized(selected.synchronized);
        items.set_has_selected(true);
    } else {
        items.set_attachments(ModelRc::new(VecModel::from(Vec::<AttachmentData>::new())));
        items.set_selected_id("".into());
        items.set_selected_title("".into());
        items.set_selected_body("".into());
        items.set_selected_state("".into());
        items.set_has_selected(false);
    }
    items.set_error(error.or(view.items.error).unwrap_or_default().into());

    if let Some(popup) = view.reminder.filter(|_| !screenshot_active) {
        reminder.set_item_title(popup.title.into());
        let _ = reminder.show();
    } else {
        let _ = reminder.hide();
    }

    settings.set_server_url(view.settings.server_url.into());
    settings.set_enrollment_code(view.settings.enrollment_code.into());
    settings.set_device_name(view.settings.device_name.into());
    settings.set_registered(view.settings.registered);
    settings.set_autostart(view.settings.autostart);
    settings.set_message(view.settings.message.unwrap_or_default().into());
    settings.set_error(view.settings.error.unwrap_or_default().into());

    screenshot_image.apply(view.screenshot.background.as_deref(), screenshot);
    if screenshot_active {
        if !*screenshot_shown {
            *screenshot_shown = true;
            show_screenshot(
                screenshot,
                view.screenshot.display,
                route_screenshot_output,
                sender,
                screenshot_route_pending,
            );
        }
    } else {
        if *screenshot_shown && view.capture.error.is_some() {
            let _ = capture.show();
        }
        *screenshot_shown = false;
        *screenshot_route_pending = false;
        screenshot.set_output_ready(true);
        let _ = screenshot.hide();
    }
}

fn show_screenshot(
    screenshot: &ScreenshotOverlay,
    display: Option<crate::DisplayGeometry>,
    route_output: bool,
    sender: &crate::UiIntentSender,
    route_pending: &mut bool,
) {
    if route_output {
        if display.is_none() {
            screenshot_failed(screenshot, sender, "截图缺少显示器几何信息".to_owned());
            return;
        }
        screenshot.set_output_ready(false);
        match screenshot.show() {
            Ok(()) => *route_pending = true,
            Err(error) => {
                screenshot_failed(screenshot, sender, format!("无法创建截图叠加层：{error}"));
            }
        }
        return;
    }
    *route_pending = false;
    screenshot.set_output_ready(true);
    if let Some(display) = display {
        screenshot
            .window()
            .set_position(slint::LogicalPosition::new(
                display.x as f32,
                display.y as f32,
            ));
    }
    screenshot.window().set_fullscreen(true);
    let _ = screenshot.show();
}

fn screenshot_failed(
    screenshot: &ScreenshotOverlay,
    sender: &crate::UiIntentSender,
    reason: String,
) {
    let _ = screenshot.hide();
    let _ = sender.send(UiIntent::ScreenshotFailed(format!(
        "无法将截图叠加层定位到截图所在显示器：{reason}"
    )));
}

#[derive(Default)]
struct CachedScreenshot {
    path: Option<std::path::PathBuf>,
    image: slint::Image,
}

impl CachedScreenshot {
    fn apply(&mut self, path: Option<&std::path::Path>, screenshot: &ScreenshotOverlay) {
        if self.path.as_deref() != path {
            self.image =
                path.map_or_else(
                    slint::Image::default,
                    |path| match slint::Image::load_from_path(path) {
                        Ok(image) => image,
                        Err(error) => {
                            eprintln!("无法加载截图静态帧 {}: {error}", path.display());
                            slint::Image::default()
                        }
                    },
                );
            self.path = path.map(std::path::Path::to_path_buf);
        }
        screenshot.set_frame(self.image.clone());
    }
}
