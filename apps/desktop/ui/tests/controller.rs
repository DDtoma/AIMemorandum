use aimemorandum_core::{Attachment, Item, ItemKind, LocalStore, SubmitKey, TaskState};
use aimemorandum_ui::{
    CaptureRegion, CapturedImage, Clipboard, Clock, DesktopShell, DeviceCredentials, Enrollment,
    ItemFilter, Screenshot, UiController, UiError, UiIntent,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }

    fn timezone(&self) -> String {
        "UTC".to_owned()
    }
}

struct NoScreenshot;
impl Screenshot for NoScreenshot {
    fn capture(&self, _region: CaptureRegion) -> Result<CapturedImage, UiError> {
        unreachable!()
    }
}

struct NoClipboard;
impl Clipboard for NoClipboard {
    fn read_text(&self) -> Result<Option<String>, UiError> {
        Ok(None)
    }

    fn read_image(&self) -> Result<Option<CapturedImage>, UiError> {
        Ok(None)
    }
}

struct NoEnrollment;
impl Enrollment for NoEnrollment {
    fn enroll(
        &self,
        _server_url: &str,
        _enrollment_code: &str,
        _device_name: &str,
    ) -> Result<DeviceCredentials, UiError> {
        unreachable!()
    }
}

struct NoShell;
impl DesktopShell for NoShell {
    fn set_autostart(&self, _enabled: bool) -> Result<(), UiError> {
        Ok(())
    }

    fn open_attachment(&self, _path: &Path) -> Result<(), UiError> {
        Ok(())
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 2, 8, 0, 0).unwrap()
}

fn controller(
    root: &Path,
) -> UiController<NoScreenshot, NoClipboard, NoEnrollment, NoShell, FixedClock> {
    let store = LocalStore::open(root).unwrap();
    UiController::new(
        store,
        NoScreenshot,
        NoClipboard,
        NoEnrollment,
        NoShell,
        FixedClock(now()),
    )
    .unwrap()
}

#[test]
fn enter_submits_only_when_not_shifted_or_marked() {
    let directory = tempfile::tempdir().unwrap();
    let mut ui = controller(directory.path());
    ui.dispatch(UiIntent::CaptureDraftChanged("中文记录".to_owned()))
        .unwrap();

    ui.dispatch(UiIntent::CaptureKey {
        key: SubmitKey::Enter,
        shift: false,
        marked: true,
    })
    .unwrap();
    assert_eq!(ui.view().capture.draft, "中文记录");

    ui.dispatch(UiIntent::CaptureKey {
        key: SubmitKey::Enter,
        shift: true,
        marked: false,
    })
    .unwrap();
    assert_eq!(ui.view().capture.draft, "中文记录");

    ui.dispatch(UiIntent::CaptureKey {
        key: SubmitKey::NumpadEnter,
        shift: false,
        marked: false,
    })
    .unwrap();
    assert_eq!(ui.view().capture.draft, "");
    assert_eq!(
        LocalStore::open(directory.path())
            .unwrap()
            .items()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn successful_capture_clears_draft_but_failed_capture_retains_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut ui = controller(directory.path());
    ui.dispatch(UiIntent::CaptureDraftChanged("先失败".to_owned()))
        .unwrap();
    ui.dispatch(UiIntent::CaptureDraftChanged("   ".to_owned()))
        .unwrap();

    assert!(ui.dispatch(UiIntent::SubmitCapture).is_err());
    assert_eq!(ui.view().capture.draft, "   ");
    assert!(ui.view().capture.error.is_some());

    ui.dispatch(UiIntent::CaptureDraftChanged("本地优先保存".to_owned()))
        .unwrap();
    ui.dispatch(UiIntent::SubmitCapture).unwrap();

    assert_eq!(ui.view().capture.draft, "");
    let items = LocalStore::open(directory.path()).unwrap().items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].body_md, "本地优先保存");
    assert_eq!(items[0].source.input_method, "text");
}

fn seed(root: &Path, title: &str, state: TaskState, offset_seconds: i64) -> String {
    let store = LocalStore::open(root).unwrap();
    let mut item = Item::new_task(
        title,
        "text",
        now() + Duration::seconds(offset_seconds),
        "UTC",
        vec![],
    )
    .unwrap();
    item.created_at = (now() + Duration::seconds(offset_seconds)).to_rfc3339();
    item.task_state = Some(state);
    let id = item.id.clone();
    store.create(item).unwrap();
    id
}

