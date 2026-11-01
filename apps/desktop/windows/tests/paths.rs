use aimemorandum_windows::data_dir_from;
use std::{ffi::OsString, path::Path};

#[test]
fn data_dir_is_rooted_at_roaming_appdata() {
    let root = OsString::from(r"C:\Users\Test\AppData\Roaming");
    assert_eq!(
        data_dir_from(Some(root)).unwrap(),
        Path::new(r"C:\Users\Test\AppData\Roaming\AIMemorandum")
    );
    assert!(data_dir_from(None).is_err());
}
