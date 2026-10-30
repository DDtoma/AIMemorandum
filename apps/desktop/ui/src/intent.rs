use crate::ItemFilter;
use aimemorandum_core::SubmitKey;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum UiIntent {
    CaptureDraftChanged(String),
    CaptureKey {
        key: SubmitKey,
        shift: bool,
        marked: bool,
    },
    SubmitCapture,
    CaptureClipboard,
    DropImages(Vec<PathBuf>),
    Refresh,
    SelectItem(String),
    SetFilter(ItemFilter),
    DetailDraftChanged {
        title: String,
        body_md: String,
    },
    SaveSelected,
    CompleteItem(String),
    ReopenItem(String),
    DeleteItem(String),
    RemindItem {
        item_id: String,
        remind_at: DateTime<Utc>,
    },
    RemindItemInTwoHours(String),
    OpenAttachment {
        item_id: String,
        attachment_id: String,
    },
    CheckReminders,
    CompleteReminder,
    SnoozeReminder,
    DismissReminder,
    BeginScreenshot,
    ScreenshotStarted {
        x: f64,
        y: f64,
    },
    ScreenshotMoved {
        x: f64,
        y: f64,
    },
    ScreenshotFinished {
        x: f64,
        y: f64,
    },
    ScreenshotCancelled,
    SettingsChanged {
        server_url: String,
        enrollment_code: String,
        device_name: String,
    },
    Register,
    SetAutostart(bool),
}
