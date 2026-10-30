use crate::{GeometryError, LogicalRegion, MonitorGeometry};
use image::RgbaImage;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    #[error("physical crop exceeds the captured pixel buffer")]
    PixelBounds,
}

#[derive(Debug, Clone)]
pub struct StaticFrame {
    image: RgbaImage,
    monitor: MonitorGeometry,
}

impl StaticFrame {
    pub fn new(image: RgbaImage, monitor: MonitorGeometry) -> Self {
        Self { image, monitor }
    }

    pub fn monitor(&self) -> MonitorGeometry {
        self.monitor
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn crop(&self, region: LogicalRegion) -> Result<RgbaImage, FrameError> {
        let crop = self.monitor.physical_crop(region)?;
        let right = crop
            .x
            .checked_add(crop.width)
            .ok_or(FrameError::PixelBounds)?;
        let bottom = crop
            .y
            .checked_add(crop.height)
            .ok_or(FrameError::PixelBounds)?;
        if right > self.image.width() || bottom > self.image.height() {
            return Err(FrameError::PixelBounds);
        }
        Ok(
            image::imageops::crop_imm(&self.image, crop.x, crop.y, crop.width, crop.height)
                .to_image(),
        )
    }
}
