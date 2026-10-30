use crate::{set_autostart_at, write_png};
use aimemorandum_ui::{Clipboard, Clock, DesktopShell, UiError};
use arboard::{Clipboard as Arboard, ImageData};
use chrono::{DateTime, Local, TimeZone};
use image::RgbaImage;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    process::Command,
};

pub struct LinuxClipboard {
    capture_dir: PathBuf,
}

impl LinuxClipboard {
    pub fn new(capture_dir: PathBuf) -> Self {
        Self { capture_dir }
    }
}

impl Clipboard for LinuxClipboard {
    fn read_text(&self) -> Result<Option<String>, UiError> {
        let mut clipboard = Arboard::new().map_err(platform_error)?;
        match clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(platform_error(error)),
        }
    }

    fn read_image(&self) -> Result<Option<aimemorandum_ui::CapturedImage>, UiError> {
        let mut clipboard = Arboard::new().map_err(platform_error)?;
        let image = match clipboard.get_image() {
            Ok(image) => image,
            Err(arboard::Error::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(platform_error(error)),
        };
        let ImageData {
            width,
            height,
            bytes,
        } = image;
        let image = RgbaImage::from_raw(width as u32, height as u32, bytes.into_owned())
            .ok_or_else(|| UiError::Platform("剪贴板图像的 RGBA 缓冲区无效".to_owned()))?;
        write_png(&image, &self.capture_dir).map(Some)
    }
}

pub struct LinuxDesktopShell {
    autostart_path: PathBuf,
    executable: PathBuf,
}

impl LinuxDesktopShell {
    pub fn new(autostart_path: PathBuf, executable: PathBuf) -> Self {
        Self {
            autostart_path,
            executable,
        }
    }
}

impl DesktopShell for LinuxDesktopShell {
    fn set_autostart(&self, enabled: bool) -> Result<(), UiError> {
        set_autostart_at(&self.autostart_path, &self.executable, enabled).map_err(platform_error)
    }

    fn open_attachment(&self, path: &Path) -> Result<(), UiError> {
        if !path.is_file() {
            return Err(UiError::NotFound(format!("附件不存在：{}", path.display())));
        }
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(platform_error)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Local> {
        Local::now()
    }

    fn timezone(&self) -> String {
        iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_owned())
    }
}

fn platform_error(error: impl std::fmt::Display) -> UiError {
    UiError::Platform(error.to_string())
}