#[test]
fn filters_counts_and_refresh_keep_selection_by_stable_id() {
    let directory = tempfile::tempdir().unwrap();
    let active = seed(directory.path(), "进行中", TaskState::Todo, 1);
    seed(directory.path(), "等待", TaskState::Waiting, 2);
    seed(directory.path(), "已完成", TaskState::Completed, 3);
    let mut ui = controller(directory.path());

    assert_eq!(ui.view().items.total_count, 3);
    assert_eq!(ui.view().items.active_count, 2);
    assert_eq!(ui.view().items.waiting_count, 1);
    assert_eq!(ui.view().items.completed_count, 1);
    assert_eq!(ui.view().items.rows[0].title, "已完成");

    ui.dispatch(UiIntent::SetFilter(ItemFilter::Active))
        .unwrap();
    assert_eq!(ui.view().items.rows.len(), 2);
    ui.dispatch(UiIntent::SelectItem(active.clone())).unwrap();
    seed(directory.path(), "新的进行中", TaskState::Todo, 4);
    ui.dispatch(UiIntent::Refresh).unwrap();
    assert_eq!(ui.view().items.selected.as_ref().unwrap().id, active);
    assert_eq!(ui.view().items.rows.len(), 3);

    ui.dispatch(UiIntent::SetFilter(ItemFilter::Waiting))
        .unwrap();
    assert!(ui.view().items.selected.is_none());
    assert_eq!(ui.view().items.rows.len(), 1);
    ui.dispatch(UiIntent::SetFilter(ItemFilter::Completed))
        .unwrap();
    assert_eq!(ui.view().items.rows.len(), 1);
}

#[test]
fn refresh_does_not_replace_dirty_detail_and_save_persists_draft() {
    let directory = tempfile::tempdir().unwrap();
    let id = seed(directory.path(), "原标题", TaskState::Todo, 1);
    let mut ui = controller(directory.path());
    ui.dispatch(UiIntent::SelectItem(id.clone())).unwrap();
    ui.dispatch(UiIntent::DetailDraftChanged {
        title: "未保存标题".to_owned(),
        body_md: "未保存正文".to_owned(),
    })
    .unwrap();

    let store = LocalStore::open(directory.path()).unwrap();
    store
        .update(&id, "外部标题", "外部正文", now() + Duration::seconds(1))
        .unwrap();
    ui.dispatch(UiIntent::Refresh).unwrap();
    let selected = ui.view().items.selected.as_ref().unwrap();
    assert_eq!(selected.title, "未保存标题");
    assert_eq!(selected.body_md, "未保存正文");
    assert!(selected.dirty);

    ui.dispatch(UiIntent::SaveSelected).unwrap();
    assert!(!ui.view().items.selected.as_ref().unwrap().dirty);
    assert_eq!(store.item(&id).unwrap().unwrap().title, "未保存标题");
}

