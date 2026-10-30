//! Shared, platform-independent desktop domain and synchronization core.

mod api;

mod input;
mod model;
mod reminder;
mod store;
mod sync;

pub use api::{ApiClient, ApiError, Enrollment, SyncApi, SyncPage};
pub use input::{InputPolicy, SubmitKey};
pub use model::{
    Attachment, CoreError, Item, ItemKind, PendingOperation, Reminder, Source, TaskState,
    MAX_BODY_MD_BYTES, MAX_TITLE_CHARS,
};
pub use reminder::{DueReminder, ReminderScheduler};
pub use store::{LocalStore, StagedCapture, StoreError};
pub use sync::{SyncEngine, SyncError, SyncStats};
