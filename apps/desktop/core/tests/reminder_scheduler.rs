use std::fs;
use std::path::PathBuf;

use aimemorandum_core::{Item, LocalStore, Reminder, ReminderScheduler};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("aimemorandum-reminder-{}", Uuid::new_v4()));
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
fn ac_not_001_scheduler_returns_due_reminder_only_once() {
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
    let now = Utc::now();
    let item = Item::new_task("离线提醒", "text", now, "UTC", vec![]).unwrap();
    let id = item.id.clone();
    store.create(item).unwrap();
    let item = store
        .set_reminder(&id, now + Duration::minutes(5), None, None, now)
        .unwrap();
    let scheduler = ReminderScheduler::new(&store);

    assert!(scheduler.due(now).unwrap().is_empty());
    let due = scheduler.due(now + Duration::minutes(6)).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].item.id, id);
    assert_eq!(due[0].reminder.id, item.reminders[0].id);

    scheduler
        .mark_fired(&due[0].item, &due[0].reminder)
        .unwrap();
    assert!(scheduler
        .due(now + Duration::minutes(6))
        .unwrap()
        .is_empty());

    drop(store);
    let reopened = LocalStore::open(&dir.0).unwrap();
    assert!(ReminderScheduler::new(&reopened)
        .due(now + Duration::minutes(6))
        .unwrap()
        .is_empty());
}

#[test]
fn ac_not_001_prune_fired_discards_stale_schedule_keys() {
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
    let now = Utc::now();
    let first = Item::new_task("第一条", "text", now, "UTC", vec![]).unwrap();
    let first_id = first.id.clone();
    store.create(first).unwrap();
    let first = store
        .set_reminder(&first_id, now + Duration::minutes(5), None, None, now)
        .unwrap();
    let second = Item::new_task("第二条", "text", now, "UTC", vec![]).unwrap();
    let second_id = second.id.clone();
    store.create(second).unwrap();
    let second = store
        .set_reminder(&second_id, now + Duration::minutes(5), None, None, now)
        .unwrap();
    let scheduler = ReminderScheduler::new(&store);
    let due = scheduler.due(now + Duration::minutes(6)).unwrap();
    for entry in &due {
        scheduler.mark_fired(&entry.item, &entry.reminder).unwrap();
    }
    let kept = ReminderScheduler::key(&second, &second.reminders[0]);

    store
        .set_reminder(
            &first_id,
            now + Duration::minutes(20),
            Some(&first.reminders[0].id),
            Some(&first.reminders[0].schedule_revision),
            now + Duration::minutes(7),
        )
        .unwrap();
    scheduler.prune_fired().unwrap();

    let raw = store.meta("fired_reminders").unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value, serde_json::json!({"keys": [kept]}));
}

#[test]
fn ac_not_001_due_excludes_ended_items_and_sorts_deterministically() {
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
    let now = Utc::now();
    for id in ["item-b", "item-a", "item-c", "item-d"] {
        let mut item = Item::new_task(id, "text", now, "UTC", vec![]).unwrap();
        item.id = id.to_owned();
        store.create(item).unwrap();
        store
            .set_reminder(
                id,
                now + Duration::minutes(5),
                Some(&format!("reminder-{id}")),
                None,
                now,
            )
            .unwrap();
    }
    store
        .complete("item-c", now + Duration::minutes(1))
        .unwrap();
    store.delete("item-d", now + Duration::minutes(1)).unwrap();

    let due = ReminderScheduler::new(&store)
        .due(now + Duration::minutes(6))
        .unwrap();
    let ids: Vec<&str> = due.iter().map(|entry| entry.item.id.as_str()).collect();
    assert_eq!(ids, ["item-a", "item-b"]);
}

#[test]
fn ac_not_001_key_matches_wire_identity_exactly() {
    let mut item = Item::new_task("key", "text", Utc::now(), "UTC", vec![]).unwrap();
    item.id = "item".to_owned();
    let reminder = Reminder {
        id: "reminder".to_owned(),
        item_id: "item".to_owned(),
        remind_at: "2026-10-30T07:00:00Z".to_owned(),
        schedule_revision: "schedule".to_owned(),
        purpose: "explicit".to_owned(),
        enabled: true,
        revision: 1,
    };
    assert_eq!(
        ReminderScheduler::key(&item, &reminder),
        "item:reminder:schedule:2026-10-30T07:00:00Z"
    );
}

#[test]
fn ac_not_001_due_sorts_by_actual_time_across_utc_offsets() {
    let dir = TestDir::new();
    let store = LocalStore::open(&dir.0).unwrap();
    for (id, remind_at) in [
        ("early", "2026-10-30T08:00:00+02:00"),
        ("late", "2026-10-30T07:30:00Z"),
    ] {
        let mut item = Item::new_task(id, "text", Utc::now(), "UTC", vec![]).unwrap();
        item.id = id.to_owned();
        item.reminders = vec![Reminder {
            id: format!("reminder-{id}"),
            item_id: id.to_owned(),
            remind_at: remind_at.to_owned(),
            schedule_revision: "schedule".to_owned(),
            purpose: "explicit".to_owned(),
            enabled: true,
            revision: 1,
        }];
        store.create(item).unwrap();
    }

    let due = ReminderScheduler::new(&store)
        .due(
            DateTime::parse_from_rfc3339("2026-10-30T09:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
    let ids: Vec<&str> = due.iter().map(|entry| entry.item.id.as_str()).collect();
    assert_eq!(ids, ["early", "late"]);
}
