use std::fs;
use std::path::PathBuf;

use aimemorandum_core::{
    ApiClient, Attachment, Item, LocalStore, ReminderScheduler, SyncApi, SyncEngine,
};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("aimemorandum-go-integration-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sync_to_quiescence(store: &LocalStore, client: &ApiClient) {
    for _ in 0..8 {
        let stats = SyncEngine::new(store, client).run_once().unwrap();
        if stats.pushed == 0 && stats.uploaded == 0 && stats.pulled == 0 {
            return;
        }
    }
    panic!("sync did not become quiescent within eight rounds");
}

#[test]
#[ignore = "requires MEMO_GO_BASE_URL and MEMO_GO_ENROLL_CODE"]
fn real_go_offline_create_sync_attachment_reminder_complete_smoke() {
    let base = std::env::var("MEMO_GO_BASE_URL").unwrap();
    let code = std::env::var("MEMO_GO_ENROLL_CODE").unwrap();
    let enrollment = ApiClient::enroll(&base, &code, "rust-go-integration").unwrap();
    let client = ApiClient::new(&base, enrollment.device_token).unwrap();
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
    let now = Utc::now();

    let text = Item::new_task("离线文字", "text", now, "UTC", vec![]).unwrap();
    let text_id = text.id.clone();
    store.create(text).unwrap();
    let text_remind_at = now + Duration::hours(1);
    let text = store
        .set_reminder(&text_id, text_remind_at, None, None, now)
        .unwrap();
    let text_reminder = text.reminders[0].clone();

    let source = dir.0.join("source.png");
    fs::write(&source, b"abc").unwrap();
    let attachment = Attachment::new(
        "image/png",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        1,
        1,
        0,
    );
    let screenshot = Item::new_task("", "screenshot", now, "UTC", vec![attachment]).unwrap();
    let screenshot_id = screenshot.id.clone();
    let staged = store
        .stage_create(
            screenshot.clone(),
            &[(screenshot.attachments[0].id.clone(), source)],
        )
        .unwrap();
    store.commit_staged(staged).unwrap();

    sync_to_quiescence(&store, &client);
    assert!(store.pending().unwrap().is_empty());
    assert!(store
        .item(&screenshot_id)
        .unwrap()
        .unwrap()
        .attachments
        .iter()
        .all(|attachment| attachment.state == "ready"));
    assert_eq!(
        client
            .download_attachment(&store.item(&screenshot_id).unwrap().unwrap().attachments[0].id)
            .unwrap(),
        b"abc"
    );

    let scheduler = ReminderScheduler::new(&store);
    let due = scheduler
        .due(text_remind_at + Duration::seconds(1))
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].item.id, text_id);
    assert_eq!(due[0].reminder.id, text_reminder.id);
    scheduler
        .mark_fired(&due[0].item, &due[0].reminder)
        .unwrap();
    assert!(scheduler
        .due(text_remind_at + Duration::seconds(1))
        .unwrap()
        .is_empty());

    let mut pending = store.item(&screenshot_id).unwrap().unwrap().attachments[0].clone();
    pending.state = "pending".to_owned();
    store.update_attachment(&pending).unwrap();
    let recovered = SyncEngine::new(&store, &client).run_once().unwrap();
    assert_eq!(recovered.uploaded, 1);
    assert_eq!(
        store.item(&screenshot_id).unwrap().unwrap().attachments[0].state,
        "ready"
    );

    store
        .update(&screenshot_id, "", "Rust → Go 更新", Utc::now())
        .unwrap();
    sync_to_quiescence(&store, &client);
    assert_eq!(
        client.item(&screenshot_id).unwrap().body_md,
        "Rust → Go 更新"
    );

    let screenshot_remind_at = Utc::now() + Duration::hours(2);
    store
        .set_reminder(&screenshot_id, screenshot_remind_at, None, None, Utc::now())
        .unwrap();
    sync_to_quiescence(&store, &client);
    assert!(client
        .item(&screenshot_id)
        .unwrap()
        .reminders
        .iter()
        .any(|reminder| reminder.enabled));

    store.complete(&text_id, Utc::now()).unwrap();
    store.complete(&screenshot_id, Utc::now()).unwrap();
    sync_to_quiescence(&store, &client);
    let completed_text = client.item(&text_id).unwrap();
    assert!(completed_text.completed_at.is_some());
    assert!(completed_text
        .reminders
        .iter()
        .all(|reminder| !reminder.enabled));
    let completed_screenshot = client.item(&screenshot_id).unwrap();
    assert!(completed_screenshot.completed_at.is_some());
    assert!(completed_screenshot
        .reminders
        .iter()
        .all(|reminder| !reminder.enabled));
    assert!(ReminderScheduler::new(&store)
        .due(DateTime::<Utc>::MAX_UTC)
        .unwrap()
        .is_empty());
}
