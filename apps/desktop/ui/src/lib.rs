//! Shared desktop interaction controller and Slint views.

mod controller;
mod error;
mod intent;
mod model;
mod overlay_output;
mod platform;
mod slint_shell;
mod worker;

pub use controller::UiController;
pub use error::UiError;
pub use intent::UiIntent;
pub use model::*;
pub use overlay_output::{
    output_geometry_for_capture, select_overlay_output, OutputMatchError, OutputRoute,
    ScreenshotOutputRouter,
};
pub use platform::*;

pub use worker::{Controller, UiIntentSender, UiUpdate, UiWorker};

pub use slint_shell::{
    run_slint, AttachmentData, CapturePanel, ItemRowData, ItemsWindow, ReminderPopup,
    ScreenshotOverlay, Settings, ShellEvent, SlintShell,
};
