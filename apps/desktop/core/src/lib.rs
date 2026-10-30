//! Shared, platform-independent desktop domain and synchronization core.

mod input;
mod model;
mod store;

pub use input::{InputPolicy, SubmitKey};
pub use model::{
    Attachment, CoreError, Item, ItemKind, PendingOperation, Reminder, Source, TaskState,
    MAX_BODY_MD_BYTES, MAX_TITLE_CHARS,
};
pub use store::{LocalStore, StagedCapture, StoreError};
