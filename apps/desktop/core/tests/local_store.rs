use std::{
    fs,
    path::{Path, PathBuf},
};

use aimemorandum_core::{
    Attachment, CoreError, Item, ItemKind, LocalStore, Reminder, StoreError, TaskState,
};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use uuid::Uuid;

struct TestDir(PathBuf);

impl TestDir {
    fn new(test: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aimemorandum-core-{test}-{}",
            Uuid::new_v4().hyphenated()
        ));
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

fn task_with_reminder(text: &str) -> Item {
    let mut item = task(text);
    item.due_at = Some("2026-11-01T07:00:00Z".to_owned());
    item.reminders.push(Reminder {
        id: Uuid::new_v4().hyphenated().to_string(),
        item_id: item.id.clone(),
        remind_at: "2026-10-31T07:00:00Z".to_owned(),
        schedule_revision: Uuid::new_v4().hyphenated().to_string(),
        purpose: "explicit".to_owned(),
        enabled: true,
        revision: 1,
    });
    item
}

#[test]
fn ac_cap_001_offline_item_and_outbox_survive_reopen() {
    let dir = TestDir::new("offline-reopen");
    let item = task("离线正文");
    let item_id = item.id.clone();
    let operation_id;

    {
        let store = LocalStore::open(dir.path()).unwrap();
        let saved = store.create(item).unwrap();
        assert_eq!(saved.kind, ItemKind::Task);

        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].item_id, item_id);
        assert_eq!(pending[0].method, "POST");
        assert_eq!(pending[0].path, "/api/v1/items");
        operation_id = pending[0].id.clone();
        let body: Value = serde_json::from_str(&pending[0].body).unwrap();
        assert_eq!(body["operation_id"], operation_id);
        assert_eq!(body["item"]["id"], item_id);
    }

    let reopened = LocalStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened.item(&item_id).unwrap().unwrap().body_md,
        "离线正文"
    );
    let pending = reopened.pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, operation_id);
}

#[test]
fn ac_not_001_complete_disables_reminders_without_changing_due_at() {
    let dir = TestDir::new("complete-reminder");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task_with_reminder("完成时取消提醒");
    let item_id = item.id.clone();
    let due_at = item.due_at.clone();
    store.create(item).unwrap();

    store
        .complete(
            &item_id,
            Utc.with_ymd_and_hms(2026, 10, 30, 8, 0, 0).unwrap(),
        )
        .unwrap();

    let completed = store.item(&item_id).unwrap().unwrap();
    assert_eq!(completed.task_state, Some(TaskState::Completed));
    assert_eq!(
        completed.completed_at.as_deref(),
        Some("2026-10-30T08:00:00Z")
    );
    assert!(completed.reminders.iter().all(|reminder| !reminder.enabled));
    assert_eq!(completed.due_at, due_at);

    let pending = store.pending().unwrap();
    assert_eq!(pending.len(), 2);
    let operation = &pending[1];
    assert_eq!(operation.path, format!("/api/v1/items/{item_id}/actions"));
    assert_eq!(
        operation.depends_on.as_deref(),
        Some(pending[0].id.as_str())
    );
    let body: Value = serde_json::from_str(&operation.body).unwrap();
    assert_eq!(body["operation_id"], operation.id);
    assert_eq!(body["action"], "complete");
    assert_eq!(body["base_revision"], 0);
}

#[test]
fn ac_sync_002_remote_does_not_overwrite_pending_body_but_updates_shadow() {
    let dir = TestDir::new("pending-shadow");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("尚未同步的本地正文");
    let item_id = item.id.clone();
    store.create(item.clone()).unwrap();

    let mut remote = item;
    remote.body_md = "服务端正文".to_owned();
    remote.revision = 2;
    store.apply_remote(remote).unwrap();

    let local = store.item(&item_id).unwrap().unwrap();
    assert_eq!(local.body_md, "尚未同步的本地正文");
    assert_eq!(local.revision, 0);
    let shadow = store.shadow(&item_id).unwrap().unwrap();
    assert_eq!(shadow.body_md, "服务端正文");
    assert_eq!(shadow.revision, 2);
    assert_eq!(store.pending().unwrap().len(), 1);
}

