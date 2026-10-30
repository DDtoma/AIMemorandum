use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use aimemorandum_core::{
    ApiError, Attachment, Item, LocalStore, PendingOperation, SyncApi, SyncEngine, SyncPage,
};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

static CASES: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!("../../fixtures/sync_cases.json")).unwrap()
});

struct TestDir(PathBuf);
impl TestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("aimemorandum-sync-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap()
}
fn task(text: &str) -> Item {
    Item::new_task(text, "text", now(), "Asia/Shanghai", vec![]).unwrap()
}
fn page(cursor: &str) -> SyncPage {
    SyncPage {
        items: vec![],
        cursor: cursor.to_owned(),
    }
}
fn http(status: u16, name: &str, item: Option<Item>) -> ApiError {
    ApiError::Http {
        status,
        name: name.to_owned(),
        message: name.to_owned(),
        item: item.map(Box::new),
    }
}

#[derive(Default)]
struct FakeApi {
    submits: RefCell<VecDeque<Result<Item, ApiError>>>,
    items: RefCell<VecDeque<Result<Item, ApiError>>>,
    uploads: RefCell<VecDeque<Result<(), ApiError>>>,
    pages: RefCell<VecDeque<Result<SyncPage, ApiError>>>,
    operations: RefCell<Vec<PendingOperation>>,
    events: RefCell<Vec<&'static str>>,
}
impl FakeApi {
    fn with_page(self, cursor: &str) -> Self {
        self.pages.borrow_mut().push_back(Ok(page(cursor)));
        self
    }
}
impl SyncApi for FakeApi {
    fn submit(&self, operation: &PendingOperation) -> Result<Item, ApiError> {
        self.events.borrow_mut().push("submit");
        self.operations.borrow_mut().push(operation.clone());
        self.submits
            .borrow_mut()
            .pop_front()
            .expect("unexpected submit")
    }
    fn item(&self, _id: &str) -> Result<Item, ApiError> {
        self.events.borrow_mut().push("item");
        self.items
            .borrow_mut()
            .pop_front()
            .expect("unexpected item GET")
    }
    fn upload_attachment(&self, _id: &str, _bytes: &[u8]) -> Result<(), ApiError> {
        self.events.borrow_mut().push("upload");
        self.uploads
            .borrow_mut()
            .pop_front()
            .expect("unexpected upload")
    }
    fn download_attachment(&self, _id: &str) -> Result<Vec<u8>, ApiError> {
        Err(ApiError::Decode("unexpected attachment GET".to_owned()))
    }

    fn pull(&self, _cursor: &str) -> Result<SyncPage, ApiError> {
        self.events.borrow_mut().push("pull");
        self.pages
            .borrow_mut()
            .pop_front()
            .expect("unexpected extra pull")
    }
}

#[test]
fn run_once_pushes_dependencies_then_advances_cursor_with_one_pull() {
    let dir = TestDir::new("ordered");
    let store = LocalStore::open(dir.path()).unwrap();
    let original = task("原始");
    let id = original.id.clone();
    store.create(original.clone()).unwrap();
    let updated = store.update(&id, "", "修改", now()).unwrap();
    let mut created = original.clone();
    created.revision = CASES["dependency"]["acknowledged_revision"]
        .as_u64()
        .unwrap();
    let mut remote_updated = updated.clone();
    remote_updated.revision = 2;
    let api = FakeApi::default().with_page(CASES["dependency"]["final_cursor"].as_str().unwrap());
    api.submits
        .borrow_mut()
        .extend([Ok(created), Ok(remote_updated.clone())]);

    let stats = SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(stats.pushed, 2);
    assert_eq!(stats.pulled, 0);
    assert_eq!(
        store.meta("cursor").unwrap().as_deref(),
        Some(CASES["dependency"]["final_cursor"].as_str().unwrap())
    );
    assert_eq!(store.item(&id).unwrap().unwrap(), remote_updated);
    let operations = api.operations.borrow();
    let update_body: serde_json::Value = serde_json::from_str(&operations[1].body).unwrap();
    assert_eq!(
        update_body["base_revision"],
        CASES["dependency"]["expected_base_revision"]
    );
    assert!(operations.iter().all(|operation| operation.frozen));
    assert_eq!(&*api.events.borrow(), &["submit", "submit", "pull"]);
}

#[test]
fn transient_push_error_keeps_queued_and_stops_pull() {
    let dir = TestDir::new("transient");
    let store = LocalStore::open(dir.path()).unwrap();
    store.create(task("offline")).unwrap();
    let api = FakeApi::default();
    api.submits
        .borrow_mut()
        .push_back(Err(ApiError::Transport("timeout".to_owned())));
    let error = SyncEngine::new(&store, &api).run_once().unwrap_err();
    assert!(error.to_string().contains("timeout"));
    assert_eq!(store.pending().unwrap()[0].state, "queued");
    assert!(api.events.borrow().is_empty() || !api.events.borrow().contains(&"pull"));
    assert_eq!(store.meta("cursor").unwrap(), None);
}

#[test]
fn item_exists_fallback_temporary_failure_is_not_permanent() {
    let dir = TestDir::new("item-exists");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("reinstall");
    store.create(item).unwrap();
    let api = FakeApi::default();
    api.submits.borrow_mut().push_back(Err(http(
        409,
        CASES["item_exists"]["submit_error"].as_str().unwrap(),
        None,
    )));
    api.items
        .borrow_mut()
        .push_back(Err(ApiError::Transport("temporary GET failure".to_owned())));
    SyncEngine::new(&store, &api).run_once().unwrap_err();
    assert_eq!(store.pending().unwrap()[0].state, "queued");
    assert!(!api.events.borrow().contains(&"pull"));
}

