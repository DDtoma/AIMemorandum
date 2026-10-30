use aimemorandum_linux::{LogicalRegion, MonitorGeometry, StaticFrame};
use image::{Rgba, RgbaImage};

#[test]
fn crops_the_cached_frame_with_fractional_monitor_scaling() {
    let mut pixels = RgbaImage::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            pixels.put_pixel(x, y, Rgba([x as u8, y as u8, 0, 255]));
        }
    }
    let frame = StaticFrame::new(
        pixels,
        MonitorGeometry {
            x: -100,
            y: -50,
            width: 4,
            height: 4,
            scale_factor: 2.0,
        },
    );

    let crop = frame
        .crop(LogicalRegion {
            x: -99.0,
            y: -49.5,
            width: 2.0,
            height: 1.5,
        })
        .unwrap();

    assert_eq!(crop.dimensions(), (4, 3));
    assert_eq!(crop.get_pixel(0, 0), &Rgba([2, 1, 0, 255]));
}

#[test]
fn rejects_a_crop_beyond_the_actual_pixel_buffer() {
    let frame = StaticFrame::new(
        RgbaImage::new(4, 4),
        MonitorGeometry {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            scale_factor: 2.0,
        },
    );
    assert!(frame
        .crop(LogicalRegion {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        })
        .is_err());
}
