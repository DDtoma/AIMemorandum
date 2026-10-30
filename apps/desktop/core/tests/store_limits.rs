use aimemorandum_core::{Attachment, Item, LocalStore, StoreError};
use chrono::{TimeZone, Utc};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

struct TestDir(PathBuf);
impl TestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("aimemorandum-limits-{name}-{}", Uuid::new_v4()));
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

fn attachment(byte_size: u64, width: u32, height: u32, position: u32) -> Attachment {
    Attachment::new(
        "image/png",
        byte_size,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        width,
        height,
        position,
    )
}
fn item(attachments: Vec<Attachment>) -> Item {
    Item::new_task(
        "",
        "screenshot",
        Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap(),
        "UTC",
        attachments,
    )
    .unwrap()
}
fn sources(dir: &Path, item: &Item) -> Vec<(String, PathBuf)> {
    let source = dir.join("source.png");
    fs::write(source, b"abc").unwrap();
    item.attachments
        .iter()
        .map(|a| (a.id.clone(), dir.join("source.png")))
        .collect()
}

fn stage_error(store: &LocalStore, item: &Item, dir: &Path) -> StoreError {
    match store.stage_create(item.clone(), &sources(dir, item)) {
        Ok(_) => panic!("invalid attachment set was staged"),
        Err(error) => error,
    }
}

#[test]
fn rejects_more_than_ten_attachments_before_staging() {
    let dir = TestDir::new("count");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = item((0..11).map(|i| attachment(3, 1, 1, i)).collect());
    let error = stage_error(&store, &item, dir.path());
    assert!(error.to_string().contains("10"), "{error}");
}

#[test]
fn rejects_single_attachment_over_twenty_mib_before_reading_source() {
    let dir = TestDir::new("single-size");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = item(vec![attachment(20 * 1024 * 1024 + 1, 1, 1, 0)]);
    let error = stage_error(&store, &item, dir.path());
    assert!(error.to_string().contains("20 MiB"), "{error}");
}

#[test]
fn rejects_total_attachment_size_over_fifty_mib_before_reading_sources() {
    let dir = TestDir::new("total-size");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = item(
        (0..3)
            .map(|i| attachment(20 * 1024 * 1024, 1, 1, i))
            .collect(),
    );
    let error = stage_error(&store, &item, dir.path());
    assert!(error.to_string().contains("50 MiB"), "{error}");
}

#[test]
fn rejects_invalid_or_excessive_pixel_dimensions() {
    for (width, height) in [(0, 1), (6001, 8000)] {
        let dir = TestDir::new("pixels");
        let store = LocalStore::open(dir.path()).unwrap();
        let item = item(vec![attachment(3, width, height, 0)]);
        let error = stage_error(&store, &item, dir.path());
        assert!(error.to_string().contains("40"), "{error}");
    }
}

#[test]
fn post_commit_staging_cleanup_failure_does_not_report_save_failure() {
    let dir = TestDir::new("cleanup");
    let root = dir.path().join("store");
    let store = LocalStore::open(&root).unwrap();
    let item = item(vec![attachment(3, 1, 1, 0)]);
    let id = item.id.clone();
    let staged = store
        .stage_create(item.clone(), &sources(dir.path(), &item))
        .unwrap();
    let staging = root.join("staging").join(&id);
    fs::remove_dir_all(&staging).unwrap();
    fs::write(&staging, b"cleanup will fail").unwrap();

    store.commit_staged(staged).unwrap();
    assert!(store.item(&id).unwrap().is_some());
    assert_eq!(store.pending().unwrap().len(), 1);
}
