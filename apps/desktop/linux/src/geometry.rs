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