#[test]
fn ac_dom_003_delete_disables_reminders_and_queues_delete_action() {
    let dir = TestDir::new("delete-reminder");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task_with_reminder("删除时取消提醒");
    let item_id = item.id.clone();
    store.create(item.clone()).unwrap();
    let mut remote = item;
    remote.revision = 3;
    store.apply_remote(remote).unwrap();

    store
        .delete(
            &item_id,
            Utc.with_ymd_and_hms(2026, 10, 30, 9, 0, 0).unwrap(),
        )
        .unwrap();

    let deleted = store.item(&item_id).unwrap().unwrap();
    assert_eq!(deleted.deleted_at.as_deref(), Some("2026-10-30T09:00:00Z"));
    assert!(deleted.reminders.iter().all(|reminder| !reminder.enabled));
    let pending = store.pending().unwrap();
    assert_eq!(pending.len(), 2);
    let operation = &pending[1];
    assert_eq!(operation.path, format!("/api/v1/items/{item_id}/actions"));
    assert_eq!(
        operation.depends_on.as_deref(),
        Some(pending[0].id.as_str())
    );
    let body: Value = serde_json::from_str(&operation.body).unwrap();
    assert_eq!(body["operation_id"], operation.id);
    assert_eq!(body["action"], "delete");
    assert_eq!(body["base_revision"], 3);
    assert_eq!(body["payload"], serde_json::json!({}));
}

#[test]
fn ac_dom_004_update_queues_put_and_falls_back_to_body_title() {
    let dir = TestDir::new("update-put");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("旧正文");
    let item_id = item.id.clone();
    store.create(item).unwrap();

    let updated = store
        .update(
            &item_id,
            "   ",
            "替换后的正文",
            Utc.with_ymd_and_hms(2026, 10, 30, 10, 0, 0).unwrap(),
        )
        .unwrap();
    assert_eq!(updated.title, "替换后的正文");
    assert_eq!(updated.title_origin, "user");
    assert_eq!(updated.body_md, "替换后的正文");

    let pending = store.pending().unwrap();
    assert_eq!(pending.len(), 2);
    let operation = &pending[1];
    assert_eq!(operation.method, "PUT");
    assert_eq!(operation.path, format!("/api/v1/items/{item_id}"));
    assert_eq!(
        operation.depends_on.as_deref(),
        Some(pending[0].id.as_str())
    );
    let body: Value = serde_json::from_str(&operation.body).unwrap();
    assert_eq!(body["operation_id"], operation.id);
    assert_eq!(body["base_revision"], 0);
    assert_eq!(body["title"], "替换后的正文");
    assert_eq!(body["body_md"], "替换后的正文");
}

#[test]
fn ac_dom_004_update_rejects_all_empty_without_saving_or_queueing() {
    let dir = TestDir::new("update-empty");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("保留原文");
    let item_id = item.id.clone();
    store.create(item).unwrap();
    let before = store.item(&item_id).unwrap().unwrap();

    let error = store.update(&item_id, " \n ", "\t  ", now()).unwrap_err();
    assert!(matches!(error, StoreError::Domain(CoreError::EmptyContent)));
    assert_eq!(store.item(&item_id).unwrap().unwrap(), before);
    assert_eq!(store.pending().unwrap().len(), 1);
}

