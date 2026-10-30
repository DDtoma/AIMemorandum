use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::model::{
    new_id, Attachment, CoreError, Item, ItemKind, PendingOperation, Reminder, TaskState,
    MAX_BODY_MD_BYTES, MAX_TITLE_CHARS,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Domain(#[from] CoreError),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("item not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

pub struct LocalStore {
    root: PathBuf,
    connection: Connection,
    recovery_errors: Vec<String>,
}

pub struct StagedCapture {
    item: Item,
    operation: PendingOperation,
}

#[derive(Serialize, Deserialize)]
struct CaptureManifest {
    item: Item,
    operation: PendingOperation,
}

impl LocalStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let connection = Connection::open(root.join("memorandum.sqlite"))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS shadow (id TEXT PRIMARY KEY, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS outbox (
                 seq INTEGER PRIMARY KEY AUTOINCREMENT,
                 id TEXT UNIQUE NOT NULL,
                 item_id TEXT NOT NULL,
                 json TEXT NOT NULL,
                 state TEXT NOT NULL,
                 result_revision INTEGER
             );
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS conflicts (
                 item_id TEXT PRIMARY KEY,
                 local_json TEXT NOT NULL,
                 remote_json TEXT NOT NULL
             );",
        )?;
        let mut store = Self {
            root,
            connection,
            recovery_errors: Vec::new(),
        };
        store.recover()?;
        Ok(store)
    }

    pub fn create(&self, item: Item) -> Result<Item, StoreError> {
        if item.kind != ItemKind::Task {
            return Err(StoreError::InvalidState(
                "only tasks can be saved".to_owned(),
            ));
        }
        if item.body_md.len() > MAX_BODY_MD_BYTES {
            return Err(CoreError::BodyTooLarge.into());
        }
        if !item.attachments.is_empty() {
            return Err(StoreError::InvalidState(
                "attachments require staged creation".to_owned(),
            ));
        }
        let operation = creation(&item)?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn stage_create(
        &self,
        item: Item,
        sources: &[(String, PathBuf)],
    ) -> Result<StagedCapture, StoreError> {
        if item.kind != ItemKind::Task {
            return Err(StoreError::InvalidState(
                "only tasks can be saved".to_owned(),
            ));
        }
        if item.body_md.len() > MAX_BODY_MD_BYTES {
            return Err(CoreError::BodyTooLarge.into());
        }
        if item.attachments.is_empty() {
            return Err(StoreError::InvalidState(
                "staged captures require 1 to 10 attachments".to_owned(),
            ));
        }
        validate_attachment_limits(&item)?;
        let source_ids: std::collections::HashSet<&String> =
            sources.iter().map(|(id, _)| id).collect();
        if source_ids.len() != sources.len()
            || sources.len() != item.attachments.len()
            || item
                .attachments
                .iter()
                .any(|attachment| !source_ids.contains(&attachment.id))
        {
            return Err(StoreError::InvalidState(
                "attachment sources must match declarations".to_owned(),
            ));
        }
        let mut files = Vec::with_capacity(item.attachments.len());
        for attachment in &item.attachments {
            if attachment.mime != "image/png" && attachment.mime != "image/jpeg" {
                return Err(StoreError::InvalidState(
                    "only PNG and JPEG attachments are supported".to_owned(),
                ));
            }
            let source = sources
                .iter()
                .find(|(id, _)| id == &attachment.id)
                .map(|(_, path)| path)
                .ok_or_else(|| {
                    StoreError::InvalidState(format!(
                        "missing source for attachment {}",
                        attachment.id
                    ))
                })?;
            let bytes = fs::read(source)?;
            validate_attachment_bytes(attachment, &bytes)?;
            files.push(bytes);
        }

        let operation = creation(&item)?;
        let manifest = CaptureManifest {
            item: item.clone(),
            operation: operation.clone(),
        };
        let staging = self.root.join("staging").join(&item.id);
        fs::create_dir_all(&staging)?;
        durable_write(
            &serde_json::to_vec(&manifest)?,
            &staging.join("manifest.json"),
        )?;
        for (attachment, bytes) in item.attachments.iter().zip(files) {
            durable_write(&bytes, &self.attachment_path(attachment))?;
        }
        Ok(StagedCapture { item, operation })
    }

    pub fn commit_staged(&self, staged: StagedCapture) -> Result<Item, StoreError> {
        let StagedCapture { item, operation } = staged;
        self.validate_attachments(&item)?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        let _ = fs::remove_dir_all(self.root.join("staging").join(&item.id));
        Ok(item)
    }

    pub fn attachment_path(&self, attachment: &Attachment) -> PathBuf {
        let extension = if attachment.mime == "image/png" {
            "png"
        } else {
            "jpg"
        };
        self.root
            .join("attachments")
            .join(&attachment.id)
            .join(format!("original.{extension}"))
    }

    pub fn complete(&self, id: &str, completed_at: DateTime<Utc>) -> Result<Item, StoreError> {
        let mut item = self
            .item(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_owned()))?;
        if item.kind != ItemKind::Task || item.ended() {
            return Err(StoreError::InvalidState(
                "only an unended task can be completed".to_owned(),
            ));
        }
        let timestamp = completed_at.to_rfc3339_opts(SecondsFormat::Secs, true);
        item.task_state = Some(TaskState::Completed);
        item.completed_at = Some(timestamp.clone());
        item.updated_at = timestamp;
        for reminder in &mut item.reminders {
            reminder.enabled = false;
        }
        let operation = self.action(&item, "complete")?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn set_reminder(
        &self,
        id: &str,
        remind_at: DateTime<Utc>,
        reminder_id: Option<&str>,
        expected_schedule: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Item, StoreError> {
        if remind_at <= now {
            return Err(StoreError::InvalidState(
                "reminder must be in the future".to_owned(),
            ));
        }
        let mut item = self
            .item(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_owned()))?;
        if item.ended() {
            return Err(StoreError::InvalidState(
                "ended items cannot be reminded".to_owned(),
            ));
        }
        let original_kind = item.kind;
        let rule_id = reminder_id.map(str::to_owned).unwrap_or_else(new_id);
        if let Some(expected) = expected_schedule {
            let current = item.reminders.iter().find(|reminder| {
                reminder.id == rule_id && reminder.enabled && reminder.schedule_revision == expected
            });
            if current.is_none() {
                return Err(StoreError::InvalidState("这条提醒已经更新".to_owned()));
            }
        }

        let schedule_revision = new_id();
        let revision = item
            .reminders
            .iter()
            .find(|reminder| reminder.id == rule_id)
            .map_or(1, |reminder| reminder.revision + 1);
        let reminder = Reminder {
            id: rule_id.clone(),
            item_id: item.id.clone(),
            remind_at: remind_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            schedule_revision: schedule_revision.clone(),
            purpose: "explicit".to_owned(),
            enabled: true,
            revision,
        };
        if let Some(index) = item
            .reminders
            .iter()
            .position(|existing| existing.id == rule_id)
        {
            item.reminders[index] = reminder;
        } else {
            if item
                .reminders
                .iter()
                .filter(|reminder| reminder.enabled)
                .count()
                >= 8
            {
                return Err(StoreError::InvalidState(
                    "每条记录最多 8 条活跃提醒".to_owned(),
                ));
            }
            item.reminders.push(reminder);
        }
        if original_kind == ItemKind::Capture {
            item.kind = ItemKind::Task;
            item.task_state = Some(TaskState::Todo);
        }
        item.updated_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let action = if expected_schedule.is_some() {
            "snooze"
        } else if original_kind == ItemKind::Task {
            "set_reminder"
        } else {
            "capture_and_remind"
        };
        let mut payload = json!({
            "reminder_id": rule_id,
            "remind_at": remind_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            "schedule_revision": schedule_revision,
        });
        if let Some(expected) = expected_schedule {
            payload
                .as_object_mut()
                .expect("reminder payload is an object")
                .insert("expected_schedule_revision".to_owned(), json!(expected));
        }
        let operation = self.action_with_payload(&item, action, payload)?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn reopen(&self, id: &str, reopened_at: DateTime<Utc>) -> Result<Item, StoreError> {
        let mut item = self
            .item(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_owned()))?;
        if item.kind != ItemKind::Task
            || item.deleted_at.is_some()
            || !matches!(
                item.task_state,
                Some(TaskState::Completed | TaskState::Canceled)
            )
        {
            return Err(StoreError::InvalidState(
                "only an ended, non-deleted task can be reopened".to_owned(),
            ));
        }
        item.task_state = Some(TaskState::Todo);
        item.completed_at = None;
        item.updated_at = reopened_at.to_rfc3339_opts(SecondsFormat::Secs, true);
        let operation = self.action(&item, "reopen")?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn delete(&self, id: &str, deleted_at: DateTime<Utc>) -> Result<Item, StoreError> {
        let mut item = self
            .item(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_owned()))?;
        if item.deleted_at.is_some() {
            return Err(StoreError::InvalidState(
                "item is already deleted".to_owned(),
            ));
        }
        let timestamp = deleted_at.to_rfc3339_opts(SecondsFormat::Secs, true);
        item.deleted_at = Some(timestamp.clone());
        item.updated_at = timestamp;
        for reminder in &mut item.reminders {
            reminder.enabled = false;
        }
        let operation = self.action(&item, "delete")?;
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn update(
        &self,
        id: &str,
        title: &str,
        body_md: &str,
        updated_at: DateTime<Utc>,
    ) -> Result<Item, StoreError> {
        let mut item = self
            .item(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_owned()))?;
        if item.deleted_at.is_some() {
            return Err(StoreError::InvalidState(
                "deleted items cannot be updated".to_owned(),
            ));
        }
        let title = title.trim();
        let body_md = body_md.trim();
        if title.is_empty() && body_md.is_empty() {
            return Err(CoreError::EmptyContent.into());
        }
        item.title = if title.is_empty() {
            body_md.chars().take(MAX_TITLE_CHARS).collect()
        } else {
            title.to_owned()
        };
        item.title_origin = "user".to_owned();
        item.body_md = if body_md.is_empty() { title } else { body_md }.to_owned();
        if item.body_md.len() > MAX_BODY_MD_BYTES {
            return Err(CoreError::BodyTooLarge.into());
        }
        item.updated_at = updated_at.to_rfc3339_opts(SecondsFormat::Secs, true);

        let operation_id = new_id();
        let depends_on = self
            .connection
            .query_row(
                "SELECT id FROM outbox WHERE item_id = ?1 AND state <> 'acknowledged'
                 ORDER BY seq DESC LIMIT 1",
                [&item.id],
                |row| row.get(0),
            )
            .optional()?;
        let base_revision = self
            .shadow(&item.id)?
            .map_or(item.revision, |shadow| shadow.revision);
        let operation = PendingOperation {
            id: operation_id.clone(),
            item_id: item.id.clone(),
            method: "PUT".to_owned(),
            path: format!("/api/v1/items/{}", item.id),
            body: serde_json::to_string(&json!({
                "operation_id": operation_id,
                "base_revision": base_revision,
                "title": item.title,
                "body_md": item.body_md,
            }))?,
            depends_on,
            frozen: false,
            state: "queued".to_owned(),
            error: None,
        };
        self.transaction(|| {
            self.save_item(&item, "items")?;
            self.queue(&operation)
        })?;
        Ok(item)
    }

    pub fn item(&self, id: &str) -> Result<Option<Item>, StoreError> {
        self.lookup(id, "items")
    }

    pub fn shadow(&self, id: &str) -> Result<Option<Item>, StoreError> {
        self.lookup(id, "shadow")
    }

    pub fn apply_remote(&self, item: Item) -> Result<(), StoreError> {
        let has_pending = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM outbox WHERE item_id = ?1 AND state <> 'acknowledged')",
            [&item.id],
            |row| row.get::<_, bool>(0),
        )?;
        self.transaction(|| {
            self.save_item(&item, "shadow")?;
            if !has_pending {
                self.save_item(&item, "items")?;
            }
            Ok(())
        })
    }

    pub fn items(&self) -> Result<Vec<Item>, StoreError> {
        let mut statement = self.connection.prepare("SELECT json FROM items")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut items: Vec<Item> = Vec::new();
        for row in rows {
            items.push(serde_json::from_str(&row?)?);
        }
        items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(items)
    }

    pub fn apply_page(&self, items: &[Item], cursor: &str) -> Result<(), StoreError> {
        self.transaction(|| {
            for item in items {
                let has_pending = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM outbox
                     WHERE item_id = ?1 AND state <> 'acknowledged')",
                    [&item.id],
                    |row| row.get::<_, bool>(0),
                )?;
                self.save_item(item, "shadow")?;
                if !has_pending {
                    self.save_item(item, "items")?;
                }
            }
            self.set_meta("cursor", cursor)
        })
    }

    pub fn update_attachment(&self, attachment: &Attachment) -> Result<(), StoreError> {
        let item_id = self
            .items()?
            .into_iter()
            .find(|item| {
                item.attachments
                    .iter()
                    .any(|candidate| candidate.id == attachment.id)
            })
            .map(|item| item.id)
            .ok_or_else(|| StoreError::NotFound(attachment.id.clone()))?;
        self.transaction(|| {
            for table in ["items", "shadow"] {
                if let Some(mut item) = self.lookup(&item_id, table)? {
                    if let Some(index) = item
                        .attachments
                        .iter()
                        .position(|candidate| candidate.id == attachment.id)
                    {
                        item.attachments[index] = attachment.clone();
                        self.save_item(&item, table)?;
                    }
                }
            }
            Ok(())
        })
    }

    pub fn pending(&self) -> Result<Vec<PendingOperation>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT json FROM outbox WHERE state <> 'acknowledged' ORDER BY seq")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut operations = Vec::new();
        for row in rows {
            operations.push(serde_json::from_str(&row?)?);
        }
        Ok(operations)
    }

    pub fn prepare(
        &self,
        operation: &PendingOperation,
    ) -> Result<Option<PendingOperation>, StoreError> {
        if operation.state != "queued" {
            return Ok(None);
        }
        if operation.frozen {
            return Ok(Some(operation.clone()));
        }

        let mut prepared = operation.clone();
        if let Some(dependency) = &operation.depends_on {
            let dependency = self
                .connection
                .query_row(
                    "SELECT state, result_revision FROM outbox WHERE id = ?1",
                    [dependency],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let revision = match dependency {
                Some((state, Some(revision))) if state == "acknowledged" && revision >= 0 => {
                    revision
                }
                _ => return Ok(None),
            };
            let mut body: Value = serde_json::from_str(&operation.body)?;
            let object = body.as_object_mut().ok_or_else(|| {
                StoreError::InvalidState("pending operation body must be a JSON object".to_owned())
            })?;
            object.insert("base_revision".to_owned(), json!(revision));
            prepared.body = serde_json::to_string(&body)?;
        }
        prepared.frozen = true;
        self.connection.execute(
            "UPDATE outbox SET json = ?1 WHERE id = ?2",
            params![serde_json::to_string(&prepared)?, prepared.id],
        )?;
        Ok(Some(prepared))
    }

    pub fn acknowledge(
        &self,
        operation: &PendingOperation,
        remote: Item,
    ) -> Result<(), StoreError> {
        if operation.item_id != remote.id {
            return Err(StoreError::InvalidState(
                "acknowledged item does not match operation".to_owned(),
            ));
        }
        let revision = i64::try_from(remote.revision).map_err(|_| {
            StoreError::InvalidState("remote revision exceeds SQLite INTEGER".to_owned())
        })?;
        self.transaction(|| {
            self.connection.execute(
                "UPDATE outbox SET state = 'acknowledged', result_revision = ?1 WHERE id = ?2",
                params![revision, operation.id],
            )?;
            self.save_item(&remote, "shadow")?;
            let has_pending = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM outbox WHERE item_id = ?1 AND state <> 'acknowledged')",
                [&remote.id],
                |row| row.get::<_, bool>(0),
            )?;
            if !has_pending {
                self.save_item(&remote, "items")?;
            }
            Ok(())
        })
    }

    pub fn fail(
        &self,
        operation: &PendingOperation,
        error: &str,
        permanent: bool,
        conflict: Option<Item>,
    ) -> Result<PendingOperation, StoreError> {
        let mut failed = operation.clone();
        failed.state = if conflict.is_some() {
            "conflict"
        } else if permanent {
            "invalid"
        } else {
            "queued"
        }
        .to_owned();
        failed.error = Some(error.to_owned());
        self.transaction(|| {
            self.connection.execute(
                "UPDATE outbox SET state = ?1, json = ?2 WHERE id = ?3",
                params![failed.state, serde_json::to_string(&failed)?, failed.id],
            )?;
            if let Some(remote) = conflict {
                if let Some(local) = self.item(&failed.item_id)? {
                    self.save_item(&remote, "shadow")?;
                    self.connection.execute(
                        "INSERT INTO conflicts(item_id, local_json, remote_json) VALUES (?1, ?2, ?3)
                         ON CONFLICT(item_id) DO UPDATE SET remote_json = excluded.remote_json",
                        params![
                            failed.item_id,
                            serde_json::to_string(&local)?,
                            serde_json::to_string(&remote)?,
                        ],
                    )?;
                }
            }
            Ok(())
        })?;
        Ok(failed)
    }

    pub fn conflict_items(&self) -> Result<Vec<String>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT item_id FROM conflicts ORDER BY item_id")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn use_remote(&self, id: &str) -> Result<(), StoreError> {
        let Some(remote) = self.shadow(id)? else {
            return Ok(());
        };
        self.transaction(|| {
            self.connection.execute(
                "UPDATE outbox SET state = 'acknowledged' WHERE item_id = ?1",
                [id],
            )?;
            self.save_item(&remote, "items")?;
            self.connection
                .execute("DELETE FROM conflicts WHERE item_id = ?1", [id])?;
            Ok(())
        })
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(StoreError::from)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn recovery_errors(&self) -> &[String] {
        &self.recovery_errors
    }

    fn recover(&mut self) -> Result<(), StoreError> {
        let staging = self.root.join("staging");
        let entries = match fs::read_dir(&staging) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let result: Result<(), StoreError> = (|| {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    return Ok(());
                }
                let manifest: CaptureManifest =
                    serde_json::from_slice(&fs::read(entry.path().join("manifest.json"))?)?;
                if self.item(&manifest.item.id)?.is_none() {
                    self.validate_attachments(&manifest.item)?;
                    self.transaction(|| {
                        self.save_item(&manifest.item, "items")?;
                        self.queue(&manifest.operation)
                    })?;
                }
                fs::remove_dir_all(entry.path())?;
                Ok(())
            })();
            if let Err(error) = result {
                self.recovery_errors
                    .push(format!("attachment recovery failed: {error}"));
            }
        }
        Ok(())
    }

    fn validate_attachments(&self, item: &Item) -> Result<(), StoreError> {
        validate_attachment_limits(item)?;
        for attachment in &item.attachments {
            let bytes = fs::read(self.attachment_path(attachment))?;
            validate_attachment_bytes(attachment, &bytes)?;
        }
        Ok(())
    }

    fn lookup(&self, id: &str, table: &str) -> Result<Option<Item>, StoreError> {
        let sql = format!("SELECT json FROM {table} WHERE id = ?1");
        let encoded = self
            .connection
            .query_row(&sql, [id], |row| row.get::<_, String>(0))
            .optional()?;
        encoded
            .map(|json| serde_json::from_str(&json).map_err(StoreError::from))
            .transpose()
    }

    fn save_item(&self, item: &Item, table: &str) -> Result<(), StoreError> {
        let sql = format!(
            "INSERT INTO {table}(id, json) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json"
        );
        self.connection
            .execute(&sql, params![&item.id, serde_json::to_string(item)?])?;
        Ok(())
    }

    fn queue(&self, operation: &PendingOperation) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO outbox(id, item_id, json, state) VALUES (?1, ?2, ?3, ?4)",
            params![
                &operation.id,
                &operation.item_id,
                serde_json::to_string(operation)?,
                &operation.state
            ],
        )?;
        Ok(())
    }

    fn action(&self, item: &Item, action: &str) -> Result<PendingOperation, StoreError> {
        self.action_with_payload(item, action, json!({}))
    }

    fn action_with_payload(
        &self,
        item: &Item,
        action: &str,
        payload: Value,
    ) -> Result<PendingOperation, StoreError> {
        let id = new_id();
        let depends_on = self
            .connection
            .query_row(
                "SELECT id FROM outbox WHERE item_id = ?1 AND state <> 'acknowledged'
                 ORDER BY seq DESC LIMIT 1",
                [&item.id],
                |row| row.get(0),
            )
            .optional()?;
        let base_revision = self
            .shadow(&item.id)?
            .map_or(item.revision, |shadow| shadow.revision);
        let body = serde_json::to_string(&json!({
            "operation_id": id,
            "action": action,
            "base_revision": base_revision,
            "payload": payload,
        }))?;
        Ok(PendingOperation {
            id,
            item_id: item.id.clone(),
            method: "POST".to_owned(),
            path: format!("/api/v1/items/{}/actions", item.id),
            body,
            depends_on,
            frozen: false,
            state: "queued".to_owned(),
            error: None,
        })
    }

    fn transaction<T>(
        &self,
        body: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        match body() {
            Ok(value) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }
}