#[test]
fn complete_reopen_remind_and_delete_use_real_store() {
    let directory = tempfile::tempdir().unwrap();
    let id = seed(directory.path(), "日常记录", TaskState::Todo, 1);
    let mut ui = controller(directory.path());
    let store = LocalStore::open(directory.path()).unwrap();

    ui.dispatch(UiIntent::RemindItemInTwoHours(id.clone()))
        .unwrap();
    let reminded = store.item(&id).unwrap().unwrap();
    assert_eq!(reminded.reminders.len(), 1);
    assert_eq!(
        reminded.reminders[0].remind_at,
        (now() + Duration::hours(2)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    ui.dispatch(UiIntent::CompleteItem(id.clone())).unwrap();
    assert_eq!(
        store.item(&id).unwrap().unwrap().task_state,
        Some(TaskState::Completed)
    );
    assert!(ui.dispatch(UiIntent::ReopenItem(id.clone())).is_err());
    assert_eq!(
        store.item(&id).unwrap().unwrap().task_state,
        Some(TaskState::Completed)
    );

    let canceled = seed(directory.path(), "已取消", TaskState::Canceled, 2);
    ui.dispatch(UiIntent::ReopenItem(canceled.clone())).unwrap();
    assert_eq!(
        store.item(&canceled).unwrap().unwrap().task_state,
        Some(TaskState::Todo)
    );

    ui.dispatch(UiIntent::DeleteItem(id.clone())).unwrap();
    assert!(store.item(&id).unwrap().unwrap().deleted_at.is_some());
    assert!(ui.view().items.rows.iter().all(|row| row.id != id));
}

#[test]
fn tray_is_oldest_first_limited_to_twelve_and_truncates_unicode_chars() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..15 {
        let title = if index == 0 {
            "记".repeat(40)
        } else {
            format!("记录 {index}")
        };
        seed(directory.path(), &title, TaskState::Todo, index);
    }
    let ui = controller(directory.path());
    assert_eq!(ui.view().tray.open_count, 15);
    assert_eq!(ui.view().tray.entries.len(), 12);
    assert_eq!(
        ui.view().tray.entries[0].title,
        format!("{}…", "记".repeat(36))
    );
    assert_eq!(ui.view().tray.entries[1].title, "记录 1");
    assert_eq!(ui.view().tray.entries[11].title, "记录 11");
}

#[derive(Clone)]
struct FakeEnrollment {
    calls: std::sync::Arc<std::sync::Mutex<Vec<(String, String, String)>>>,
    fail: bool,
}

impl Enrollment for FakeEnrollment {
    fn enroll(
        &self,
        server_url: &str,
        enrollment_code: &str,
        device_name: &str,
    ) -> Result<DeviceCredentials, UiError> {
        self.calls.lock().unwrap().push((
            server_url.to_owned(),
            enrollment_code.to_owned(),
            device_name.to_owned(),
        ));
        if self.fail {
            return Err(UiError::Platform("注册失败".to_owned()));
        }
        Ok(DeviceCredentials {
            device_token: "secret-device-token".to_owned(),
            device_name: device_name.to_owned(),
        })
    }
}

#[derive(Clone, Default)]
struct RecordingShell {
    autostart: std::sync::Arc<std::sync::Mutex<Vec<bool>>>,
}

impl DesktopShell for RecordingShell {
    fn set_autostart(&self, enabled: bool) -> Result<(), UiError> {
        self.autostart.lock().unwrap().push(enabled);
        Ok(())
    }

    fn open_attachment(&self, _path: &Path) -> Result<(), UiError> {
        Ok(())
    }
}

#[test]
fn reminder_popup_completes_its_own_item_and_snoozes_same_schedule_for_600_seconds() {
    let directory = tempfile::tempdir().unwrap();
    let target = seed(directory.path(), "提醒目标", TaskState::Todo, 1);
    let selected = seed(directory.path(), "另一选中项", TaskState::Todo, 2);
    let store = LocalStore::open(directory.path()).unwrap();
    let scheduled = store
        .set_reminder(
            &target,
            now() - Duration::seconds(5),
            None,
            None,
            now() - Duration::seconds(10),
        )
        .unwrap();
    let original = scheduled.reminders[0].clone();
    let mut ui = controller(directory.path());
    ui.dispatch(UiIntent::SelectItem(selected.clone())).unwrap();

    ui.dispatch(UiIntent::CheckReminders).unwrap();
    let popup = ui.view().reminder.clone().unwrap();
    assert_eq!(popup.item_id, target);
    assert_eq!(popup.reminder_id, original.id);
    assert_eq!(popup.expected_schedule, original.schedule_revision);

    ui.dispatch(UiIntent::SnoozeReminder).unwrap();
    assert!(ui.view().reminder.is_none());
    let snoozed = store.item(&target).unwrap().unwrap();
    assert_eq!(snoozed.reminders[0].id, original.id);
    assert_ne!(
        snoozed.reminders[0].schedule_revision,
        original.schedule_revision
    );
    assert_eq!(
        snoozed.reminders[0].remind_at,
        (now() + Duration::seconds(600)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    assert_eq!(
        store.item(&selected).unwrap().unwrap().task_state,
        Some(TaskState::Todo)
    );

    let later = store
        .set_reminder(
            &target,
            now() - Duration::seconds(1),
            None,
            None,
            now() - Duration::seconds(2),
        )
        .unwrap();
    assert!(later.reminders.len() >= 2);
    ui.dispatch(UiIntent::CheckReminders).unwrap();
    let popup_target = ui.view().reminder.clone().unwrap().item_id;
    ui.dispatch(UiIntent::CompleteReminder).unwrap();
    assert_eq!(
        store.item(&popup_target).unwrap().unwrap().task_state,
        Some(TaskState::Completed)
    );
    assert_eq!(
        store.item(&selected).unwrap().unwrap().task_state,
        Some(TaskState::Todo)
    );
    assert!(ui.view().reminder.is_none());
}

#[test]
fn enrollment_persists_settings_only_after_success_and_never_exposes_token() {
    let directory = tempfile::tempdir().unwrap();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let enrollment = FakeEnrollment {
        calls: calls.clone(),
        fail: false,
    };
    let shell = RecordingShell::default();
    let store = LocalStore::open(directory.path()).unwrap();
    let mut ui = UiController::new(
        store,
        NoScreenshot,
        NoClipboard,
        enrollment.clone(),
        shell.clone(),
        FixedClock(now()),
    )
    .unwrap();
    ui.dispatch(UiIntent::SettingsChanged {
        server_url: "  http://localhost:8787  ".to_owned(),
        enrollment_code: "  CODE  ".to_owned(),
        device_name: "测试机".to_owned(),
    })
    .unwrap();
    ui.dispatch(UiIntent::Register).unwrap();

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[(
            "http://localhost:8787".to_owned(),
            "CODE".to_owned(),
            "测试机".to_owned(),
        )]
    );
    assert!(ui.view().settings.registered);
    assert!(ui.view().settings.enrollment_code.is_empty());
    assert!(!format!("{:?}", ui.view()).contains("secret-device-token"));
    let store = LocalStore::open(directory.path()).unwrap();
    assert_eq!(
        store.meta("server_url").unwrap().as_deref(),
        Some("http://localhost:8787")
    );
    assert_eq!(
        store.meta("device_token").unwrap().as_deref(),
        Some("secret-device-token")
    );
    assert_eq!(
        store.meta("device_name").unwrap().as_deref(),
        Some("测试机")
    );

    ui.dispatch(UiIntent::SetAutostart(true)).unwrap();
    assert_eq!(shell.autostart.lock().unwrap().as_slice(), &[true]);
    assert_eq!(store.meta("autostart").unwrap().as_deref(), Some("true"));
}

#[test]
fn failed_enrollment_does_not_mark_device_registered() {
    let directory = tempfile::tempdir().unwrap();
    let enrollment = FakeEnrollment {
        calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        fail: true,
    };
    let store = LocalStore::open(directory.path()).unwrap();
    let mut ui = UiController::new(
        store,
        NoScreenshot,
        NoClipboard,
        enrollment,
        RecordingShell::default(),
        FixedClock(now()),
    )
    .unwrap();
    ui.dispatch(UiIntent::SettingsChanged {
        server_url: "http://localhost:8787".to_owned(),
        enrollment_code: "BAD".to_owned(),
        device_name: "测试机".to_owned(),
    })
    .unwrap();
    assert!(ui.dispatch(UiIntent::Register).is_err());
    assert!(!ui.view().settings.registered);
    assert!(ui.view().settings.error.is_some());
    let store = LocalStore::open(directory.path()).unwrap();
    assert!(store.meta("device_token").unwrap().is_none());
    assert!(store.meta("server_url").unwrap().is_none());
}

#[derive(Clone)]
struct FakeScreenshot {
    path: std::path::PathBuf,
    regions: std::sync::Arc<std::sync::Mutex<Vec<CaptureRegion>>>,
    full_frames: std::sync::Arc<std::sync::Mutex<usize>>,
    cancellations: std::sync::Arc<std::sync::Mutex<usize>>,
    releases: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
}

impl Screenshot for FakeScreenshot {
    fn capture_full(&self) -> Result<CapturedImage, UiError> {
        *self.full_frames.lock().unwrap() += 1;
        Ok(CapturedImage {
            path: self.path.clone(),
            mime: "image/png".to_owned(),
            pixel_width: 100,
            pixel_height: 100,
        })
    }

    fn capture(&self, region: CaptureRegion) -> Result<CapturedImage, UiError> {
        self.regions.lock().unwrap().push(region);
        Ok(CapturedImage {
            path: self.path.clone(),
            mime: "image/png".to_owned(),
            pixel_width: 2,
            pixel_height: 3,
        })
    }

    fn cancel_capture(&self) {
        *self.cancellations.lock().unwrap() += 1;
    }

    fn release_capture(&self, image: &CapturedImage) {
        self.releases.lock().unwrap().push(image.path.clone());
    }
}

#[derive(Clone)]
struct FakeClipboard {
    text: Option<String>,
    image: Option<CapturedImage>,
    releases: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> Result<Option<String>, UiError> {
        Ok(self.text.clone())
    }

    fn read_image(&self) -> Result<Option<CapturedImage>, UiError> {
        Ok(self.image.clone())
    }

    fn release_image(&self, image: &CapturedImage) {
        self.releases.lock().unwrap().push(image.path.clone());
    }
}

fn write_png(path: &Path) {
    image::RgbImage::new(2, 3).save(path).unwrap();
}

#[test]
fn dropped_image_saves_immediately_and_invalid_image_retains_draft() {
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("drop.PNG");
    write_png(&image_path);
    let mut ui = controller(directory.path());
    ui.dispatch(UiIntent::CaptureDraftChanged("配图记录".to_owned()))
        .unwrap();
    ui.dispatch(UiIntent::DropImages(vec![image_path])).unwrap();

    assert_eq!(ui.view().capture.draft, "");
    let store = LocalStore::open(directory.path()).unwrap();
    let items = store.items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].body_md, "配图记录");
    assert_eq!(items[0].source.input_method, "drop");
    assert_eq!(items[0].attachments.len(), 1);
    assert_eq!(items[0].attachments[0].pixel_width, 2);
    assert_eq!(items[0].attachments[0].pixel_height, 3);
    assert!(store.attachment_path(&items[0].attachments[0]).is_file());

    let invalid = directory.path().join("not-image.png");
    std::fs::write(&invalid, b"not an image").unwrap();
    ui.dispatch(UiIntent::CaptureDraftChanged("失败时保留".to_owned()))
        .unwrap();
    assert!(ui.dispatch(UiIntent::DropImages(vec![invalid])).is_err());
    assert_eq!(ui.view().capture.draft, "失败时保留");
    assert_eq!(store.items().unwrap().len(), 1);
}