#[test]
fn attachment_staging_recovers_after_crash_before_commit() {
    let dir = TestDir::new("attachment-recovery");
    let root = dir.path().join("store");
    let source = dir.path().join("source.png");
    let bytes = b"abc";
    fs::write(&source, bytes).unwrap();
    let attachment = Attachment::new(
        "image/png",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        1,
        1,
        0,
    );
    let attachment_id = attachment.id.clone();
    let item = Item::new_task(
        "OCR 正文",
        "screenshot",
        now(),
        "Asia/Shanghai",
        vec![attachment],
    )
    .unwrap();
    let item_id = item.id.clone();

    let store = LocalStore::open(&root).unwrap();
    let staged = store
        .stage_create(item.clone(), &[(attachment_id, source)])
        .unwrap();
    assert!(store.item(&item_id).unwrap().is_none());
    assert!(store.pending().unwrap().is_empty());
    drop(staged);
    drop(store);

    let recovered = LocalStore::open(&root).unwrap();
    assert_eq!(recovered.item(&item_id).unwrap().unwrap(), item);
    assert_eq!(
        fs::read(recovered.attachment_path(&item.attachments[0])).unwrap(),
        bytes
    );
    let recovered_pending = recovered.pending().unwrap();
    assert_eq!(recovered_pending.len(), 1);
    let operation_id = recovered_pending[0].id.clone();
    drop(recovered);

    let reopened = LocalStore::open(&root).unwrap();
    let reopened_pending = reopened.pending().unwrap();
    assert_eq!(reopened_pending.len(), 1);
    assert_eq!(reopened_pending[0].id, operation_id);
}

#[test]
fn attachment_staged_commit_persists_and_cleans_up() {
    let dir = TestDir::new("attachment-commit");
    let root = dir.path().join("store");
    let source = dir.path().join("source.png");
    fs::write(&source, b"abc").unwrap();
    let attachment = Attachment::new(
        "image/png",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        1,
        1,
        0,
    );
    let attachment_id = attachment.id.clone();
    let item = Item::new_task(
        "OCR 正文",
        "screenshot",
        now(),
        "Asia/Shanghai",
        vec![attachment],
    )
    .unwrap();
    let item_id = item.id.clone();

    let store = LocalStore::open(&root).unwrap();
    let staged = store
        .stage_create(item.clone(), &[(attachment_id, source)])
        .unwrap();
    let committed = store.commit_staged(staged).unwrap();
    assert_eq!(committed, item);
    assert_eq!(store.item(&item_id).unwrap().unwrap(), item);
    assert_eq!(store.pending().unwrap().len(), 1);
    drop(store);

    let reopened = LocalStore::open(&root).unwrap();
    assert_eq!(reopened.item(&item_id).unwrap().unwrap(), item);
    assert_eq!(reopened.pending().unwrap().len(), 1);
}

#[test]
fn attachment_recovery_error_retains_staging_until_bytes_are_repaired() {
    let dir = TestDir::new("attachment-recovery-error");
    let root = dir.path().join("store");
    let source = dir.path().join("source.png");
    fs::write(&source, b"abc").unwrap();
    let attachment = Attachment::new(
        "image/png",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        1,
        1,
        0,
    );
    let attachment_id = attachment.id.clone();
    let item = Item::new_task(
        "OCR 正文",
        "screenshot",
        now(),
        "Asia/Shanghai",
        vec![attachment],
    )
    .unwrap();
    let item_id = item.id.clone();

    let store = LocalStore::open(&root).unwrap();
    let staged = store
        .stage_create(item.clone(), &[(attachment_id, source)])
        .unwrap();
    let attachment_path = store.attachment_path(&item.attachments[0]);
    fs::write(&attachment_path, b"corrupt").unwrap();
    drop(staged);
    drop(store);

    let failed = LocalStore::open(&root).unwrap();
    assert!(failed.item(&item_id).unwrap().is_none());
    assert!(failed.pending().unwrap().is_empty());
    assert_eq!(failed.recovery_errors().len(), 1);
    assert!(failed.recovery_errors()[0].contains("SHA-256"));
    drop(failed);

    fs::write(&attachment_path, b"abc").unwrap();
    let recovered = LocalStore::open(&root).unwrap();
    assert!(recovered.recovery_errors().is_empty());
    assert_eq!(recovered.item(&item_id).unwrap().unwrap(), item);
    assert_eq!(recovered.pending().unwrap().len(), 1);
}