fn validate_attachment_limits(item: &Item) -> Result<(), StoreError> {
    if item.attachments.is_empty() {
        return Err(StoreError::InvalidState(
            "expected 1 to 10 attachments".to_owned(),
        ));
    }
    if uuid::Uuid::parse_str(&item.id).is_err() {
        return Err(StoreError::InvalidState("invalid item UUID".to_owned()));
    }
    let mut ids = std::collections::HashSet::new();
    for attachment in &item.attachments {
        if uuid::Uuid::parse_str(&attachment.id).is_err() {
            return Err(StoreError::InvalidState(
                "invalid attachment UUID".to_owned(),
            ));
        }
        if !ids.insert(&attachment.id) {
            return Err(StoreError::InvalidState(
                "unique attachment IDs are required".to_owned(),
            ));
        }
        if attachment.sha256.len() != 64
            || !attachment
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StoreError::InvalidState(
                "attachment SHA-256 must be 64 lowercase hex characters".to_owned(),
            ));
        }
    }
    if item.attachments.len() > 10 {
        return Err(StoreError::InvalidState(
            "each item supports at most 10 attachments".to_owned(),
        ));
    }
    if item
        .attachments
        .iter()
        .any(|a| a.byte_size == 0 || a.byte_size > 20 * 1024 * 1024)
    {
        return Err(StoreError::InvalidState(
            "each attachment must be at most 20 MiB".to_owned(),
        ));
    }
    let total = item.attachments.iter().try_fold(0_u64, |sum, a| {
        sum.checked_add(a.byte_size)
            .ok_or_else(|| StoreError::InvalidState("attachment sizes overflowed".to_owned()))
    })?;
    if total > 50 * 1024 * 1024 {
        return Err(StoreError::InvalidState(
            "attachments must total at most 50 MiB".to_owned(),
        ));
    }
    if item.attachments.iter().any(|a| {
        a.pixel_width == 0
            || a.pixel_height == 0
            || u64::from(a.pixel_width) * u64::from(a.pixel_height) > 40_000_000
    }) {
        return Err(StoreError::InvalidState(
            "attachments require positive dimensions and at most 40 MP".to_owned(),
        ));
    }
    Ok(())
}

