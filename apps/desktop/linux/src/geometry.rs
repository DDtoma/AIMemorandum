#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GeometryError {
    #[error("capture metadata did not include the output position")]
    MissingOutputPosition,
    #[error("capture metadata did not include the logical output size")]
    MissingLogicalSize,
    #[error("captured output has non-uniform scaling")]
    NonUniformScaling,
    #[error("monitor scale and region coordinates must be finite")]
    NonFinite,
    #[error("region size and monitor scale must be positive")]
    NotPositive,
    #[error("region is outside the monitor")]
    OutsideMonitor,
    #[error("rounded region is empty or too large")]
    InvalidPixels,
}

impl MonitorGeometry {
    pub fn from_capture_metadata(
        position: Option<(i32, i32)>,
        logical_size: Option<(i32, i32)>,
        physical_width: u32,
        physical_height: u32,
    ) -> Result<Self, GeometryError> {
        let (x, y) = position.ok_or(GeometryError::MissingOutputPosition)?;
        let (width, height) = logical_size.ok_or(GeometryError::MissingLogicalSize)?;
        if width <= 0 || height <= 0 || physical_width == 0 || physical_height == 0 {
            return Err(GeometryError::NotPositive);
        }
        let scale_x = f64::from(physical_width) / f64::from(width);
        let scale_y = f64::from(physical_height) / f64::from(height);
        if (scale_x - scale_y).abs() > 0.01 {
            return Err(GeometryError::NonUniformScaling);
        }
        Ok(Self {
            x,
            y,
            width: width as u32,
            height: height as u32,
            scale_factor: scale_x,
        })
    }

    pub fn physical_crop(&self, region: LogicalRegion) -> Result<PhysicalRegion, GeometryError> {
        if ![
            region.x,
            region.y,
            region.width,
            region.height,
            self.scale_factor,
        ]
        .iter()
        .all(|value| value.is_finite())
        {
            return Err(GeometryError::NonFinite);
        }
        if region.width <= 0.0 || region.height <= 0.0 || self.scale_factor <= 0.0 {
            return Err(GeometryError::NotPositive);
        }
        let local_x = region.x - f64::from(self.x);
        let local_y = region.y - f64::from(self.y);
        if local_x < 0.0
            || local_y < 0.0
            || local_x + region.width > f64::from(self.width)
            || local_y + region.height > f64::from(self.height)
        {
            return Err(GeometryError::OutsideMonitor);
        }
        let left = (local_x * self.scale_factor).round();
        let top = (local_y * self.scale_factor).round();
        let right = ((local_x + region.width) * self.scale_factor).round();
        let bottom = ((local_y + region.height) * self.scale_factor).round();
        if right <= left || bottom <= top || right > u32::MAX as f64 || bottom > u32::MAX as f64 {
            return Err(GeometryError::InvalidPixels);
        }
        Ok(PhysicalRegion {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        })
    }
}
