use aimemorandum_ui::{
    output_geometry_for_capture, select_overlay_output, DisplayGeometry, OutputMatchError,
};

fn display(x: i32, y: i32, width: u32, height: u32) -> DisplayGeometry {
    DisplayGeometry {
        x,
        y,
        width,
        height,
        scale_factor: 1.0,
    }
}

fn with_scale(scale_factor: f64) -> DisplayGeometry {
    DisplayGeometry {
        scale_factor,
        ..display(0, 0, 1920, 1080)
    }
}

#[test]
fn selects_the_unique_output_at_a_negative_origin_with_fractional_scaling() {
    let captured = DisplayGeometry {
        scale_factor: 1.5,
        ..display(-1707, 100, 1707, 960)
    };
    // Winit may expose an integer output scale of 1 here. The capture's own
    // physical/logical ratio is the coordinate space shared with cropping.
    let candidates = [
        output_geometry_for_capture(0, 0, 1920, 1080, with_scale(1.0)).unwrap(),
        output_geometry_for_capture(-2560, 150, 2560, 1440, captured).unwrap(),
    ];

    assert_eq!(select_overlay_output(captured, &candidates), Ok(1));
}

#[test]
fn rejects_missing_and_ambiguous_output_matches() {
    let target = display(-100, 50, 1280, 720);
    assert_eq!(
        select_overlay_output(target, &[display(0, 0, 1920, 1080)]),
        Err(OutputMatchError::NoMatch)
    );
    assert_eq!(
        select_overlay_output(target, &[target, target]),
        Err(OutputMatchError::Ambiguous {
            matches: vec![0, 1]
        })
    );
}

#[test]
fn rejects_non_finite_or_non_positive_output_scale() {
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            output_geometry_for_capture(0, 0, 1920, 1080, with_scale(scale)),
            None
        );
    }
}

#[test]
fn distinguishes_same_sized_outputs_by_their_origins() {
    let candidates = [display(0, 0, 1920, 1080), display(1920, -200, 1920, 1080)];

    assert_eq!(
        select_overlay_output(display(1920, -200, 1920, 1080), &candidates),
        Ok(1)
    );
}

#[test]
fn accepts_one_logical_pixel_of_rounding_but_not_two() {
    let candidates = [display(-100, 50, 1280, 720)];

    assert_eq!(
        select_overlay_output(display(-99, 49, 1281, 719), &candidates),
        Ok(0)
    );
    assert_eq!(
        select_overlay_output(display(-98, 50, 1280, 720), &candidates),
        Err(OutputMatchError::NoMatch)
    );
}

#[test]
fn rejects_zero_sized_and_overflowing_output_geometry() {
    assert_eq!(
        output_geometry_for_capture(0, 0, 0, 1080, with_scale(1.0)),
        None
    );
    assert_eq!(
        output_geometry_for_capture(0, 0, 1920, 0, with_scale(1.0)),
        None
    );
    assert_eq!(
        output_geometry_for_capture(i32::MIN, 0, 1920, 1080, with_scale(f64::MIN_POSITIVE)),
        None
    );
    assert_eq!(
        output_geometry_for_capture(0, 0, u32::MAX, 1080, with_scale(f64::MIN_POSITIVE)),
        None
    );
}