#[test]
fn update_enforces_utf8_body_limit_without_partial_save() {
    let dir = TestDir::new("update-body-limit");
    let root = dir.path().join("store");
    let store = LocalStore::open(&root).unwrap();
    let item = store
        .create(Item::new_task("small", "keyboard", now(), "UTC", vec![]).unwrap())
        .unwrap();
    let maximum = "a".repeat(aimemorandum_core::MAX_BODY_MD_BYTES);
    let updated = store.update(&item.id, "", &maximum, now()).unwrap();
    assert_eq!(updated.body_md.len(), aimemorandum_core::MAX_BODY_MD_BYTES);

    let oversized = "界".repeat(aimemorandum_core::MAX_BODY_MD_BYTES / 3 + 1);
    let error = match store.update(&item.id, "", &oversized, now()) {
        Ok(_) => panic!("oversized UTF-8 body was saved"),
        Err(error) => error,
    };
    assert!(matches!(error, StoreError::Domain(CoreError::BodyTooLarge)));
    assert_eq!(store.item(&item.id).unwrap().unwrap(), updated);
    assert_eq!(store.pending().unwrap().len(), 2);
}

#[test]
fn reopen_restores_todo_without_reactivating_reminders() {
    let dir = TestDir::new("reopen");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task_with_reminder("完成后重开");
    let item_id = item.id.clone();
    store.create(item).unwrap();
    store.complete(&item_id, now()).unwrap();

    let reopened = store.reopen(&item_id, now()).unwrap();
    assert_eq!(reopened.task_state, Some(TaskState::Todo));
    assert_eq!(reopened.completed_at, None);
    assert!(reopened.reminders.iter().all(|reminder| !reminder.enabled));

    let pending = store.pending().unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(
        pending[2].depends_on.as_deref(),
        Some(pending[1].id.as_str())
    );
    let body: Value = serde_json::from_str(&pending[2].body).unwrap();
    assert_eq!(body["action"], "reopen");
    assert_eq!(body["payload"], serde_json::json!({}));
}

#[test]
fn prepare_freezes_rewritten_bytes_and_ack_projects_after_successors() {
    let dir = TestDir::new("prepare-ack");
    let store = LocalStore::open(dir.path()).unwrap();
    let original = task("原始正文");
    let item_id = original.id.clone();
    store.create(original.clone()).unwrap();
    let updated = store.update(&item_id, "", "本地修改", now()).unwrap();

    let pending = store.pending().unwrap();
    let create = pending[0].clone();
    let update = pending[1].clone();
    assert!(store.prepare(&update).unwrap().is_none());

    let frozen_create = store.prepare(&create).unwrap().unwrap();
    assert!(frozen_create.frozen);
    assert_eq!(frozen_create.body, create.body);

    let mut remote_create = original;
    remote_create.revision = 1;
    remote_create.body_md = "服务端创建版本".to_owned();
    store
        .acknowledge(&frozen_create, remote_create.clone())
        .unwrap();
    assert_eq!(store.item(&item_id).unwrap().unwrap(), updated);
    assert_eq!(store.shadow(&item_id).unwrap().unwrap(), remote_create);

    let queued_update = store.pending().unwrap().remove(0);
    let prepared = store.prepare(&queued_update).unwrap().unwrap();
    assert!(prepared.frozen);
    assert_eq!(
        prepared.body,
        format!(
            "{{\"base_revision\":1,\"body_md\":\"本地修改\",\"operation_id\":\"{}\",\"title\":\"本地修改\"}}",
            queued_update.id
        )
    );
    assert_eq!(store.prepare(&prepared).unwrap().unwrap(), prepared);

    let mut remote_update = remote_create;
    remote_update.revision = 2;
    remote_update.body_md = "服务端更新版本".to_owned();
    store.acknowledge(&prepared, remote_update.clone()).unwrap();
    assert!(store.pending().unwrap().is_empty());
    assert_eq!(store.item(&item_id).unwrap().unwrap(), remote_update);
    assert_eq!(store.shadow(&item_id).unwrap().unwrap(), remote_update);
}

