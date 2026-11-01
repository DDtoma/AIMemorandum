#![cfg(windows)]

use aimemorandum_windows::save_rgba_png;

#[test]
fn rgba_clipboard_pixels_are_saved_as_a_real_png() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("clipboard.png");
    let pixels = [255, 0, 0, 255, 0, 255, 0, 128];
    save_rgba_png(&path, 2, 1, &pixels).unwrap();

    let image = image::open(&path).unwrap();
    assert_eq!(image.width(), 2);
    assert_eq!(image.height(), 1);
    assert!(save_rgba_png(&path, 3, 1, &pixels).is_err());
}
