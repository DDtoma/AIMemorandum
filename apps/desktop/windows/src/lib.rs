//! Windows executable integration for AIMemorandum.

mod clipboard;
mod command;
mod hotkeys;
#[cfg(windows)]
mod ipc;
mod ocr;
mod paths;
mod screenshot;
mod shell;
mod support;
mod tray;

pub use clipboard::{save_rgba_png, WindowsClipboard};
pub use command::{decode_command, encode_command, parse_args, CliAction, CommandError};
pub use hotkeys::{HotkeyBinding, HotkeyBindings, NativeHotkeys};
#[cfg(windows)]
pub use ipc::{pipe_name, IpcClient, IpcServer};
pub use ocr::{fit_within_max_dimension, OcrError, WindowsOcr};
pub use paths::{data_dir, data_dir_from};
pub use screenshot::{
    monitor_geometries, normalized_selection, plan_capture, project_selection,
    virtual_desktop_bounds, CapturePlan, CaptureTile, LogicalRect, MonitorGeometry, PhysicalPoint,
    PhysicalRect, SelectionDrag, WindowsScreenshot,
};
pub use shell::{autostart_command, WindowsDesktopShell};
pub use tray::{menu_action, NativeTray, TrayAction};
