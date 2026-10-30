use crate::{
    AttachmentModel, CaptureRegion, Clipboard, Clock, DesktopShell, Enrollment, ItemDetailModel,
    ItemFilter, ItemRowModel, ReminderPopupModel, Screenshot, TrayEntry, UiError, UiIntent,
    ViewModel,
};
use aimemorandum_core::{
    Attachment, InputPolicy, Item, ItemKind, LocalStore, ReminderScheduler, StoreError, TaskState,
};
use chrono::Duration;
use image::ImageReader;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct UiController<S, C, E, D, K> {
    store: LocalStore,
    #[allow(dead_code)]
    screenshot: S,
    #[allow(dead_code)]
    clipboard: C,
    #[allow(dead_code)]
    enrollment: E,
    #[allow(dead_code)]
    shell: D,
    clock: K,
    view: ViewModel,
}

impl<S, C, E, D, K> UiController<S, C, E, D, K>
where
    S: Screenshot,
    C: Clipboard,
    E: Enrollment,
    D: DesktopShell,
    K: Clock,
{
    pub fn new(
        store: LocalStore,
        screenshot: S,
        clipboard: C,
        enrollment: E,
        shell: D,
        clock: K,
    ) -> Result<Self, UiError> {
        let mut controller = Self {
            store,
            screenshot,
            clipboard,
            enrollment,
            shell,
            clock,
            view: ViewModel::default(),
        };
        controller.load_settings()?;
        controller.refresh_items()?;
        Ok(controller)
    }

    pub fn view(&self) -> &ViewModel {
        &self.view
    }

    pub fn dispatch(&mut self, intent: UiIntent) -> Result<(), UiError> {
        match intent {
            UiIntent::CaptureDraftChanged(draft) => {
                self.view.capture.draft = draft;
                self.view.capture.error = None;
                self.view.capture.message = None;
                Ok(())
            }
            UiIntent::CaptureKey { key, shift, marked } => {
                if InputPolicy::should_submit(key, shift, marked) {
                    self.save_capture("text", Vec::new())
                } else {
                    Ok(())
                }
            }
            UiIntent::SubmitCapture => self.save_capture("text", Vec::new()),
            UiIntent::CaptureClipboard => self.capture_clipboard(),
            UiIntent::DropImages(paths) => self.drop_images(paths),
            UiIntent::Refresh => self.refresh_items(),
            UiIntent::SelectItem(id) => self.select_item(&id),
            UiIntent::SetFilter(filter) => {
                if self
                    .view
                    .items
                    .selected
                    .as_ref()
                    .is_some_and(|item| item.dirty)
                    && self.view.items.filter != filter
                {
                    return Err(UiError::UnsavedChanges);
                }
                self.view.items.filter = filter;
                self.view.items.selected = None;
                self.refresh_items()
            }
            UiIntent::DetailDraftChanged { title, body_md } => {
                let selected = self
                    .view
                    .items
                    .selected
                    .as_mut()
                    .ok_or_else(|| UiError::InvalidState("请先选择一条记录".to_owned()))?;
                selected.title = title;
                selected.body_md = body_md;
                selected.dirty = true;
                Ok(())
            }
            UiIntent::SaveSelected => self.save_selected(),
            UiIntent::CompleteItem(id) => {
                let result = self.store.complete(&id, self.clock.now());
                self.apply_item_result(result)
            }
            UiIntent::ReopenItem(id) => self.reopen_item(&id),
            UiIntent::DeleteItem(id) => {
                let result = self.store.delete(&id, self.clock.now());
                self.apply_item_result(result)
            }
            UiIntent::RemindItem { item_id, remind_at } => {
                let result =
                    self.store
                        .set_reminder(&item_id, remind_at, None, None, self.clock.now());
                self.apply_item_result(result)
            }
            UiIntent::RemindItemInTwoHours(item_id) => {
                let now = self.clock.now();
                let result =
                    self.store
                        .set_reminder(&item_id, now + Duration::hours(2), None, None, now);
                self.apply_item_result(result)
            }
            UiIntent::CheckReminders => self.check_reminders(),
            UiIntent::CompleteReminder => self.complete_reminder(),
            UiIntent::SnoozeReminder => self.snooze_reminder(),
            UiIntent::DismissReminder => {
                self.view.reminder = None;
                Ok(())
            }
            UiIntent::SettingsChanged {
                server_url,
                enrollment_code,
                device_name,
            } => {
                self.view.settings.server_url = server_url.trim().to_owned();
                self.view.settings.enrollment_code = enrollment_code.trim().to_owned();
                self.view.settings.device_name = device_name;
                self.view.settings.error = None;
                self.view.settings.message = None;
                Ok(())
            }
            UiIntent::Register => self.register(),
            UiIntent::SetAutostart(enabled) => self.set_autostart(enabled),
            UiIntent::OpenAttachment {
                item_id,
                attachment_id,
            } => self.open_attachment(&item_id, &attachment_id),
            UiIntent::BeginScreenshot => self.begin_screenshot(),
            UiIntent::ScreenshotStarted { x, y } => {
                self.view.screenshot.start_x = Some(x);
                self.view.screenshot.start_y = Some(y);
                self.view.screenshot.current_x = x;
                self.view.screenshot.current_y = y;
                Ok(())
            }
            UiIntent::ScreenshotMoved { x, y } => {
                if self.view.screenshot.start_x.is_some() {
                    self.view.screenshot.current_x = x;
                    self.view.screenshot.current_y = y;
                }
                Ok(())
            }
            UiIntent::ScreenshotFinished { x, y } => self.finish_screenshot(x, y),
            UiIntent::ScreenshotCancelled => {
                self.screenshot.cancel_capture();
                self.view.screenshot = Default::default();
                Ok(())
            }
        }
    }

    fn save_capture(&mut self, method: &str, sources: Vec<ImageSource>) -> Result<(), UiError> {
        let text = self.view.capture.draft.clone();
        self.save_content(method, text, sources, true)
    }

    fn save_content(
        &mut self,
        method: &str,
        text: String,
        sources: Vec<ImageSource>,
        clear_draft: bool,
    ) -> Result<(), UiError> {
        let attachments: Vec<Attachment> = sources
            .iter()
            .map(|source| source.attachment.clone())
            .collect();
        let result = Item::new_task(
            text,
            method,
            self.clock.now(),
            self.clock.timezone(),
            attachments,
        )
        .map_err(UiError::from)
        .and_then(|item| {
            if sources.is_empty() {
                self.store.create(item).map_err(UiError::from)
            } else {
                let source_paths: Vec<(String, PathBuf)> = sources
                    .iter()
                    .map(|source| (source.attachment.id.clone(), source.path.clone()))
                    .collect();
                let staged = self.store.stage_create(item, &source_paths)?;
                self.store.commit_staged(staged).map_err(UiError::from)
            }
        });
        match result {
            Ok(_) => {
                if clear_draft {
                    self.view.capture.draft.clear();
                }
                self.view.capture.error = None;
                self.view.capture.message = Some("已保存".to_owned());
                self.refresh_items()
            }
            Err(error) => {
                self.view.capture.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn load_settings(&mut self) -> Result<(), UiError> {
        if let Some(value) = self.store.meta("server_url")? {
            self.view.settings.server_url = value;
        }
        if let Some(value) = self.store.meta("device_name")? {
            self.view.settings.device_name = value;
        }
        self.view.settings.registered = self.store.meta("device_token")?.is_some();
        self.view.settings.autostart = self.store.meta("autostart")?.as_deref() == Some("true");
        Ok(())
    }

    fn refresh_items(&mut self) -> Result<(), UiError> {
        let selected_id = self
            .view
            .items
            .selected
            .as_ref()
            .map(|item| item.id.clone());
        let selected_was_dirty = self
            .view
            .items
            .selected
            .as_ref()
            .is_some_and(|item| item.dirty);
        let alive: Vec<Item> = self
            .store
            .items()?
            .into_iter()
            .filter(|item| item.deleted_at.is_none())
            .collect();
        let pending = self.store.pending()?;
        let pending_ids: HashSet<&str> = pending
            .iter()
            .map(|operation| operation.item_id.as_str())
            .collect();

        self.view.items.total_count = alive.len();
        self.view.items.active_count = alive.iter().filter(|item| is_open_task(item)).count();
        self.view.items.waiting_count = alive
            .iter()
            .filter(|item| item.task_state == Some(TaskState::Waiting))
            .count();
        self.view.items.completed_count = alive
            .iter()
            .filter(|item| item.task_state == Some(TaskState::Completed))
            .count();
        self.view.items.pending_count = pending.len();
        self.view.items.conflict_count = self.store.conflict_items()?.len();

        let filter = self.view.items.filter;
        let mut displayed: Vec<&Item> = alive
            .iter()
            .filter(|item| match filter {
                ItemFilter::All => true,
                ItemFilter::Active => is_open_task(item),
                ItemFilter::Waiting => item.task_state == Some(TaskState::Waiting),
                ItemFilter::Completed => item.task_state == Some(TaskState::Completed),
            })
            .collect();
        displayed.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        self.view.items.rows = displayed
            .iter()
            .map(|item| ItemRowModel {
                id: item.id.clone(),
                title: item.title.clone(),
                state: state_name(item),
                waiting_reason: item.resume_note.clone(),
                updated_at: item.updated_at.clone(),
                synchronized: is_synchronized(item, pending_ids.contains(item.id.as_str())),
            })
            .collect();

        let selected_item = selected_id.as_deref().and_then(|id| {
            alive.iter().find(|item| {
                item.id == id
                    && (selected_was_dirty
                        || self.view.items.rows.iter().any(|row| row.id == item.id))
            })
        });
        self.view.items.selected = selected_item.map(|item| {
            let mut detail = detail_model(
                item,
                is_synchronized(item, pending_ids.contains(item.id.as_str())),
            );
            if selected_was_dirty {
                if let Some(previous) = self.view.items.selected.take() {
                    detail.title = previous.title;
                    detail.body_md = previous.body_md;
                    detail.dirty = true;
                }
            }
            detail
        });

        let mut tray_items: Vec<&Item> = alive.iter().filter(|item| is_open_task(item)).collect();
        tray_items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        self.view.tray.open_count = tray_items.len();
        self.view.tray.entries = tray_items
            .into_iter()
            .take(12)
            .map(|item| TrayEntry {
                id: item.id.clone(),
                title: tray_title(&item.title),
            })
            .collect();
        Ok(())
    }

    fn select_item(&mut self, id: &str) -> Result<(), UiError> {
        if self
            .view
            .items
            .selected
            .as_ref()
            .is_some_and(|selected| selected.dirty && selected.id != id)
        {
            return Err(UiError::UnsavedChanges);
        }
        if !self.view.items.rows.iter().any(|row| row.id == id) {
            return Err(UiError::NotFound(id.to_owned()));
        }
        let item = self
            .store
            .item(id)?
            .ok_or_else(|| UiError::NotFound(id.to_owned()))?;
        let has_pending = self
            .store
            .pending()?
            .iter()
            .any(|operation| operation.item_id == id);
        let synchronized = is_synchronized(&item, has_pending);
        self.view.items.selected = Some(detail_model(&item, synchronized));
        Ok(())
    }

    fn reopen_item(&mut self, id: &str) -> Result<(), UiError> {
        let item = self
            .store
            .item(id)?
            .ok_or_else(|| UiError::NotFound(id.to_owned()))?;
        if item.task_state != Some(TaskState::Canceled) {
            let error = UiError::InvalidState("只有已取消的待办可以重新打开".to_owned());
            self.view.items.error = Some(error.to_string());
            return Err(error);
        }
        let result = self.store.reopen(id, self.clock.now());
        self.apply_item_result(result)
    }

    fn save_selected(&mut self) -> Result<(), UiError> {
        let selected = self
            .view
            .items
            .selected
            .clone()
            .ok_or_else(|| UiError::InvalidState("请先选择一条记录".to_owned()))?;
        let result = self.store.update(
            &selected.id,
            &selected.title,
            &selected.body_md,
            self.clock.now(),
        );
        if result.is_ok() {
            if let Some(current) = self.view.items.selected.as_mut() {
                current.dirty = false;
            }
        }
        self.apply_item_result(result)
    }

    fn drop_images(&mut self, paths: Vec<PathBuf>) -> Result<(), UiError> {
        let sources = paths
            .iter()
            .enumerate()
            .map(|(index, path)| image_source(path, index as u32))
            .collect::<Result<Vec<_>, _>>();
        match sources {
            Ok(sources) => self.save_capture("drop", sources),
            Err(error) => {
                self.view.capture.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn capture_clipboard(&mut self) -> Result<(), UiError> {
        if let Some(image) = self.clipboard.read_image()? {
            let source = image_source(&image.path, 0)?;
            return self.save_capture("clipboard", vec![source]);
        }
        if let Some(text) = self.clipboard.read_text()? {
            return self.save_content("clipboard", text, Vec::new(), false);
        }
        let error = UiError::InvalidState("剪贴板没有可保存的文本或图片".to_owned());
        self.view.capture.error = Some(error.to_string());
        Err(error)
    }

    fn begin_screenshot(&mut self) -> Result<(), UiError> {
        let image = self.screenshot.capture_full()?;
        self.view.screenshot.active = true;
        self.view.screenshot.background = Some(image.path);
        Ok(())
    }

    fn finish_screenshot(&mut self, x: f64, y: f64) -> Result<(), UiError> {
        let start = self
            .view
            .screenshot
            .start_x
            .zip(self.view.screenshot.start_y);
        self.view.screenshot = Default::default();
        let Some((start_x, start_y)) = start else {
            self.screenshot.cancel_capture();
            return Ok(());
        };
        self.capture_region(normalized_region(start_x, start_y, x, y))
    }

    fn capture_region(&mut self, region: CaptureRegion) -> Result<(), UiError> {
        if !region.x.is_finite()
            || !region.y.is_finite()
            || !region.width.is_finite()
            || !region.height.is_finite()
        {
            self.screenshot.cancel_capture();
            return Err(UiError::InvalidState("截图坐标必须是有限数值".to_owned()));
        }
        if region.width < 8.0 || region.height < 8.0 {
            self.screenshot.cancel_capture();
            return Ok(());
        }
        let image = self.screenshot.capture(region)?;
        let source = image_source(&image.path, 0)?;
        self.save_capture("screenshot", vec![source])
    }

    fn open_attachment(&mut self, item_id: &str, attachment_id: &str) -> Result<(), UiError> {
        let item = self
            .store
            .item(item_id)?
            .ok_or_else(|| UiError::NotFound(item_id.to_owned()))?;
        let attachment = item
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or_else(|| UiError::NotFound(attachment_id.to_owned()))?;
        self.shell
            .open_attachment(&self.store.attachment_path(attachment))
    }

    fn check_reminders(&mut self) -> Result<(), UiError> {
        if self.view.reminder.is_some() {
            return Ok(());
        }
        let scheduler = ReminderScheduler::new(&self.store);
        if let Some(due) = scheduler.due(self.clock.now())?.into_iter().next() {
            scheduler.mark_fired(&due.item, &due.reminder)?;
            self.view.reminder = Some(ReminderPopupModel {
                item_id: due.item.id,
                reminder_id: due.reminder.id,
                expected_schedule: due.reminder.schedule_revision,
                title: due.item.title,
            });
        }
        Ok(())
    }

    fn complete_reminder(&mut self) -> Result<(), UiError> {
        let popup = self
            .view
            .reminder
            .clone()
            .ok_or_else(|| UiError::InvalidState("当前没有提醒".to_owned()))?;
        let result = self.store.complete(&popup.item_id, self.clock.now());
        if result.is_ok() {
            self.view.reminder = None;
        }
        self.apply_item_result(result)
    }

    fn snooze_reminder(&mut self) -> Result<(), UiError> {
        let popup = self
            .view
            .reminder
            .clone()
            .ok_or_else(|| UiError::InvalidState("当前没有提醒".to_owned()))?;
        let now = self.clock.now();
        let result = self.store.set_reminder(
            &popup.item_id,
            now + Duration::seconds(600),
            Some(&popup.reminder_id),
            Some(&popup.expected_schedule),
            now,
        );
        if result.is_ok() {
            self.view.reminder = None;
        }
        self.apply_item_result(result)
    }

    fn register(&mut self) -> Result<(), UiError> {
        let result = self.enrollment.enroll(
            &self.view.settings.server_url,
            &self.view.settings.enrollment_code,
            &self.view.settings.device_name,
        );
        match result {
            Ok(credentials) => {
                self.store
                    .set_meta("server_url", &self.view.settings.server_url)?;
                self.store
                    .set_meta("device_token", &credentials.device_token)?;
                self.store
                    .set_meta("device_name", &credentials.device_name)?;
                self.view.settings.device_name = credentials.device_name;
                self.view.settings.registered = true;
                self.view.settings.enrollment_code.clear();
                self.view.settings.error = None;
                self.view.settings.message = Some("注册成功".to_owned());
                Ok(())
            }
            Err(error) => {
                self.view.settings.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn set_autostart(&mut self, enabled: bool) -> Result<(), UiError> {
        if let Err(error) = self.shell.set_autostart(enabled) {
            self.view.settings.error = Some(error.to_string());
            return Err(error);
        }
        self.store
            .set_meta("autostart", if enabled { "true" } else { "false" })?;
        self.view.settings.autostart = enabled;
        self.view.settings.error = None;
        Ok(())
    }

    fn apply_item_result(&mut self, result: Result<Item, StoreError>) -> Result<(), UiError> {
        match result {
            Ok(_) => {
                self.view.items.error = None;
                self.refresh_items()
            }
            Err(error) => {
                let error = UiError::from(error);
                self.view.items.error = Some(error.to_string());
                Err(error)
            }
        }
    }
}

fn is_open_task(item: &Item) -> bool {
    item.kind == ItemKind::Task && !item.ended()
}

fn is_synchronized(item: &Item, has_pending: bool) -> bool {
    !has_pending
        && item
            .attachments
            .iter()
            .all(|attachment| attachment.state == "ready")
}

fn state_name(item: &Item) -> String {
    match item.task_state {
        Some(TaskState::Todo) => "todo",
        Some(TaskState::InProgress) => "in_progress",
        Some(TaskState::Waiting) => "waiting",
        Some(TaskState::Paused) => "paused",
        Some(TaskState::Completed) => "completed",
        Some(TaskState::Canceled) => "canceled",
        None => match item.kind {
            ItemKind::Capture => "capture",
            ItemKind::Memo => "memo",
            ItemKind::Task => "unknown",
        },
    }
    .to_owned()
}

fn detail_model(item: &Item, synchronized: bool) -> ItemDetailModel {
    ItemDetailModel {
        id: item.id.clone(),
        title: item.title.clone(),
        body_md: item.body_md.clone(),
        state: state_name(item),
        waiting_reason: item.resume_note.clone(),
        synchronized,
        attachments: item
            .attachments
            .iter()
            .map(|attachment| AttachmentModel {
                id: attachment.id.clone(),
                mime: attachment.mime.clone(),
            })
            .collect(),
        dirty: false,
    }
}

fn tray_title(title: &str) -> String {
    if title.chars().count() > 36 {
        format!("{}…", title.chars().take(36).collect::<String>())
    } else {
        title.to_owned()
    }
}

struct ImageSource {
    attachment: Attachment,
    path: PathBuf,
}

fn image_source(path: &Path, position: u32) -> Result<ImageSource, UiError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mime = match reader.format() {
        Some(image::ImageFormat::Png) => "image/png",
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        _ => return Err(UiError::InvalidState("仅支持 PNG 和 JPEG 图片".to_owned())),
    };
    let (pixel_width, pixel_height) = reader.into_dimensions()?;
    let bytes = std::fs::read(path)?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let attachment = Attachment::new(
        mime,
        bytes.len() as u64,
        sha256,
        pixel_width,
        pixel_height,
        position,
    );
    Ok(ImageSource {
        attachment,
        path: path.to_owned(),
    })
}

fn normalized_region(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> CaptureRegion {
    CaptureRegion {
        x: start_x.min(end_x),
        y: start_y.min(end_y),
        width: (end_x - start_x).abs(),
        height: (end_y - start_y).abs(),
    }
}