#[test]
fn revision_conflict_preserves_local_and_continues_to_pull() {
    let dir = TestDir::new("conflict");
    let store = LocalStore::open(dir.path()).unwrap();
    let original = task("local");
    let id = original.id.clone();
    store.create(original.clone()).unwrap();
    let mut remote = original;
    remote.revision = CASES["revision_conflict"]["remote_revision"]
        .as_u64()
        .unwrap();
    remote.body_md = CASES["revision_conflict"]["remote_body_md"]
        .as_str()
        .unwrap()
        .to_owned();
    let api = FakeApi::default().with_page("0");
    api.submits
        .borrow_mut()
        .push_back(Err(http(409, "REVISION_CONFLICT", Some(remote.clone()))));
    SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(store.pending().unwrap()[0].state, "conflict");
    assert_eq!(store.item(&id).unwrap().unwrap().body_md, "local");
    assert_eq!(store.shadow(&id).unwrap().unwrap(), remote);
    assert_eq!(store.conflict_items().unwrap(), vec![id]);
    assert!(api.events.borrow().contains(&"pull"));
}

#[test]
fn stale_schedule_is_permanent_and_not_a_revision_conflict() {
    let dir = TestDir::new("stale");
    let store = LocalStore::open(dir.path()).unwrap();
    store.create(task("stale schedule")).unwrap();
    let api = FakeApi::default().with_page("0");
    api.submits
        .borrow_mut()
        .push_back(Err(http(409, "STALE_SCHEDULE", None)));
    SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(store.pending().unwrap()[0].state, "invalid");
    assert!(store.conflict_items().unwrap().is_empty());
}

fn staged_item(store: &LocalStore, source: &Path) -> Item {
    let bytes = CASES["attachment"]["bytes"].as_str().unwrap().as_bytes();
    fs::write(source, bytes).unwrap();
    let attachment = Attachment::new(
        "image/png",
        CASES["attachment"]["byte_size"].as_u64().unwrap(),
        CASES["attachment"]["sha256"].as_str().unwrap(),
        1,
        1,
        0,
    );
    let item = Item::new_task("", "screenshot", now(), "UTC", vec![attachment]).unwrap();
    let staged = store
        .stage_create(
            item.clone(),
            &[(item.attachments[0].id.clone(), source.to_path_buf())],
        )
        .unwrap();
    store.commit_staged(staged).unwrap();
    let operation = store
        .prepare(&store.pending().unwrap()[0])
        .unwrap()
        .unwrap();
    let mut remote = item.clone();
    remote.revision = 1;
    store.acknowledge(&operation, remote).unwrap();
    item
}

#[test]
fn corrupt_local_attachment_blocks_upload_and_pull() {
    let dir = TestDir::new("corrupt");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = staged_item(&store, &dir.path().join("source.png"));
    fs::write(store.attachment_path(&item.attachments[0]), b"bad").unwrap();
    let api = FakeApi::default();
    SyncEngine::new(&store, &api).run_once().unwrap_err();
    assert!(!api.events.borrow().contains(&"upload"));
    assert!(!api.events.borrow().contains(&"pull"));
    assert_eq!(
        store.item(&item.id).unwrap().unwrap().attachments[0].state,
        "pending"
    );
}

#[test]
fn duplicate_upload_reconciles_only_matching_ready_remote_attachment() {
    let dir = TestDir::new("reconcile");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = staged_item(&store, &dir.path().join("source.png"));
    let mut remote = item.clone();
    remote.revision = 1;
    remote.attachments[0].state = "ready".to_owned();
    let api = FakeApi::default().with_page("0");
    api.uploads
        .borrow_mut()
        .push_back(Err(http(409, "INVALID_STATE", None)));
    api.items.borrow_mut().push_back(Ok(remote));
    let stats = SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(stats.uploaded, 1);
    assert_eq!(
        store.item(&item.id).unwrap().unwrap().attachments[0].state,
        "ready"
    );
    assert_eq!(
        store.shadow(&item.id).unwrap().unwrap().attachments[0].state,
        "ready"
    );
    assert_eq!(&*api.events.borrow(), &["upload", "item", "pull"]);
}

#[test]
fn run_once_caps_submit_attempts_at_one_hundred_even_when_invalid() {
    let dir = TestDir::new("push-cap");
    let store = LocalStore::open(dir.path()).unwrap();
    for index in 0..101 {
        store.create(task(&format!("item {index}"))).unwrap();
    }
    let api = FakeApi::default().with_page("0");
    api.submits.borrow_mut().extend((0..101).map(|_| {
        Err(http(
            404,
            CASES["item_exists"]["fallback_error"].as_str().unwrap(),
            None,
        ))
    }));

    let stats = SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(stats.pushed, 0);
    assert_eq!(api.operations.borrow().len(), 100);
    assert_eq!(store.pending().unwrap().len(), 101);
}

#[test]
fn item_exists_fallback_not_found_is_invalid_but_still_pulls() {
    let dir = TestDir::new("item-exists-not-found");
    let store = LocalStore::open(dir.path()).unwrap();
    store.create(task("missing on server")).unwrap();
    let api = FakeApi::default().with_page("0");
    api.submits
        .borrow_mut()
        .push_back(Err(http(409, "ITEM_EXISTS", None)));
    api.items
        .borrow_mut()
        .push_back(Err(http(404, "NOT_FOUND", None)));

    SyncEngine::new(&store, &api).run_once().unwrap();
    assert_eq!(store.pending().unwrap()[0].state, "invalid");
    assert_eq!(&*api.events.borrow(), &["submit", "item", "pull"]);
}
