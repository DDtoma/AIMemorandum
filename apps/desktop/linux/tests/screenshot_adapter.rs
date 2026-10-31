use aimemorandum_linux::{FrameProvider, MonitorGeometry, ScreenshotAdapter, StaticFrame};
use aimemorandum_ui::{
    output_geometry_for_capture, select_overlay_output, CaptureRegion, DisplayGeometry, UiError,
};
use image::RgbaImage;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct FakeProvider {
    calls: Arc<AtomicUsize>,
    frame: Option<StaticFrame>,
}

impl FrameProvider for FakeProvider {
    fn capture_frame(&mut self) -> Result<StaticFrame, UiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.frame
            .take()
            .ok_or_else(|| UiError::Platform("unexpected second capture".to_owned()))
    }
}

#[test]
fn selection_crops_the_one_cached_frame_and_stages_a_png() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = FakeProvider {
        calls: calls.clone(),
        frame: Some(StaticFrame::new(
            RgbaImage::new(8, 8),
            MonitorGeometry {
                x: -100,
                y: -50,
                width: 4,
                height: 4,
                scale_factor: 2.0,
            },
        )),
    };
    let adapter = ScreenshotAdapter::new(Box::new(provider), directory.path().join("captures"));

    let full = adapter.capture_full().unwrap();
    assert!(full.path.is_file());
    let captured_display = DisplayGeometry {
        x: -100,
        y: -50,
        width: 4,
        height: 4,
        scale_factor: 2.0,
    };
    assert_eq!(adapter.capture_display(), Some(captured_display));
    let outputs = [
        output_geometry_for_capture(0, 0, 1920, 1080, captured_display).unwrap(),
        output_geometry_for_capture(-200, -100, 8, 8, captured_display).unwrap(),
    ];
    assert_eq!(select_overlay_output(captured_display, &outputs), Ok(1));
    let selected = adapter
        .capture(CaptureRegion {
            x: 1.0,
            y: 1.0,
            width: 2.0,
            height: 1.5,
        })
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!((selected.pixel_width, selected.pixel_height), (4, 3));
    assert_eq!(selected.mime, "image/png");
    assert_eq!(
        image::open(&selected.path).unwrap().to_rgba8().dimensions(),
        (4, 3)
    );
    assert!(!full.path.exists());
    adapter.release_capture(&selected);
    assert!(!selected.path.exists());
    assert!(adapter
        .capture(CaptureRegion {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        })
        .is_err());
}

#[test]
fn dropping_adapter_removes_an_unused_full_frame() {
    let directory = tempfile::tempdir().unwrap();
    let capture_dir = directory.path().join("captures");
    let full_path;
    {
        let adapter = ScreenshotAdapter::new(
            Box::new(FakeProvider {
                calls: Arc::new(AtomicUsize::new(0)),
                frame: Some(StaticFrame::new(
                    RgbaImage::new(8, 8),
                    MonitorGeometry {
                        x: 0,
                        y: 0,
                        width: 4,
                        height: 4,
                        scale_factor: 2.0,
                    },
                )),
            }),
            capture_dir,
        );
        full_path = adapter.capture_full().unwrap().path;
        assert!(full_path.exists());
    }

    assert!(!full_path.exists());
}

#[test]
fn cancellation_removes_the_cached_full_frame() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = FakeProvider {
        calls: calls.clone(),
        frame: Some(StaticFrame::new(
            RgbaImage::new(8, 8),
            MonitorGeometry {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
                scale_factor: 2.0,
            },
        )),
    };
    let adapter = ScreenshotAdapter::new(Box::new(provider), directory.path().join("captures"));
    let full = adapter.capture_full().unwrap();

    adapter.cancel_capture();

    assert!(!full.path.exists());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(adapter
        .capture(CaptureRegion {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        })
        .is_err());
}
