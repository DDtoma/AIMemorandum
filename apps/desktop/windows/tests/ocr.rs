use aimemorandum_core::LocalStore;
use aimemorandum_ui::{
    CaptureRegion, CapturedImage, Clipboard, Clock, CoreEnrollment, DesktopShell, Screenshot,
    UiController, UiError, UiIntent,
};
use aimemorandum_windows::{fit_within_max_dimension, WindowsOcr};
use chrono::{DateTime, Utc};
use std::path::Path;

#[test]
#[ignore = "requires a real Windows OCR language pack and AIMEMORANDUM_OCR_FIXTURE"]
fn windows_ocr_recognizes_text_from_a_real_image() {
    let path = std::env::var_os("AIMEMORANDUM_OCR_FIXTURE")
        .expect("set AIMEMORANDUM_OCR_FIXTURE to a PNG containing AIMemorandum OCR 123");
    let text = WindowsOcr::user_profile()
        .recognize_file(std::path::Path::new(&path))
        .expect("recognize real image with WinRT OCR");
    println!("WinRT OCR output: {text}");
    assert!(
        text.contains("AIMemorandum"),
        "unexpected OCR output: {text}"
    );
    assert!(text.contains("123"), "unexpected OCR output: {text}");
}

struct NoScreenshot;
impl Screenshot for NoScreenshot {
    fn capture(&self, _region: CaptureRegion) -> Result<CapturedImage, UiError> {
        unreachable!()
    }
}

struct NoClipboard;
impl Clipboard for NoClipboard {
    fn read_text(&self) -> Result<Option<String>, UiError> {
        Ok(None)
    }

    fn read_image(&self) -> Result<Option<CapturedImage>, UiError> {
        Ok(None)
    }
}

struct NoShell;
impl DesktopShell for NoShell {
    fn set_autostart(&self, _enabled: bool) -> Result<(), UiError> {
        Ok(())
    }

    fn open_attachment(&self, _path: &Path) -> Result<(), UiError> {
        Ok(())
    }
}

struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn timezone(&self) -> String {
        "UTC".to_owned()
    }
}

#[test]
#[ignore = "requires a real Windows OCR language pack and AIMEMORANDUM_OCR_FIXTURE"]
fn windows_ocr_populates_a_dropped_item_through_the_product_controller() {
    let path = std::env::var_os("AIMEMORANDUM_OCR_FIXTURE")
        .expect("set AIMEMORANDUM_OCR_FIXTURE to a PNG containing AIMemorandum OCR 123");
    let original = std::fs::read(&path).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut controller = UiController::new(
        LocalStore::open(directory.path()).unwrap(),
        NoScreenshot,
        NoClipboard,
        CoreEnrollment,
        NoShell,
        TestClock,
    )
    .unwrap()
    .with_text_recognizer(WindowsOcr::user_profile());

    controller
        .dispatch(UiIntent::DropImages(vec![Path::new(&path).to_path_buf()]))
        .unwrap();
    let store = LocalStore::open(directory.path()).unwrap();
    let items = store.items().unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].body_md.contains("AIMemorandum OCR 123"));
    assert_eq!(items[0].attachments.len(), 1);
    assert!(store.attachment_path(&items[0].attachments[0]).is_file());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn ocr_scaling_preserves_aspect_ratio_and_never_upscales() {
    assert_eq!(fit_within_max_dimension(4000, 2000, 2600), (2600, 1300));
    assert_eq!(fit_within_max_dimension(1000, 3000, 2600), (866, 2600));
    assert_eq!(fit_within_max_dimension(800, 600, 2600), (800, 600));
}
