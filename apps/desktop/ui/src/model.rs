use crate::platform::DisplayGeometry;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemFilter {
    #[default]
    All,
    Active,
    Waiting,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapturePanelModel {
    pub draft: String,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRowModel {
    pub id: String,
    pub title: String,
    pub state: String,
    pub waiting_reason: String,
    pub updated_at: String,
    pub synchronized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentModel {
    pub id: String,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDetailModel {
    pub id: String,
    pub title: String,
    pub body_md: String,
    pub state: String,
    pub waiting_reason: String,
    pub synchronized: bool,
    pub attachments: Vec<AttachmentModel>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemsWindowModel {
    pub rows: Vec<ItemRowModel>,
    pub selected: Option<ItemDetailModel>,
    pub filter: ItemFilter,
    pub total_count: usize,
    pub active_count: usize,
    pub waiting_count: usize,
    pub completed_count: usize,
    pub pending_count: usize,
    pub conflict_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderPopupModel {
    pub item_id: String,
    pub reminder_id: String,
    pub expected_schedule: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsModel {
    pub server_url: String,
    pub enrollment_code: String,
    pub device_name: String,
    pub registered: bool,
    pub autostart: bool,
    pub message: Option<String>,
    pub error: Option<String>,
}

impl Default for SettingsModel {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8787".to_owned(),
            enrollment_code: String::new(),
            device_name: "desktop".to_owned(),
            registered: false,
            autostart: false,
            message: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayEntry {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayModel {
    pub open_count: usize,
    pub entries: Vec<TrayEntry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScreenshotOverlayModel {
    pub active: bool,
    pub background: Option<PathBuf>,
    pub display: Option<DisplayGeometry>,
    pub start_x: Option<f64>,
    pub start_y: Option<f64>,
    pub current_x: f64,
    pub current_y: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewModel {
    pub capture: CapturePanelModel,
    pub items: ItemsWindowModel,
    pub reminder: Option<ReminderPopupModel>,
    pub settings: SettingsModel,
    pub tray: TrayModel,
    pub screenshot: ScreenshotOverlayModel,
}
