use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{Item, LocalStore, Reminder, StoreError};

const FIRED_KEY: &str = "fired_reminders";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub item: Item,
    pub reminder: Reminder,
}

#[derive(Debug, Default, Deserialize)]
struct FiredReminders {
    #[serde(default)]
    keys: BTreeSet<String>,
}

pub struct ReminderScheduler<'a> {
    store: &'a LocalStore,
}

impl<'a> ReminderScheduler<'a> {
    pub fn new(store: &'a LocalStore) -> Self {
        Self { store }
    }

    pub fn key(item: &Item, reminder: &Reminder) -> String {
        format!(
            "{}:{}:{}:{}",
            item.id, reminder.id, reminder.schedule_revision, reminder.remind_at
        )
    }

    pub fn due(&self, now: DateTime<Utc>) -> Result<Vec<DueReminder>, StoreError> {
        let fired = self.fired_keys()?;
        let mut result = Vec::new();
        for item in self.store.items()? {
            if item.ended() {
                continue;
            }
            for reminder in &item.reminders {
                if !reminder.enabled {
                    continue;
                }
                let Ok(remind_at) = DateTime::parse_from_rfc3339(&reminder.remind_at) else {
                    continue;
                };
                if remind_at > now || fired.contains(&Self::key(&item, reminder)) {
                    continue;
                }
                result.push(DueReminder {
                    item: item.clone(),
                    reminder: reminder.clone(),
                });
            }
        }
        result.sort_by(|left, right| {
            let left_at = DateTime::parse_from_rfc3339(&left.reminder.remind_at)
                .expect("due reminders have valid timestamps");
            let right_at = DateTime::parse_from_rfc3339(&right.reminder.remind_at)
                .expect("due reminders have valid timestamps");
            left_at
                .cmp(&right_at)
                .then_with(|| left.item.id.cmp(&right.item.id))
                .then_with(|| left.reminder.id.cmp(&right.reminder.id))
        });
        Ok(result)
    }

    pub fn mark_fired(&self, item: &Item, reminder: &Reminder) -> Result<(), StoreError> {
        let mut fired = self.fired_keys()?;
        fired.insert(Self::key(item, reminder));
        self.store.set_meta(
            FIRED_KEY,
            &serde_json::to_string(&serde_json::json!({ "keys": fired }))?,
        )
    }

    pub fn prune_fired(&self) -> Result<(), StoreError> {
        let mut active = BTreeSet::new();
        for item in self.store.items()? {
            if item.ended() {
                continue;
            }
            for reminder in item.reminders.iter().filter(|reminder| reminder.enabled) {
                active.insert(Self::key(&item, reminder));
            }
        }
        let fired = self.fired_keys()?;
        let retained: BTreeSet<String> = fired.intersection(&active).cloned().collect();
        if retained.len() != fired.len() {
            self.store.set_meta(
                FIRED_KEY,
                &serde_json::to_string(&serde_json::json!({ "keys": retained }))?,
            )?;
        }
        Ok(())
    }

    fn fired_keys(&self) -> Result<BTreeSet<String>, StoreError> {
        let Some(raw) = self.store.meta(FIRED_KEY)? else {
            return Ok(BTreeSet::new());
        };
        Ok(serde_json::from_str::<FiredReminders>(&raw)
            .unwrap_or_default()
            .keys)
    }
}
