use aimemorandum_ui::{
    output_configuration_ready, output_geometry_for_capture, select_overlay_output,
    DisplayGeometry, OutputMatchError, OutputRoute, ScreenshotOutputRouter,
};
use slint::winit_030::{winit::window::Fullscreen, WinitWindowAccessor};

pub struct WaylandOutputRouter;

impl ScreenshotOutputRouter for WaylandOutputRouter {
    fn route(
        &self,
        window: &slint::Window,
        captured: DisplayGeometry,
    ) -> Result<OutputRoute, String> {
        let Some(route) = window.with_winit_window(|native| {
            let monitors = native.available_monitors().collect::<Vec<_>>();
            let candidates = monitors
                .iter()
                .map(|monitor| {
                    let position = monitor.position();
                    let size = monitor.size();
                    output_geometry_for_capture(
                        position.x,
                        position.y,
                        size.width,
                        size.height,
                        monitor.scale_factor(),
                        captured,
                    )
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| "无法换算一个或多个显示器的逻辑几何信息".to_owned())?;
            let selected =
                select_overlay_output(captured, &candidates).map_err(|error| match error {
                    OutputMatchError::NoMatch => "没有与截图几何信息匹配的显示器".to_owned(),
                    OutputMatchError::Ambiguous { .. } => {
                        "多个显示器与截图几何信息匹配，无法安全选择".to_owned()
                    }
                })?;
            let target = &monitors[selected];
            let fullscreen_output = match native.fullscreen() {
                Some(Fullscreen::Borderless(Some(monitor))) => Some(monitor.native_identifier()),
                _ => None,
            };
            let window_size = native.inner_size();
            let output_size = target.size();
            if output_configuration_ready(
                native
                    .current_monitor()
                    .map(|monitor| monitor.native_identifier()),
                fullscreen_output,
                target.native_identifier(),
                (window_size.width, window_size.height),
                (output_size.width, output_size.height),
            ) {
                return Ok(OutputRoute::Routed);
            }
            native.set_fullscreen(Some(Fullscreen::Borderless(Some(target.clone()))));
            Ok(OutputRoute::Pending)
        }) else {
            return Ok(OutputRoute::Pending);
        };
        route
    }
}
