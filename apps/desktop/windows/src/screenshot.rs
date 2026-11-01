use crate::support::platform;
use aimemorandum_ui::{CaptureRegion, CapturedImage, Screenshot, UiError};
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureTile {
    pub monitor_index: usize,
    pub source_x: u32,
    pub source_y: u32,
    pub destination_x: u32,
    pub destination_y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturePlan {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<CaptureTile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPoint {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Default)]
pub struct SelectionDrag {
    start: Option<PhysicalPoint>,
    current: Option<PhysicalPoint>,
}

impl SelectionDrag {
    pub fn begin(&mut self, point: PhysicalPoint) {
        self.start = Some(point);
        self.current = Some(point);
    }

    pub fn update(&mut self, point: PhysicalPoint) -> Option<PhysicalRect> {
        self.start?;
        self.current = Some(point);
        self.selection()
    }

    pub fn finish(&mut self, point: PhysicalPoint) -> Option<PhysicalRect> {
        let start = self.start.take()?;
        self.current = None;
        Some(normalized_selection(start, point))
    }

    pub fn cancel(&mut self) -> bool {
        self.current = None;
        self.start.take().is_some()
    }

    pub fn selection(&self) -> Option<PhysicalRect> {
        Some(normalized_selection(self.start?, self.current?))
    }
}

pub fn normalized_selection(start: PhysicalPoint, end: PhysicalPoint) -> PhysicalRect {
    PhysicalRect {
        left: start.x.min(end.x),
        top: start.y.min(end.y),
        right: start.x.max(end.x),
        bottom: start.y.max(end.y),
    }
}

pub fn project_selection(
    selection: PhysicalRect,
    window_origin: PhysicalPoint,
    scale: f64,
) -> Option<LogicalRect> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some(LogicalRect {
        x: selection.left.checked_sub(window_origin.x)? as f64 / scale,
        y: selection.top.checked_sub(window_origin.y)? as f64 / scale,
        width: selection.right.checked_sub(selection.left)? as f64 / scale,
        height: selection.bottom.checked_sub(selection.top)? as f64 / scale,
    })
}

pub fn virtual_desktop_bounds(monitors: &[MonitorGeometry]) -> Result<MonitorGeometry, UiError> {
    if monitors.is_empty() {
        return Err(UiError::Platform("没有可用显示器".to_owned()));
    }
    let mut left = i64::MAX;
    let mut top = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for monitor in monitors {
        let x = i64::from(monitor.x);
        let y = i64::from(monitor.y);
        left = left.min(x);
        top = top.min(y);
        right = right.max(
            x.checked_add(i64::from(monitor.width))
                .ok_or_else(|| UiError::InvalidState("显示器水平坐标超出范围".to_owned()))?,
        );
        bottom = bottom.max(
            y.checked_add(i64::from(monitor.height))
                .ok_or_else(|| UiError::InvalidState("显示器垂直坐标超出范围".to_owned()))?,
        );
    }
    Ok(MonitorGeometry {
        x: i32::try_from(left)
            .map_err(|_| UiError::InvalidState("虚拟桌面原点超出范围".to_owned()))?,
        y: i32::try_from(top)
            .map_err(|_| UiError::InvalidState("虚拟桌面原点超出范围".to_owned()))?,
        width: u32::try_from(right - left)
            .map_err(|_| UiError::InvalidState("虚拟桌面过宽".to_owned()))?,
        height: u32::try_from(bottom - top)
            .map_err(|_| UiError::InvalidState("虚拟桌面过高".to_owned()))?,
    })
}

pub fn monitor_geometries() -> Result<Vec<MonitorGeometry>, UiError> {
    geometries(&xcap::Monitor::all().map_err(platform)?)
}

fn geometries(monitors: &[xcap::Monitor]) -> Result<Vec<MonitorGeometry>, UiError> {
    monitors
        .iter()
        .map(|monitor| {
            Ok(MonitorGeometry {
                x: monitor.x().map_err(platform)?,
                y: monitor.y().map_err(platform)?,
                width: monitor.width().map_err(platform)?,
                height: monitor.height().map_err(platform)?,
            })
        })
        .collect()
}

