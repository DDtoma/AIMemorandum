use aimemorandum_core::{CoreError, Item, LocalStore, StoreError, MAX_BODY_MD_BYTES};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

#[test]
fn create_rejects_oversized_item_without_writing() {
    let root = std::env::temp_dir().join(format!(
        "aimemorandum-core-create-body-limit-{}",
        Uuid::new_v4().hyphenated()
    ));
    let store = LocalStore::open(&root).unwrap();
    let mut item = Item::new_task(
        "small",
        "keyboard",
        Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap(),
        "UTC",
        vec![],
    )
    .unwrap();
    item.body_md = "界".repeat(MAX_BODY_MD_BYTES / 3 + 1);
    let item_id = item.id.clone();

    let error = match store.create(item) {
        Ok(_) => panic!("oversized UTF-8 item was saved"),
        Err(error) => error,
    };
    assert!(matches!(error, StoreError::Domain(CoreError::BodyTooLarge)));
    assert!(store.item(&item_id).unwrap().is_none());
    assert!(store.pending().unwrap().is_empty());
}
