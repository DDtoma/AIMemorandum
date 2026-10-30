use crate::{ItemFilter, UiError, UiIntent, UiUpdate, UiWorker, ViewModel};
use aimemorandum_core::SubmitKey;
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::path::PathBuf;
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

pub struct SlintShell {
    pub capture: CapturePanel,
    pub items: ItemsWindow,
    pub reminder: ReminderPopup,
    pub settings: Settings,
    pub screenshot: ScreenshotOverlay,
    sender: crate::UiIntentSender,
    updates: std::sync::mpsc::Receiver<UiUpdate>,
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
        capture.on_show_settings({
            let settings = settings.as_weak();
            move || {
                if let Some(settings) = settings.upgrade() {
                    let _ = settings.show();
                }
            }
        });

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
        items.on_show_settings({
            let settings = settings.as_weak();
            move || {
                if let Some(settings) = settings.upgrade() {
                    let _ = settings.show();
                }
            }
        });

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
        })
    }

    pub fn sender(&self) -> &crate::UiIntentSender {
        &self.sender
    }

    pub fn run(self) -> Result<(), UiError> {
        let Self {
            capture,
            items,
            reminder,
            settings,
            screenshot,
            sender,
            updates,
        } = self;
        capture.show().map_err(platform_error)?;
        let timer = Timer::default();
        let mut last_view: Option<ViewModel> = None;
        let mut ticks = 0_u64;
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let mut latest = None;
            while let Ok(update) = updates.try_recv() {
                latest = Some(update);
            }
            if let Some(update) = latest {
                if last_view.as_ref() != Some(&update.view) || update.error.is_some() {
                    last_view = Some(update.view.clone());
                    apply_update(&capture, &items, &reminder, &settings, &screenshot, update);
                }
            }
            ticks += 1;
            if ticks % 20 == 0 {
                let _ = sender.send(UiIntent::Refresh);
            }
            if ticks % 100 == 0 {
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

fn apply_update(
    capture: &CapturePanel,
    items: &ItemsWindow,
    reminder: &ReminderPopup,
    settings: &Settings,
    screenshot: &ScreenshotOverlay,
    update: UiUpdate,
) {
    let UiUpdate { view, error } = update;
    capture.set_draft(view.capture.draft.into());
    capture.set_message(view.capture.message.unwrap_or_default().into());
    capture.set_error(view.capture.error.unwrap_or_default().into());

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

    if let Some(popup) = view.reminder {
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

    if view.screenshot.start_x.is_some() {
        let _ = screenshot.show();
    } else {
        let _ = screenshot.hide();
    }
}