#[test]
fn reminder_set_and_snooze_preserve_schedule_contract() {
    let dir = TestDir::new("reminder-snooze");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("设置提醒");
    let item_id = item.id.clone();
    store.create(item).unwrap();
    let first_time = Utc.with_ymd_and_hms(2026, 11, 1, 7, 0, 0).unwrap();

    let with_reminder = store
        .set_reminder(&item_id, first_time, None, None, now())
        .unwrap();
    assert_eq!(with_reminder.reminders.len(), 1);
    let first = &with_reminder.reminders[0];
    assert_eq!(first.remind_at, "2026-11-01T07:00:00Z");
    assert_eq!(first.revision, 1);
    assert!(first.enabled);
    let pending = store.pending().unwrap();
    let set_body: Value = serde_json::from_str(&pending[1].body).unwrap();
    assert_eq!(set_body["action"], "set_reminder");
    assert_eq!(set_body["payload"]["reminder_id"], first.id);
    assert_eq!(
        set_body["payload"]["schedule_revision"],
        first.schedule_revision
    );

    let second_time = Utc.with_ymd_and_hms(2026, 11, 2, 7, 0, 0).unwrap();
    let snoozed = store
        .set_reminder(
            &item_id,
            second_time,
            Some(&first.id),
            Some(&first.schedule_revision),
            now(),
        )
        .unwrap();
    assert_eq!(snoozed.reminders.len(), 1);
    let second = &snoozed.reminders[0];
    assert_eq!(second.id, first.id);
    assert_eq!(second.remind_at, "2026-11-02T07:00:00Z");
    assert_eq!(second.revision, 2);
    assert_ne!(second.schedule_revision, first.schedule_revision);
    let pending = store.pending().unwrap();
    let snooze_body: Value = serde_json::from_str(&pending[2].body).unwrap();
    assert_eq!(snooze_body["action"], "snooze");
    assert_eq!(
        snooze_body["payload"]["expected_schedule_revision"],
        first.schedule_revision
    );

    let error = store
        .set_reminder(
            &item_id,
            second_time,
            Some(&first.id),
            Some(&first.schedule_revision),
            now(),
        )
        .unwrap_err();
    assert!(matches!(error, StoreError::InvalidState(_)));
    assert_eq!(store.pending().unwrap().len(), 3);
}

#[test]
fn conflict_preserves_local_until_use_remote_and_meta_survives_reopen() {
    let dir = TestDir::new("conflict-meta");
    let root = dir.path().join("store");
    let store = LocalStore::open(&root).unwrap();
    let original = task("原始正文");
    let item_id = original.id.clone();
    store.create(original.clone()).unwrap();
    let local = store.update(&item_id, "", "本地冲突正文", now()).unwrap();
    let operation = store.pending().unwrap().remove(1);
    let mut remote = original;
    remote.revision = 3;
    remote.body_md = "远端冲突正文".to_owned();

    let failed = store
        .fail(&operation, "revision conflict", false, Some(remote.clone()))
        .unwrap();
    assert_eq!(failed.state, "conflict");
    assert_eq!(failed.error.as_deref(), Some("revision conflict"));
    assert_eq!(store.item(&item_id).unwrap().unwrap(), local);
    assert_eq!(store.shadow(&item_id).unwrap().unwrap(), remote);
    assert_eq!(store.conflict_items().unwrap(), vec![item_id.clone()]);
    assert_eq!(store.pending().unwrap().len(), 2);

    store.use_remote(&item_id).unwrap();
    assert_eq!(store.item(&item_id).unwrap().unwrap(), remote);
    assert!(store.conflict_items().unwrap().is_empty());
    assert!(store.pending().unwrap().is_empty());

    assert_eq!(store.meta("cursor").unwrap(), None);
    store.set_meta("cursor", "42").unwrap();
    store.set_meta("cursor", "43").unwrap();
    assert_eq!(store.meta("cursor").unwrap().as_deref(), Some("43"));
    drop(store);

    let reopened = LocalStore::open(&root).unwrap();
    assert_eq!(reopened.meta("cursor").unwrap().as_deref(), Some("43"));
}

