use crate::UiError;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedImage {
    pub path: PathBuf,
    pub mime: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCredentials {
    pub device_token: String,
    pub device_name: String,
}

pub trait Screenshot {
    fn capture_full(&self) -> Result<CapturedImage, UiError> {
        Err(UiError::Platform("当前平台尚未实现静态全屏截图".to_owned()))
    }

    fn capture(&self, region: CaptureRegion) -> Result<CapturedImage, UiError>;

    fn capture_display(&self) -> Option<DisplayGeometry> {
        None
    }

    fn cancel_capture(&self) {}

    fn release_capture(&self, _image: &CapturedImage) {}
}

pub trait Clipboard {
    fn read_text(&self) -> Result<Option<String>, UiError>;
    fn read_image(&self) -> Result<Option<CapturedImage>, UiError>;
    fn release_image(&self, _image: &CapturedImage) {}
}

pub trait Enrollment {
    fn enroll(
        &self,
        server_url: &str,
        enrollment_code: &str,
        device_name: &str,
    ) -> Result<DeviceCredentials, UiError>;
}

pub trait DesktopShell {
    fn set_autostart(&self, enabled: bool) -> Result<(), UiError>;
    fn open_attachment(&self, path: &Path) -> Result<(), UiError>;
}

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
    fn timezone(&self) -> String;
}

pub struct CoreEnrollment;

impl Enrollment for CoreEnrollment {
    fn enroll(
        &self,
        server_url: &str,
        enrollment_code: &str,
        device_name: &str,
    ) -> Result<DeviceCredentials, UiError> {
        let enrolled = aimemorandum_core::ApiClient::enroll(
            server_url.trim(),
            enrollment_code.trim(),
            device_name,
        )
        .map_err(|error| UiError::Platform(error.to_string()))?;
        Ok(DeviceCredentials {
            device_token: enrolled.device_token,
            device_name: enrolled.device_name,
        })
    }
}
