use crate::{
    ItemFilter, OutputRoute, ScreenshotOutputRouter, ScreenshotWindowAction, ScreenshotWindowState,
    UiError, UiIntent, UiUpdate, UiWorker, ViewModel,
};
use aimemorandum_core::SubmitKey;
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
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
    last_view: Option<ViewModel>,
    ticks: u64,
    screenshot_output_router: Option<Box<dyn ScreenshotOutputRouter>>,
    screenshot_generation: Rc<Cell<u64>>,
}

impl SlintShell {
    pub fn new<C: crate::Controller + 'static>(controller: C) -> Result<Self, UiError> {
        let (sender, updates) = UiWorker::spawn(controller).into_parts();
        let capture = CapturePanel::new().map_err(platform_error)?;
        let items = ItemsWindow::new().map_err(platform_error)?;
        let reminder = ReminderPopup::new().map_err(platform_error)?;
        let settings = Settings::new().map_err(platform_error)?;
        let screenshot = ScreenshotOverlay::new().map_err(platform_error)?;
        let screenshot_generation = Rc::new(Cell::new(0_u64));

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
        screenshot.on_cancelled({
            let sender = sender.clone();
            let screenshot = screenshot.as_weak();
            let generation = Rc::clone(&screenshot_generation);
            move || {
                generation.set(generation.get().wrapping_add(1));
                if let Some(screenshot) = screenshot.upgrade() {
                    screenshot.set_output_ready(false);
                    hide_screenshot(&screenshot);
                }
                let _ = sender.send(UiIntent::ScreenshotCancelled);
            }
        });

