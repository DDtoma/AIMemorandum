use std::fs;
use std::path::PathBuf;

use aimemorandum_core::{ApiClient, Attachment, Item, LocalStore, SyncApi, SyncEngine};
use chrono::{Duration, Utc};
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

#[test]
#[ignore = "requires MEMO_GO_BASE_URL and MEMO_GO_ENROLL_CODE"]
fn real_go_create_upload_update_remind_complete_loop() {
    let base = std::env::var("MEMO_GO_BASE_URL").unwrap();
    let code = std::env::var("MEMO_GO_ENROLL_CODE").unwrap();
    let enrollment = ApiClient::enroll(&base, &code, "rust-go-integration").unwrap();
    let client = ApiClient::new(&base, enrollment.device_token).unwrap();
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
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
    let item = Item::new_task("", "screenshot", Utc::now(), "UTC", vec![attachment]).unwrap();
    let id = item.id.clone();
    let staged = store
        .stage_create(item.clone(), &[(item.attachments[0].id.clone(), source)])
        .unwrap();
    store.commit_staged(staged).unwrap();

    let stats = SyncEngine::new(&store, &client).run_once().unwrap();
    assert_eq!(stats.pushed, 1);
    assert_eq!(stats.uploaded, 1);
    assert_eq!(
        store.item(&id).unwrap().unwrap().attachments[0].state,
        "ready"
    );
    assert_eq!(
        client
            .download_attachment(&store.item(&id).unwrap().unwrap().attachments[0].id)
            .unwrap(),
        b"abc"
    );

    let mut pending = store.item(&id).unwrap().unwrap().attachments[0].clone();
    pending.state = "pending".to_owned();
    store.update_attachment(&pending).unwrap();
    let recovered = SyncEngine::new(&store, &client).run_once().unwrap();
    assert_eq!(recovered.uploaded, 1);
    assert_eq!(
        store.item(&id).unwrap().unwrap().attachments[0].state,
        "ready"
    );

    store.update(&id, "", "Rust → Go 更新", Utc::now()).unwrap();
    SyncEngine::new(&store, &client).run_once().unwrap();
    assert_eq!(client.item(&id).unwrap().body_md, "Rust → Go 更新");

    let remind_at = Utc::now() + Duration::hours(2);
    store
        .set_reminder(&id, remind_at, None, None, Utc::now())
        .unwrap();
    SyncEngine::new(&store, &client).run_once().unwrap();
    assert!(client
        .item(&id)
        .unwrap()
        .reminders
        .iter()
        .any(|reminder| reminder.enabled));

    store.complete(&id, Utc::now()).unwrap();
    SyncEngine::new(&store, &client).run_once().unwrap();
    let completed = client.item(&id).unwrap();
    assert!(completed.completed_at.is_some());
    assert!(completed.reminders.iter().all(|reminder| !reminder.enabled));
}
