use aimemorandum_linux::{autostart_path_from, data_dir_from, socket_path_from};
use std::{ffi::OsString, path::Path};

fn os(value: &str) -> Option<OsString> {
    Some(OsString::from(value))
}

#[test]
fn xdg_data_home_takes_precedence() {
    assert_eq!(
        data_dir_from(os("/xdg/data"), os("/home/test")).unwrap(),
        Path::new("/xdg/data/aimemorandum")
    );
}

#[test]
fn data_dir_uses_the_xdg_fallback_and_ignores_relative_paths() {
    assert_eq!(
        data_dir_from(None, os("/home/test")).unwrap(),
        Path::new("/home/test/.local/share/aimemorandum")
    );
    assert_eq!(
        data_dir_from(os("relative/data"), os("/home/test")).unwrap(),
        Path::new("/home/test/.local/share/aimemorandum")
    );
    assert!(data_dir_from(None, None).is_err());
}

#[test]
fn autostart_and_socket_paths_follow_their_xdg_roots() {
    assert_eq!(
        autostart_path_from(None, os("/home/test")).unwrap(),
        Path::new("/home/test/.config/autostart/aimemorandum.desktop")
    );
    assert_eq!(
        socket_path_from(os("/run/user/1000"), None).unwrap(),
        Path::new("/run/user/1000/aimemorandum.sock")
    );
    assert!(socket_path_from(None, None).is_err());
}
