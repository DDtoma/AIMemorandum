use crate::{platform_error, LogicalRegion, StaticFrame};
use aimemorandum_ui::{CaptureRegion, CapturedImage, DisplayGeometry, Screenshot, UiError};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub trait FrameProvider: Send {
    fn capture_frame(&mut self) -> Result<StaticFrame, UiError>;
}

struct CachedFrame {
    frame: StaticFrame,
    image: CapturedImage,
}

pub struct ScreenshotAdapter {
    provider: Mutex<Box<dyn FrameProvider>>,
    cached: Mutex<Option<CachedFrame>>,
    capture_dir: PathBuf,
}

impl ScreenshotAdapter {
    pub fn new(provider: Box<dyn FrameProvider>, capture_dir: PathBuf) -> Self {
        Self {
            provider: Mutex::new(provider),
            cached: Mutex::new(None),
            capture_dir,
        }
    }

    pub fn capture_full(&self) -> Result<CapturedImage, UiError> {
        let frame = self
            .provider
            .lock()
            .map_err(|_| UiError::Platform("截图提供器锁已中毒".to_owned()))?
            .capture_frame()?;
        let image = write_png(frame.image(), &self.capture_dir)?;
        let mut cached = self
            .cached
            .lock()
            .map_err(|_| UiError::Platform("截图缓存锁已中毒".to_owned()))?;
        if let Some(previous) = cached.replace(CachedFrame {
            frame,
            image: image.clone(),
        }) {
            let _ = std::fs::remove_file(previous.image.path);
        }
        Ok(image)
    }

    pub fn capture_display(&self) -> Option<DisplayGeometry> {
        let cached = self.cached.lock().ok()?;
        cached.as_ref().map(|current| {
            let monitor = current.frame.monitor();
            DisplayGeometry {
                x: monitor.x,
                y: monitor.y,
                width: monitor.width,
                height: monitor.height,
            }
        })
    }

    pub fn cancel_capture(&self) {
        let cached = self.cached.lock().ok().and_then(|mut cached| cached.take());
        if let Some(cached) = cached {
            let _ = std::fs::remove_file(cached.image.path);
        }
    }

    pub fn release_capture(&self, image: &CapturedImage) {
        let _ = std::fs::remove_file(&image.path);
    }

    pub fn capture(&self, region: CaptureRegion) -> Result<CapturedImage, UiError> {
        let current = self
            .cached
            .lock()
            .map_err(|_| UiError::Platform("截图缓存锁已中毒".to_owned()))?
            .take()
            .ok_or_else(|| {
                UiError::InvalidState("请先通过 BeginScreenshot 获取静态帧".to_owned())
            })?;
        let full_path = current.image.path.clone();
        let result = (|| {
            let monitor = current.frame.monitor();
            let cropped = current
                .frame
                .crop(LogicalRegion {
                    // Slint reports selection coordinates relative to the overlay window.
                    x: region.x + f64::from(monitor.x),
                    y: region.y + f64::from(monitor.y),
                    width: region.width,
                    height: region.height,
                })
                .map_err(|error| UiError::Platform(error.to_string()))?;
            write_png(&cropped, &self.capture_dir)
        })();
        let _ = std::fs::remove_file(full_path);
        result
    }
}

impl Drop for ScreenshotAdapter {
    fn drop(&mut self) {
        self.cancel_capture();
    }
}

impl Screenshot for ScreenshotAdapter {
    fn capture_full(&self) -> Result<CapturedImage, UiError> {
        ScreenshotAdapter::capture_full(self)
    }

    fn capture(&self, region: CaptureRegion) -> Result<CapturedImage, UiError> {
        ScreenshotAdapter::capture(self, region)
    }

    fn capture_display(&self) -> Option<DisplayGeometry> {
        ScreenshotAdapter::capture_display(self)
    }

    fn cancel_capture(&self) {
        ScreenshotAdapter::cancel_capture(self);
    }

    fn release_capture(&self, image: &CapturedImage) {
        ScreenshotAdapter::release_capture(self, image);
    }
}

pub(crate) fn write_png(image: &RgbaImage, directory: &Path) -> Result<CapturedImage, UiError> {
    std::fs::create_dir_all(directory).map_err(platform_error)?;
    let path = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&temporary).map_err(platform_error)?;
        let mut writer = BufWriter::new(file);
        image::codecs::png::PngEncoder::new(&mut writer)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(platform_error)?;
        writer.flush().map_err(platform_error)?;
        writer.get_ref().sync_all().map_err(platform_error)?;
        drop(writer);
        std::fs::rename(&temporary, &path).map_err(platform_error)?;
        Ok(CapturedImage {
            path,
            mime: "image/png".to_owned(),
            pixel_width: image.width(),
            pixel_height: image.height(),
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