#[test]
fn attachment_staging_enforces_limits_before_writing() {
    let dir = TestDir::new("attachment-limits");
    let root = dir.path().join("store");
    let source = dir.path().join("source.png");
    fs::write(&source, b"abc").unwrap();
    let store = LocalStore::open(&root).unwrap();
    let valid = || {
        Attachment::new(
            "image/png",
            3,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            1,
            1,
            0,
        )
    };
    let mut cases: Vec<(&str, Vec<Attachment>)> = Vec::new();
    cases.push(("1 to 10 attachments", vec![]));
    cases.push(("at most 10 attachments", (0..11).map(|_| valid()).collect()));
    let mut oversized = valid();
    oversized.byte_size = 20 * 1024 * 1024 + 1;
    cases.push(("20 MiB", vec![oversized]));
    let mut too_many_pixels = valid();
    too_many_pixels.pixel_width = 40_000_001;
    cases.push(("40 MP", vec![too_many_pixels]));
    let mut zero_dimensions = valid();
    zero_dimensions.pixel_width = 0;
    cases.push(("positive dimensions", vec![zero_dimensions]));
    let mut excessive_total = Vec::new();
    for _ in 0..3 {
        let mut attachment = valid();
        attachment.byte_size = 18 * 1024 * 1024;
        excessive_total.push(attachment);
    }
    cases.push(("50 MiB", excessive_total));
    let mut bad_hash = valid();
    bad_hash.sha256 = "ABC".to_owned();
    cases.push(("SHA-256", vec![bad_hash]));
    let duplicate = valid();
    cases.push(("unique attachment IDs", vec![duplicate.clone(), duplicate]));
    let mut invalid_id = valid();
    invalid_id.id = "../escape".to_owned();
    cases.push(("attachment UUID", vec![invalid_id]));

    for (expected, attachments) in cases {
        let sources: Vec<(String, PathBuf)> = attachments
            .iter()
            .map(|attachment| (attachment.id.clone(), source.clone()))
            .collect();
        let item =
            Item::new_task("截图", "screenshot", now(), "Asia/Shanghai", attachments).unwrap();
        let error = match store.stage_create(item, &sources) {
            Ok(_) => panic!("invalid attachment declaration was staged: {expected}"),
            Err(StoreError::InvalidState(message)) => message,
            Err(error) => panic!("unexpected error for {expected}: {error}"),
        };
        assert!(
            error.contains(expected),
            "expected '{expected}' in error '{error}'"
        );
    }
    assert!(!root.join("staging").exists());
}

#[test]
fn ended_items_reject_invalid_local_transitions_without_new_outbox_rows() {
    let dir = TestDir::new("invalid-transitions");
    let store = LocalStore::open(dir.path()).unwrap();
    let item = task("状态约束");
    let item_id = item.id.clone();
    store.create(item).unwrap();
    store.complete(&item_id, now()).unwrap();
    let completed = store.item(&item_id).unwrap().unwrap();
    let pending = store.pending().unwrap();

    assert!(matches!(
        store.complete(&item_id, now()),
        Err(StoreError::InvalidState(_))
    ));
    assert_eq!(store.item(&item_id).unwrap().unwrap(), completed);
    assert_eq!(store.pending().unwrap(), pending);

    store.delete(&item_id, now()).unwrap();
    let deleted = store.item(&item_id).unwrap().unwrap();
    let pending = store.pending().unwrap();
    assert!(matches!(
        store.update(&item_id, "", "不应保存", now()),
        Err(StoreError::InvalidState(_))
    ));
    assert!(matches!(
        store.delete(&item_id, now()),
        Err(StoreError::InvalidState(_))
    ));
    assert_eq!(store.item(&item_id).unwrap().unwrap(), deleted);
    assert_eq!(store.pending().unwrap(), pending);
}

#[test]
fn create_rejects_attachments_outside_staging() {
    let dir = TestDir::new("unstaged-attachment");
    let store = LocalStore::open(dir.path()).unwrap();
    let attachment = Attachment::new(
        "image/png",
        3,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        1,
        1,
        0,
    );
    let item = Item::new_task(
        "截图",
        "screenshot",
        now(),
        "Asia/Shanghai",
        vec![attachment],
    )
    .unwrap();
    let item_id = item.id.clone();

    assert!(matches!(
        store.create(item),
        Err(StoreError::InvalidState(_))
    ));
    assert!(store.item(&item_id).unwrap().is_none());
    assert!(store.pending().unwrap().is_empty());
}
