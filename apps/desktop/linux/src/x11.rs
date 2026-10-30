use crate::{FrameProvider, MonitorGeometry, StaticFrame};
use aimemorandum_ui::UiError;
use xcap::Monitor;

#[derive(Clone, Copy, Debug, Default)]
pub struct X11FrameProvider;

impl FrameProvider for X11FrameProvider {
    fn capture_frame(&mut self) -> Result<StaticFrame, UiError> {
        let monitors = Monitor::all().map_err(platform_error)?;
        let monitor = monitors
            .iter()
            .find(|monitor| matches!(monitor.is_primary(), Ok(true)))
            .or_else(|| monitors.first())
            .ok_or_else(|| UiError::Platform("X11 未报告可用显示器".to_owned()))?;
        let scale = monitor.scale_factor().map_err(platform_error)? as f64;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(UiError::Platform("X11 显示器缩放无效".to_owned()));
        }
        let geometry = MonitorGeometry {
            x: monitor.x().map_err(platform_error)?,
            y: monitor.y().map_err(platform_error)?,
            // xcap's Linux monitor dimensions are already logical coordinates.
            width: monitor.width().map_err(platform_error)?,
            height: monitor.height().map_err(platform_error)?,
            scale_factor: scale,
        };
        let image = monitor.capture_image().map_err(platform_error)?;
        Ok(StaticFrame::new(image, geometry))
    }
}

fn platform_error(error: impl std::fmt::Display) -> UiError {
    UiError::Platform(error.to_string())
}