#[test]
fn screenshot_selection_is_normalized_and_small_selection_cancels() {
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("screen.png");
    write_png(&image_path);
    let regions = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let full_frames = std::sync::Arc::new(std::sync::Mutex::new(0));
    let cancellations = std::sync::Arc::new(std::sync::Mutex::new(0));
    let releases = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let screenshot = FakeScreenshot {
        path: image_path.clone(),
        regions: regions.clone(),
        full_frames: full_frames.clone(),
        cancellations: cancellations.clone(),
        releases: releases.clone(),
    };
    let store = LocalStore::open(directory.path()).unwrap();
    let mut ui = UiController::new(
        store,
        screenshot,
        NoClipboard,
        NoEnrollment,
        NoShell,
        FixedClock(now()),
    )
    .unwrap();

    ui.dispatch(UiIntent::CaptureDraftChanged("未发送草稿".to_owned()))
        .unwrap();
    ui.dispatch(UiIntent::BeginScreenshot).unwrap();
    assert!(ui.view().screenshot.active);
    assert!(ui.view().screenshot.background.is_some());
    assert_eq!(*full_frames.lock().unwrap(), 1);
    ui.dispatch(UiIntent::ScreenshotStarted { x: 90.0, y: 70.0 })
        .unwrap();
    ui.dispatch(UiIntent::ScreenshotMoved { x: 10.0, y: 20.0 })
        .unwrap();
    ui.dispatch(UiIntent::ScreenshotFinished { x: 10.0, y: 20.0 })
        .unwrap();
    assert_eq!(
        regions.lock().unwrap().as_slice(),
        &[CaptureRegion {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 50.0,
        }]
    );
    assert_eq!(
        LocalStore::open(directory.path())
            .unwrap()
            .items()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(ui.view().capture.draft, "");
    assert_eq!(
        releases.lock().unwrap().as_slice(),
        std::slice::from_ref(&image_path)
    );
    assert_eq!(
        LocalStore::open(directory.path()).unwrap().items().unwrap()[0].body_md,
        "未发送草稿"
    );

    ui.dispatch(UiIntent::ScreenshotStarted { x: 0.0, y: 0.0 })
        .unwrap();
    ui.dispatch(UiIntent::ScreenshotFinished { x: 4.0, y: 100.0 })
        .unwrap();
    assert_eq!(regions.lock().unwrap().len(), 1);
    assert_eq!(
        LocalStore::open(directory.path())
            .unwrap()
            .items()
            .unwrap()
            .len(),
        1
    );
    assert!(ui.view().screenshot.start_x.is_none());
    assert_eq!(*cancellations.lock().unwrap(), 1);

    ui.dispatch(UiIntent::BeginScreenshot).unwrap();
    ui.dispatch(UiIntent::ScreenshotCancelled).unwrap();
    assert_eq!(*cancellations.lock().unwrap(), 2);
    assert!(!ui.view().screenshot.active);
}

#[test]
fn clipboard_image_is_saved_through_core_and_released() {
    let directory = tempfile::tempdir().unwrap();
    let image_path = directory.path().join("clipboard.png");
    write_png(&image_path);
    let releases = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let clipboard = FakeClipboard {
        text: None,
        image: Some(CapturedImage {
            path: image_path.clone(),
            mime: "image/png".to_owned(),
            pixel_width: 2,
            pixel_height: 3,
        }),
        releases: releases.clone(),
    };
    let store = LocalStore::open(directory.path()).unwrap();
    let mut ui = UiController::new(
        store,
        NoScreenshot,
        clipboard,
        NoEnrollment,
        NoShell,
        FixedClock(now()),
    )
    .unwrap();

    ui.dispatch(UiIntent::CaptureClipboard).unwrap();

    let items = LocalStore::open(directory.path()).unwrap().items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].attachments.len(), 1);
    assert_eq!(
        releases.lock().unwrap().as_slice(),
        std::slice::from_ref(&image_path)
    );
}

