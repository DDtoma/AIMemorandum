use crate::DisplayGeometry;
use std::fmt;

const LOGICAL_TOLERANCE: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputRoute {
    Pending,
    Routed,
}

/// Platform hook that places an already-created overlay on its captured output.
pub trait ScreenshotOutputRouter {
    fn route(
        &self,
        window: &slint::Window,
        captured: DisplayGeometry,
    ) -> Result<OutputRoute, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenshotWindowAction {
    Show,
    Hide,
    None,
}

#[derive(Debug, Default)]
pub struct ScreenshotWindowState {
    shown: bool,
    display: Option<DisplayGeometry>,
}

impl ScreenshotWindowState {
    /// Returns Show for a new capture or a changed output, including coalesced recaptures.
    pub fn synchronize(
        &mut self,
        active: bool,
        display: Option<DisplayGeometry>,
    ) -> ScreenshotWindowAction {
        if !active {
            return if self.shown {
                self.shown = false;
                self.display = None;
                ScreenshotWindowAction::Hide
            } else {
                ScreenshotWindowAction::None
            };
        }
        if self.shown && self.display == display {
            return ScreenshotWindowAction::None;
        }
        self.shown = true;
        self.display = display;
        ScreenshotWindowAction::Show
    }
}

/// Requests are asynchronous on Wayland; only actual output and size confirm readiness.
pub fn output_configuration_ready(
    current_output: Option<u32>,
    fullscreen_output: Option<u32>,
    target_output: u32,
    window_size: (u32, u32),
    output_size: (u32, u32),
) -> bool {
    current_output == Some(target_output)
        && fullscreen_output == Some(target_output)
        && window_size == output_size
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputMatchError {
    NoMatch,
    Ambiguous { matches: Vec<usize> },
}

impl fmt::Display for OutputMatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMatch => formatter.write_str("no output matches the captured display geometry"),
            Self::Ambiguous { matches } => {
                write!(
                    formatter,
                    "multiple outputs match the captured display: {matches:?}"
                )
            }
        }
    }
}

impl std::error::Error for OutputMatchError {}

/// Converts native output geometry into the compositor's logical coordinates.
///
/// The origin uses winit's native output scale; dimensions use ScreenCast's
/// physical/logical ratio. Neither scale alone describes both native fields.
pub fn output_geometry_for_capture(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    native_scale_factor: f64,
    captured: DisplayGeometry,
) -> Option<DisplayGeometry> {
    let scale_factor = captured.scale_factor;
    if !native_scale_factor.is_finite()
        || native_scale_factor <= 0.0
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
    {
        return None;
    }
    // Winit's Wayland position is logical_position * integer output scale,
    // whereas its size is the physical mode. Keep those conversion domains
    // separate or a fractional-scale output at a nonzero origin will not match.
    let logical_x = (f64::from(x) / native_scale_factor).round();
    let logical_y = (f64::from(y) / native_scale_factor).round();
    let logical_width = (f64::from(width) / scale_factor).round();
    let logical_height = (f64::from(height) / scale_factor).round();
    if logical_x < f64::from(i32::MIN)
        || logical_x > f64::from(i32::MAX)
        || logical_y < f64::from(i32::MIN)
        || logical_y > f64::from(i32::MAX)
        || logical_width < 1.0
        || logical_width > f64::from(u32::MAX)
        || logical_height < 1.0
        || logical_height > f64::from(u32::MAX)
    {
        return None;
    }
    Some(DisplayGeometry {
        x: logical_x as i32,
        y: logical_y as i32,
        width: logical_width as u32,
        height: logical_height as u32,
        scale_factor,
    })
}

/// Selects only a unique geometry match; guessing could crop a different output's pixels.
pub fn select_overlay_output(
    captured: DisplayGeometry,
    candidates: &[DisplayGeometry],
) -> Result<usize, OutputMatchError> {
    let matches = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| geometry_matches(captured, *candidate).then_some(index))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(OutputMatchError::NoMatch),
        _ => Err(OutputMatchError::Ambiguous { matches }),
    }
}

fn geometry_matches(captured: DisplayGeometry, candidate: DisplayGeometry) -> bool {
    within_tolerance(i64::from(captured.x), i64::from(candidate.x))
        && within_tolerance(i64::from(captured.y), i64::from(candidate.y))
        && within_tolerance(i64::from(captured.width), i64::from(candidate.width))
        && within_tolerance(i64::from(captured.height), i64::from(candidate.height))
}

fn within_tolerance(left: i64, right: i64) -> bool {
    (left - right).abs() <= LOGICAL_TOLERANCE
}