pub fn plan_capture(
    region: CaptureRegion,
    monitors: &[MonitorGeometry],
) -> Result<CapturePlan, UiError> {
    if !region.x.is_finite()
        || !region.y.is_finite()
        || !region.width.is_finite()
        || !region.height.is_finite()
        || region.width <= 0.0
        || region.height <= 0.0
    {
        return Err(UiError::InvalidState(
            "截图区域必须是有限且非空的矩形".to_owned(),
        ));
    }
    let left = floor_i64(region.x)?;
    let top = floor_i64(region.y)?;
    let right = ceil_i64(region.x + region.width)?;
    let bottom = ceil_i64(region.y + region.height)?;
    if right <= left || bottom <= top {
        return Err(UiError::InvalidState("截图区域为空".to_owned()));
    }
    let width = u32::try_from(right - left)
        .map_err(|_| UiError::InvalidState("截图区域过宽".to_owned()))?;
    let height = u32::try_from(bottom - top)
        .map_err(|_| UiError::InvalidState("截图区域过高".to_owned()))?;
    let mut tiles = Vec::new();
    for (monitor_index, monitor) in monitors.iter().enumerate() {
        let monitor_left = i64::from(monitor.x);
        let monitor_top = i64::from(monitor.y);
        let monitor_right = monitor_left + i64::from(monitor.width);
        let monitor_bottom = monitor_top + i64::from(monitor.height);
        let intersection_left = left.max(monitor_left);
        let intersection_top = top.max(monitor_top);
        let intersection_right = right.min(monitor_right);
        let intersection_bottom = bottom.min(monitor_bottom);
        if intersection_right <= intersection_left || intersection_bottom <= intersection_top {
            continue;
        }
        tiles.push(CaptureTile {
            monitor_index,
            source_x: u32::try_from(intersection_left - monitor_left)
                .map_err(|_| UiError::InvalidState("截图坐标超出显示器范围".to_owned()))?,
            source_y: u32::try_from(intersection_top - monitor_top)
                .map_err(|_| UiError::InvalidState("截图坐标超出显示器范围".to_owned()))?,
            destination_x: u32::try_from(intersection_left - left)
                .map_err(|_| UiError::InvalidState("截图坐标超出输出范围".to_owned()))?,
            destination_y: u32::try_from(intersection_top - top)
                .map_err(|_| UiError::InvalidState("截图坐标超出输出范围".to_owned()))?,
            width: u32::try_from(intersection_right - intersection_left)
                .map_err(|_| UiError::InvalidState("截图区域过宽".to_owned()))?,
            height: u32::try_from(intersection_bottom - intersection_top)
                .map_err(|_| UiError::InvalidState("截图区域过高".to_owned()))?,
        });
    }
    if tiles.is_empty() {
        return Err(UiError::InvalidState(
            "截图区域不与任何显示器相交".to_owned(),
        ));
    }
    Ok(CapturePlan {
        width,
        height,
        tiles,
    })
}

fn floor_i64(value: f64) -> Result<i64, UiError> {
    let value = value.floor();
    if value < i64::MIN as f64 || value > i64::MAX as f64 {
        Err(UiError::InvalidState("截图坐标超出范围".to_owned()))
    } else {
        Ok(value as i64)
    }
}

fn ceil_i64(value: f64) -> Result<i64, UiError> {
    let value = value.ceil();
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        Err(UiError::InvalidState("截图坐标超出范围".to_owned()))
    } else {
        Ok(value as i64)
    }
}

#[derive(Debug, Clone)]
pub struct WindowsScreenshot {
    captures_dir: PathBuf,
}

impl WindowsScreenshot {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            captures_dir: crate::data_dir()?.join("captures"),
        })
    }
}

impl Screenshot for WindowsScreenshot {
    fn capture(&self, region: CaptureRegion) -> Result<CapturedImage, UiError> {
        let monitors = xcap::Monitor::all().map_err(platform)?;
        let plan = plan_capture(region, &geometries(&monitors)?)?;
        let mut output = image::RgbaImage::new(plan.width, plan.height);
        for tile in plan.tiles {
            let captured = monitors[tile.monitor_index]
                .capture_region(tile.source_x, tile.source_y, tile.width, tile.height)
                .map_err(platform)?;
            if captured.width() != tile.width || captured.height() != tile.height {
                return Err(UiError::Platform(
                    "xcap returned unexpected capture dimensions".to_owned(),
                ));
            }
            image::imageops::replace(
                &mut output,
                &captured,
                i64::from(tile.destination_x),
                i64::from(tile.destination_y),
            );
        }
        fs::create_dir_all(&self.captures_dir).map_err(UiError::Io)?;
        let path = self
            .captures_dir
            .join(format!("screenshot-{}.png", uuid::Uuid::new_v4()));
        crate::save_rgba_png(&path, plan.width, plan.height, output.as_raw())
            .map_err(UiError::Io)?;
        Ok(CapturedImage {
            path,
            mime: "image/png".to_owned(),
            pixel_width: plan.width,
            pixel_height: plan.height,
        })
    }
}
