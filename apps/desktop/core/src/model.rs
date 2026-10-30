use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_BODY_MD_BYTES: usize = 262_144;
pub const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("content, title, or an attachment is required")]
    EmptyContent,
    #[error("body_md exceeds 262144 UTF-8 bytes")]
    BodyTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Capture,
    Task,
    Memo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Todo,
    InProgress,
    Waiting,
    Paused,
    Completed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub input_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_app: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub mime: String,
    pub byte_size: u64,
    pub sha256: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub position: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    pub state: String,
    pub revision: u64,
}

impl Attachment {
    pub fn new(
        mime: impl Into<String>,
        byte_size: u64,
        sha256: impl Into<String>,
        pixel_width: u32,
        pixel_height: u32,
        position: u32,
    ) -> Self {
        Self {
            id: new_id(),
            owner_id: None,
            item_id: None,
            mime: mime.into(),
            byte_size,
            sha256: sha256.into(),
            pixel_width,
            pixel_height,
            position,
            caption: None,
            state: "pending".to_owned(),
            revision: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub item_id: String,
    pub remind_at: String,
    pub schedule_revision: String,
    pub purpose: String,
    pub enabled: bool,
    pub revision: u64,
}

impl Reminder {
    pub fn request_id(&self) -> String {
        format!("{}:{}:{}", self.id, self.schedule_revision, self.remind_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOperation {
    pub id: String,
    pub item_id: String,
    pub method: String,
    pub path: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<String>,
    pub frozen: bool,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub kind: ItemKind,
    #[serde(default)]
    pub task_state: Option<TaskState>,
    pub title: String,
    pub title_origin: String,
    pub body_md: String,
    pub timezone: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub planned_for: Option<String>,
    #[serde(default)]
    pub review_at: Option<String>,
    pub next_action: String,
    pub resume_note: String,
    pub source: Source,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reminders: Vec<Reminder>,
}

impl Item {
    pub fn new_task(
        text: impl Into<String>,
        input_method: impl Into<String>,
        now: DateTime<Utc>,
        timezone: impl Into<String>,
        attachments: Vec<Attachment>,
    ) -> Result<Self, CoreError> {
        let text = text.into();
        let input_method = input_method.into();
        if text.len() > MAX_BODY_MD_BYTES {
            return Err(CoreError::BodyTooLarge);
        }
        if text.trim().is_empty() && attachments.is_empty() {
            return Err(CoreError::EmptyContent);
        }

        let generated_title = text.is_empty() || input_method == "screenshot";
        let title = if generated_title {
            format!("截图 · {}", now.format("%Y-%m-%d %H:%M"))
        } else {
            text.chars().take(MAX_TITLE_CHARS).collect()
        };
        let timestamp = now.to_rfc3339_opts(SecondsFormat::Secs, true);

        Ok(Self {
            id: new_id(),
            owner_id: None,
            kind: ItemKind::Task,
            task_state: Some(TaskState::Todo),
            title,
            title_origin: if generated_title { "generated" } else { "user" }.to_owned(),
            body_md: text,
            timezone: timezone.into(),
            revision: 0,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            completed_at: None,
            deleted_at: None,
            due_at: None,
            due_date: None,
            planned_for: None,
            review_at: None,
            next_action: String::new(),
            resume_note: String::new(),
            source: Source {
                input_method,
                reported_app: None,
            },
            attachments,
            reminders: Vec::new(),
        })
    }

    pub fn ended(&self) -> bool {
        matches!(
            self.task_state,
            Some(TaskState::Completed | TaskState::Canceled)
        ) || self.deleted_at.is_some()
    }
}

pub(crate) fn new_id() -> String {
    Uuid::new_v4().hyphenated().to_string()
}