#[test]
fn clipboard_text_is_saved_through_core() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalStore::open(directory.path()).unwrap();
    let clipboard = FakeClipboard {
        text: Some("剪贴板记录".to_owned()),
        image: None,
        releases: Default::default(),
    };
    let mut ui = UiController::new(
        store,
        NoScreenshot,
        clipboard,
        NoEnrollment,
        NoShell,
        FixedClock(now()),
    )
    .unwrap();
    ui.dispatch(UiIntent::CaptureDraftChanged("独立草稿".to_owned()))
        .unwrap();
    ui.dispatch(UiIntent::CaptureClipboard).unwrap();
    assert_eq!(ui.view().capture.draft, "独立草稿");
    let items = LocalStore::open(directory.path()).unwrap().items().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].body_md, "剪贴板记录");
    assert_eq!(items[0].source.input_method, "clipboard");
}

#[test]
fn synchronized_requires_ready_attachments_as_well_as_no_pending_operations() {
    let directory = tempfile::tempdir().unwrap();
    let mut item = Item::new_task("带附件", "text", now(), "UTC", vec![]).unwrap();
    item.attachments
        .push(Attachment::new("image/png", 3, "abc", 2, 3, 0));
    let store = LocalStore::open(directory.path()).unwrap();
    store.apply_remote(item).unwrap();
    let mut ui = controller(directory.path());
    assert!(store.pending().unwrap().is_empty());
    assert!(!ui.view().items.rows[0].synchronized);

    let mut item = store.items().unwrap().remove(0);
    item.attachments[0].state = "ready".to_owned();
    store.update_attachment(&item.attachments[0]).unwrap();
    ui.dispatch(UiIntent::Refresh).unwrap();
    assert!(ui.view().items.rows[0].synchronized);
    ui.dispatch(UiIntent::SelectItem(item.id)).unwrap();
    assert!(ui.view().items.selected.as_ref().unwrap().synchronized);
}

#[test]
fn missing_task_state_uses_item_kind_instead_of_assuming_capture() {
    let directory = tempfile::tempdir().unwrap();
    let mut item = Item::new_task("普通备忘", "text", now(), "UTC", vec![]).unwrap();
    item.kind = ItemKind::Memo;
    item.task_state = None;
    LocalStore::open(directory.path())
        .unwrap()
        .apply_remote(item)
        .unwrap();
    let ui = controller(directory.path());
    assert_eq!(ui.view().items.rows[0].state, "memo");
}
