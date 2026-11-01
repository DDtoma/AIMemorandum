use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::support::wide;
use aimemorandum_ui::{DesktopShell, UiError};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "AIMemorandum";

pub fn autostart_command(executable: &Path) -> String {
    format!(r#""{}" --background"#, executable.display())
}

pub fn autostart_registry_value(executable: &Path) -> Vec<u16> {
    wide(&autostart_command(executable))
}

#[derive(Clone, Debug)]
pub struct WindowsDesktopShell {
    executable: PathBuf,
}

impl WindowsDesktopShell {
    pub fn new() -> Result<Self, UiError> {
        Ok(Self {
            executable: std::env::current_exe()
                .map_err(|error| UiError::Platform(format!("current executable: {error}")))?,
        })
    }
}

impl DesktopShell for WindowsDesktopShell {
    fn set_autostart(&self, enabled: bool) -> Result<(), UiError> {
        let key = open_run_key()?;
        if enabled {
            let command = autostart_registry_value(&self.executable);
            let status = unsafe {
                RegSetValueExW(
                    key.0,
                    wide(RUN_VALUE).as_ptr(),
                    0,
                    REG_SZ,
                    command.as_ptr().cast(),
                    (command.len() * size_of::<u16>()) as u32,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(registry_error("write autostart command", status));
            }
        } else {
            let status = unsafe { RegDeleteValueW(key.0, wide(RUN_VALUE).as_ptr()) };
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(registry_error("remove autostart command", status));
            }
        }
        Ok(())
    }

    fn open_attachment(&self, path: &Path) -> Result<(), UiError> {
        let result = unsafe {
            ShellExecuteW(
                null_mut(),
                wide("open").as_ptr(),
                wide(&path.to_string_lossy()).as_ptr(),
                null_mut(),
                null_mut(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err(UiError::Platform(format!(
                "Windows could not open attachment (ShellExecute code {})",
                result as isize
            )));
        }
        Ok(())
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn open_run_key() -> Result<RegistryKey, UiError> {
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null_mut(),
            &mut key,
            null_mut(),
        )
    };
    if status == ERROR_SUCCESS {
        Ok(RegistryKey(key))
    } else {
        Err(registry_error("open Run key", status))
    }
}

fn registry_error(action: &str, status: u32) -> UiError {
    UiError::Platform(format!("{action}: Windows error {status}"))
}