        Ok(Self {
            capture,
            items,
            reminder,
            settings,
            screenshot,
            sender,
            updates,
            last_view: None,
            ticks: 0,
            screenshot_output_router: None,
            screenshot_generation,
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
            mut last_view,
            mut ticks,
            screenshot_output_router,
            screenshot_generation,
        } = self;
        if show_capture {
            show_and_focus_capture(&capture);
        }
        let timer = Timer::default();
        let mut screenshot_image = CachedScreenshot::default();
        let mut screenshot_state = ScreenshotWindowState::default();
        let mut screenshot_route_pending = None;
        let shell_screenshot_generation = screenshot_generation;
        let mut quitting = false;
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            if let Some(events) = events.as_mut() {
                while let Ok(event) = events.try_recv() {
                    match event {
                        ShellEvent::ShowCapture => {
                            show_and_focus_capture(&capture);
                        }
                        ShellEvent::ToggleCapture => {
                            if capture.window().is_visible() {
                                let _ = capture.hide();
                            } else {
                                show_and_focus_capture(&capture);
                            }
                        }
                        ShellEvent::Screenshot => {
                            // Invalidate old updates and delayed requests immediately. Only
                            // this generation may arm routing or report a routing failure.
                            let generation = shell_screenshot_generation.get().wrapping_add(1);
                            shell_screenshot_generation.set(generation);
                            screenshot_route_pending = None;
                            screenshot_state.invalidate_for_recapture();
                            screenshot.set_output_ready(false);
                            hide_screenshot(&screenshot);
                            let _ = capture.hide();
                            let _ = items.hide();
                            let _ = reminder.hide();
                            let _ = settings.hide();
                            let screenshot_sender = sender.clone();
                            let requested_generation = Rc::clone(&shell_screenshot_generation);
                            Timer::single_shot(Duration::from_millis(150), move || {
                                if requested_generation.get() == generation {
                                    let _ = screenshot_sender
                                        .send(UiIntent::BeginScreenshot { generation });
                                }
                            });
                        }
                        ShellEvent::SaveClipboard => {
                            show_and_focus_capture(&capture);
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
                                shell_screenshot_generation
                                    .set(shell_screenshot_generation.get().wrapping_add(1));
                                screenshot_route_pending = None;
                                screenshot_state.invalidate_for_recapture();
                                screenshot.set_output_ready(false);
                                let _ = capture.hide();
                                let _ = items.hide();
                                let _ = reminder.hide();
                                let _ = settings.hide();
                                hide_screenshot(&screenshot);
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
                        Some(&mut ScreenshotUpdateState {
                            image: &mut screenshot_image,
                            window: &mut screenshot_state,
                            route_pending: &mut screenshot_route_pending,
                            latest_generation: shell_screenshot_generation.get(),
                        }),
                        &sender,
                        screenshot_output_router.is_some(),
                    );
                }
            }
            // A delayed update from an older request can never re-arm routing.
            if let Some(route) = screenshot_route_pending {
                if route.generation != shell_screenshot_generation.get() {
                    screenshot_route_pending = None;
                } else {
                    let routed = match screenshot_output_router.as_ref() {
                        Some(router) => router.route(screenshot.window(), route.display),
                        None => Err("截图输出路由器不可用".to_owned()),
                    };
                    match routed {
                        Ok(OutputRoute::Pending) => {}
                        Ok(OutputRoute::Routed) => {
                            screenshot_route_pending = None;
                            screenshot.set_output_ready(true);
                        }
                        Err(error) => {
                            screenshot_route_pending = None;
                            screenshot_failed(&screenshot, &sender, route.generation, error);
                        }
                    }
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
        // Tray-only operation is a steady state: hiding the last window must not
        // terminate the primary process. Only the explicit shutdown flow above
        // may leave this event loop.
        slint::run_event_loop_until_quit().map_err(platform_error)
    }

    /// Apply pending worker updates and periodic work from the Slint UI thread.
    /// Returns the changed view so native shells can update tray projections.
    pub fn poll(&mut self) -> Option<ViewModel> {
        let mut latest = None;
        while let Ok(update) = self.updates.try_recv() {
            latest = Some(update);
        }
        let mut changed = None;
        if let Some(update) = latest {
            if self.last_view.as_ref() != Some(&update.view) || update.error.is_some() {
                self.last_view = Some(update.view.clone());
                changed = Some(update.view.clone());
                apply_update(
                    ShellWindows {
                        capture: &self.capture,
                        items: &self.items,
                        reminder: &self.reminder,
                        settings: &self.settings,
                        screenshot: &self.screenshot,
                    },
                    update,
                    None,
                    &self.sender,
                    false,
                );
            }
        }
        self.ticks += 1;
        if self.ticks.is_multiple_of(20) {
            let _ = self.sender.send(UiIntent::Refresh);
        }
        if self.ticks.is_multiple_of(100) {
            let _ = self.sender.send(UiIntent::CheckReminders);
        }
        changed
    }
}

fn show_and_focus_capture(capture: &CapturePanel) {
    let _ = capture.show();
    let capture = capture.as_weak();
    // X11 cannot focus an editor until its window has been mapped.
    Timer::single_shot(Duration::from_millis(50), move || {
        if let Some(capture) = capture.upgrade() {
            capture.invoke_focus_editor();
        }
    });
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

#[derive(Clone, Copy)]
struct PendingScreenshotRoute {
    generation: u64,
    display: crate::DisplayGeometry,
}

struct ScreenshotUpdateState<'a> {
    image: &'a mut CachedScreenshot,
    window: &'a mut ScreenshotWindowState,
    route_pending: &'a mut Option<PendingScreenshotRoute>,
    latest_generation: u64,
}

fn apply_update(
    windows: ShellWindows<'_>,
    update: UiUpdate,
    screenshot_update: Option<&mut ScreenshotUpdateState<'_>>,
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
    let screenshot_active = screenshot_update.is_some()
        && (view.screenshot.active || view.screenshot.start_x.is_some());
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
    if let Some(screenshot_update) = screenshot_update {
        let ScreenshotUpdateState {
            image: screenshot_image,
            window: screenshot_state,
            route_pending: screenshot_route_pending,
            latest_generation: latest_screenshot_generation,
        } = screenshot_update;
        let screenshot_image = &mut *screenshot_image;
        let screenshot_state = &mut *screenshot_state;
        let screenshot_route_pending = &mut **screenshot_route_pending;
        let latest_screenshot_generation = *latest_screenshot_generation;
        let screenshot_current = view.screenshot.generation == latest_screenshot_generation;
        let screenshot_background = screenshot_current
            .then_some(view.screenshot.background.as_deref())
            .flatten();
        match screenshot_state.synchronize_current(
            screenshot_active,
            view.screenshot.display,
            view.screenshot.generation,
            latest_screenshot_generation,
        ) {
            ScreenshotWindowAction::Show => {
                screenshot_image.apply(screenshot_background, screenshot);
                show_screenshot(
                    screenshot,
                    view.screenshot.display,
                    route_screenshot_output,
                    sender,
                    screenshot_route_pending,
                    view.screenshot.generation,
                );
            }
            ScreenshotWindowAction::Hide => {
                if view.capture.error.is_some() {
                    let _ = capture.show();
                }
                *screenshot_route_pending = None;
                screenshot.set_output_ready(true);
                hide_screenshot(screenshot);
                // Destroy the window's renderer resources before releasing the frame.
                screenshot_image.apply(None, screenshot);
            }
            ScreenshotWindowAction::None => {
                screenshot_image.apply(screenshot_background, screenshot);
            }
        }
    }
}

fn show_screenshot(
    screenshot: &ScreenshotOverlay,
    display: Option<crate::DisplayGeometry>,
    route_output: bool,
    sender: &crate::UiIntentSender,
    route_pending: &mut Option<PendingScreenshotRoute>,
    generation: u64,
) {
    if route_output {
        let Some(display) = display else {
            screenshot_failed(
                screenshot,
                sender,
                generation,
                "截图缺少显示器几何信息".to_owned(),
            );
            return;
        };
        screenshot.set_output_ready(false);
        match screenshot.show() {
            Ok(()) => {
                *route_pending = Some(PendingScreenshotRoute {
                    generation,
                    display,
                });
            }
            Err(error) => {
                screenshot_failed(
                    screenshot,
                    sender,
                    generation,
                    format!("无法创建截图叠加层：{error}"),
                );
            }
        }
        return;
    }
    *route_pending = None;
    screenshot.set_output_ready(true);
    if let Some(display) = display {
        screenshot
            .window()
            .set_position(slint::LogicalPosition::new(
                display.x as f32,
                display.y as f32,
            ));
    }
    let _ = screenshot.show();
    screenshot.window().set_fullscreen(true);
}

fn hide_screenshot(screenshot: &ScreenshotOverlay) {
    screenshot.window().set_fullscreen(false);
    let _ = screenshot.hide();
}

fn screenshot_failed(
    screenshot: &ScreenshotOverlay,
    sender: &crate::UiIntentSender,
    generation: u64,
    reason: String,
) {
    hide_screenshot(screenshot);
    let _ = sender.send(UiIntent::ScreenshotFailed {
        generation,
        message: format!("无法将截图叠加层定位到截图所在显示器：{reason}"),
    });
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
