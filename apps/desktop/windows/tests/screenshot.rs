#![cfg(windows)]

use aimemorandum_ui::{CaptureRegion, Screenshot};
use aimemorandum_windows::{
    monitor_geometries, normalized_selection, plan_capture, project_selection,
    virtual_desktop_bounds, MonitorGeometry, PhysicalPoint, PhysicalRect, SelectionDrag,
    WindowsScreenshot,
};

#[test]
#[ignore = "captures the real Windows virtual desktop"]
fn xcap_captures_the_real_virtual_desktop_to_png() {
    let monitors = monitor_geometries().expect("enumerate real monitors");
    println!("real monitor geometries: {monitors:?}");
    let bounds = virtual_desktop_bounds(&monitors).expect("real virtual desktop bounds");
    let captured = WindowsScreenshot::new()
        .expect("create native screenshot adapter")
        .capture(CaptureRegion {
            x: bounds.x as f64,
            y: bounds.y as f64,
            width: bounds.width as f64,
            height: bounds.height as f64,
        })
        .expect("capture real virtual desktop");
    println!("real capture: {}", captured.path.display());
    assert_eq!(captured.mime, "image/png");
    assert_eq!(
        (captured.pixel_width, captured.pixel_height),
        (bounds.width, bounds.height)
    );
    let decoded = image::open(&captured.path).expect("decode native PNG capture");
    assert_eq!(
        (decoded.width(), decoded.height()),
        (bounds.width, bounds.height)
    );
}

#[test]
fn physical_selection_normalizes_reversed_drags_and_consumes_finish_or_cancel() {
    let start = PhysicalPoint { x: 100, y: 200 };
    let end = PhysicalPoint { x: -100, y: 100 };
    assert_eq!(
        normalized_selection(start, end),
        PhysicalRect {
            left: -100,
            top: 100,
            right: 100,
            bottom: 200,
        }
    );

    let mut drag = SelectionDrag::default();
    drag.begin(start);
    assert!(drag.update(end).is_some());
    assert!(drag.finish(end).is_some());
    assert!(drag.finish(end).is_none());
    assert!(!drag.cancel());
    drag.begin(start);
    assert!(drag.cancel());
    assert!(drag.finish(end).is_none());
}

#[test]
fn selection_projection_preserves_physical_pixels_across_negative_origins_and_dpi_changes() {
    let selection = PhysicalRect {
        left: -2500,
        top: -1000,
        right: -2460,
        bottom: -960,
    };
    let origin = PhysicalPoint { x: -2560, y: -1080 };
    let projected = project_selection(selection, origin, 1.25).unwrap();
    assert_eq!(projected.x, 48.0);
    assert_eq!(projected.y, 64.0);
    assert_eq!(projected.width, 32.0);
    assert_eq!(projected.height, 32.0);

    for scale in [1.0, 1.25, 1.5] {
        let projected = project_selection(selection, origin, scale).unwrap();
        assert_eq!(projected.x * scale + origin.x as f64, selection.left as f64);
        assert_eq!(projected.y * scale + origin.y as f64, selection.top as f64);
        assert_eq!(projected.width * scale, 40.0);
        assert_eq!(projected.height * scale, 40.0);
    }
    assert!(project_selection(selection, origin, 0.0).is_none());
    assert!(project_selection(selection, origin, f64::NAN).is_none());
    assert!(project_selection(selection, origin, f64::INFINITY).is_none());
}

#[test]
fn splits_a_global_selection_across_negative_and_primary_monitor_origins() {
    let monitors = vec![
        MonitorGeometry {
            x: -1280,
            y: 0,
            width: 1280,
            height: 1024,
        },
        MonitorGeometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
    ];
    let plan = plan_capture(
        CaptureRegion {
            x: -100.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
        },
        &monitors,
    )
    .unwrap();

    assert_eq!((plan.width, plan.height), (200, 100));
    assert_eq!(plan.tiles.len(), 2);
    assert_eq!(
        (
            plan.tiles[0].monitor_index,
            plan.tiles[0].source_x,
            plan.tiles[0].source_y
        ),
        (0, 1180, 50)
    );
    assert_eq!(
        (
            plan.tiles[0].destination_x,
            plan.tiles[0].width,
            plan.tiles[0].height
        ),
        (0, 100, 100)
    );
    assert_eq!(
        (
            plan.tiles[1].monitor_index,
            plan.tiles[1].source_x,
            plan.tiles[1].source_y
        ),
        (1, 0, 50)
    );
    assert_eq!(
        (
            plan.tiles[1].destination_x,
            plan.tiles[1].width,
            plan.tiles[1].height
        ),
        (100, 100, 100)
    );
}

#[test]
fn computes_virtual_desktop_bounds_with_negative_and_vertical_origins() {
    let monitors = vec![
        MonitorGeometry {
            x: -1280,
            y: -200,
            width: 1280,
            height: 1024,
        },
        MonitorGeometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
    ];
    assert_eq!(
        virtual_desktop_bounds(&monitors).unwrap(),
        MonitorGeometry {
            x: -1280,
            y: -200,
            width: 3200,
            height: 1280
        }
    );
    assert!(virtual_desktop_bounds(&[]).is_err());
}

#[test]
fn expands_fractional_selection_to_whole_pixels() {
    let monitors = vec![MonitorGeometry {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    }];
    let plan = plan_capture(
        CaptureRegion {
            x: 10.25,
            y: 20.75,
            width: 30.5,
            height: 40.25,
        },
        &monitors,
    )
    .unwrap();
    assert_eq!((plan.width, plan.height), (31, 41));
    assert_eq!((plan.tiles[0].source_x, plan.tiles[0].source_y), (10, 20));
}

#[test]
fn rejects_empty_or_uncovered_regions() {
    let monitors = vec![MonitorGeometry {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    }];
    assert!(plan_capture(
        CaptureRegion {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 20.0
        },
        &monitors,
    )
    .is_err());
    assert!(plan_capture(
        CaptureRegion {
            x: 900.0,
            y: 0.0,
            width: 20.0,
            height: 20.0
        },
        &monitors,
    )
    .is_err());
}