fn validate_attachment_bytes(attachment: &Attachment, bytes: &[u8]) -> Result<(), StoreError> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    if bytes.len() as u64 != attachment.byte_size || digest != attachment.sha256 {
        return Err(StoreError::InvalidState(format!(
            "attachment {} failed size or SHA-256 validation",
            attachment.id
        )));
    }
    Ok(())
}

fn durable_write(bytes: &[u8], path: &Path) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::InvalidState(format!("attachment path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", new_id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn creation(item: &Item) -> Result<PendingOperation, StoreError> {
    let id = new_id();
    let body = serde_json::to_string(&json!({
        "operation_id": id,
        "item": item_input(item),
    }))?;
    Ok(PendingOperation {
        id,
        item_id: item.id.clone(),
        method: "POST".to_owned(),
        path: "/api/v1/items".to_owned(),
        body,
        depends_on: None,
        frozen: false,
        state: "queued".to_owned(),
        error: None,
    })
}

fn item_input(item: &Item) -> Value {
    let attachments: Vec<Value> = item
        .attachments
        .iter()
        .map(|attachment| {
            json!({
                "id": attachment.id,
                "mime": attachment.mime,
                "byte_size": attachment.byte_size,
                "sha256": attachment.sha256,
                "pixel_width": attachment.pixel_width,
                "pixel_height": attachment.pixel_height,
                "position": attachment.position,
                "caption": attachment.caption,
            })
        })
        .collect();
    json!({
        "id": item.id,
        "kind": item.kind,
        "task_state": item.task_state,
        "title": item.title,
        "title_origin": item.title_origin,
        "body_md": item.body_md,
        "timezone": item.timezone,
        "source": { "input_method": item.source.input_method },
        "attachments": attachments,
    })
}
