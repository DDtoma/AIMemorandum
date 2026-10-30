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
            .find(|monitor| monitor.is_primary())
            .or_else(|| monitors.first())
            .ok_or_else(|| UiError::Platform("X11 未报告可用显示器".to_owned()))?;
        let scale = monitor.scale_factor() as f64;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(UiError::Platform("X11 显示器缩放无效".to_owned()));
        }
        let geometry = MonitorGeometry {
            x: monitor.x() as f64,
            y: monitor.y() as f64,
            width: monitor.width() as f64 / scale,
            height: monitor.height() as f64 / scale,
            scale_factor: scale,
        };
        let image = monitor.capture_image().map_err(platform_error)?;
        StaticFrame::new(image, geometry)
    }
}

fn platform_error(error: impl std::fmt::Display) -> UiError {
    UiError::Platform(error.to_string())
}
