//! Linux executable integration for AIMemorandum.

mod autostart;
mod capture;
mod command;
mod frame;
mod geometry;
mod paths;
mod session;
mod tray;

#[cfg(target_os = "linux")]
mod hotkey;
#[cfg(target_os = "linux")]
mod ipc;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod tray_service;
#[cfg(target_os = "linux")]
mod x11;

pub use autostart::set_autostart_at;
pub use capture::{FrameProvider, ScreenshotAdapter};
pub use command::{decode_command, encode_command, parse_args, CliAction, CommandError};
pub use frame::{FrameError, StaticFrame};
pub use geometry::{GeometryError, LogicalRegion, MonitorGeometry, PhysicalRegion};
#[cfg(target_os = "linux")]
pub use hotkey::{
    start_hotkeys, start_wayland_hotkeys, start_x11_hotkeys, HotkeyService, PortalShortcutService,
};
#[cfg(target_os = "linux")]
pub use ipc::{send_command, IpcError, PrimaryInstance};
pub use paths::{
    autostart_path, autostart_path_from, data_dir, data_dir_from, socket_path, socket_path_from,
};
#[cfg(target_os = "linux")]
pub use platform::{LinuxClipboard, LinuxDesktopShell, SystemClock};
#[cfg(target_os = "linux")]
pub use portal::PortalFrameProvider;
pub use session::{detect_session, detect_session_from, SessionError, SessionKind};
pub use tray::{TrayItem, TrayMenuEntry, TrayMenuModel};
#[cfg(target_os = "linux")]
pub use tray_service::TrayService;
#[cfg(target_os = "linux")]
pub use x11::X11FrameProvider;
