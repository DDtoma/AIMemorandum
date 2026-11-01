use aimemorandum_linux::{GeometryError, LogicalRegion, MonitorGeometry};

#[test]
fn capture_metadata_requires_real_output_position_and_logical_size() {
    assert_eq!(
        MonitorGeometry::from_capture_metadata(None, Some((1920, 1080)), 3840, 2160),
        Err(GeometryError::MissingOutputPosition)
    );
    assert_eq!(
        MonitorGeometry::from_capture_metadata(Some((0, 0)), None, 3840, 2160),
        Err(GeometryError::MissingLogicalSize)
    );
}

#[test]
fn capture_metadata_preserves_negative_origin_and_fractional_scale() {
    assert_eq!(
        MonitorGeometry::from_capture_metadata(Some((-1707, 100)), Some((1707, 960)), 2560, 1440),
        Ok(MonitorGeometry {
            x: -1707,
            y: 100,
            width: 1707,
            height: 960,
            scale_factor: 2560.0 / 1707.0,
        })
    );
}

#[test]
fn converts_negative_monitor_origins_and_fractional_scale_to_pixels() {
    let monitor = MonitorGeometry {
        x: -1920,
        y: 0,
        width: 1920,
        height: 1080,
        scale_factor: 1.5,
    };
    let crop = monitor
        .physical_crop(LogicalRegion {
            x: -1800.0,
            y: 100.0,
            width: 300.0,
            height: 200.0,
        })
        .unwrap();
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (180, 150, 450, 300)
    );
}

#[test]
fn rounds_opposite_edges_so_rounding_does_not_shrink_the_selection() {
    let monitor = MonitorGeometry {
        x: 0,
        y: 0,
        width: 1000,
        height: 1000,
        scale_factor: 1.5,
    };
    let crop = monitor
        .physical_crop(LogicalRegion {
            x: 10.2,
            y: 0.0,
            width: 20.4,
            height: 10.0,
        })
        .unwrap();
    assert_eq!((crop.x, crop.width), (15, 31));
}

#[test]
fn rejects_invalid_or_out_of_monitor_regions() {
    let monitor = MonitorGeometry {
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        scale_factor: 1.0,
    };
    for region in [
        LogicalRegion {
            x: f64::NAN,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        LogicalRegion {
            x: 0.0,
            y: 0.0,
            width: -1.0,
            height: 10.0,
        },
        LogicalRegion {
            x: 95.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
    ] {
        assert!(monitor.physical_crop(region).is_err());
    }
    assert!(MonitorGeometry {
        scale_factor: 0.0,
        ..monitor
    }
    .physical_crop(LogicalRegion {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0
    })
    .is_err());
}
