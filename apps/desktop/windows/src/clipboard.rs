use crate::support::platform;
use aimemorandum_ui::{CapturedImage, Clipboard, UiError};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct WindowsClipboard {
    captures_dir: PathBuf,
}

impl WindowsClipboard {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            captures_dir: crate::data_dir()?.join("captures"),
        })
    }

    fn clipboard() -> Result<arboard::Clipboard, UiError> {
        arboard::Clipboard::new().map_err(platform)
    }
}

impl Clipboard for WindowsClipboard {
    fn read_text(&self) -> Result<Option<String>, UiError> {
        match Self::clipboard()?.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(platform(error)),
        }
    }

    fn read_image(&self) -> Result<Option<CapturedImage>, UiError> {
        let image = match Self::clipboard()?.get_image() {
            Ok(image) => image,
            Err(arboard::Error::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(platform(error)),
        };
        fs::create_dir_all(&self.captures_dir).map_err(UiError::Io)?;
        let path = self
            .captures_dir
            .join(format!("clipboard-{}.png", uuid::Uuid::new_v4()));
        let width = u32::try_from(image.width)
            .map_err(|_| UiError::Platform("clipboard image is too wide".into()))?;
        let height = u32::try_from(image.height)
            .map_err(|_| UiError::Platform("clipboard image is too tall".into()))?;
        save_rgba_png(&path, width, height, &image.bytes).map_err(UiError::Io)?;
        Ok(Some(CapturedImage {
            path,
            mime: "image/png".into(),
            pixel_width: width,
            pixel_height: height,
        }))
    }
}

pub fn save_rgba_png(path: &Path, width: u32, height: u32, bytes: &[u8]) -> io::Result<()> {
    let expected = width as usize * height as usize * 4;
    if bytes.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RGBA buffer length does not match image dimensions",
        ));
    }
    let image = image::RgbaImage::from_raw(width, height, bytes.to_vec())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid RGBA image"))?;
    image
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
