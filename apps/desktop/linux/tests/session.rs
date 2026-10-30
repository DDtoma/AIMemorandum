use aimemorandum_linux::{detect_session_from, SessionError, SessionKind};
use std::ffi::OsString;

fn value(value: &str) -> Option<OsString> {
    Some(OsString::from(value))
}

#[test]
fn session_selection_honours_wayland_then_x11() {
    assert_eq!(
        detect_session_from(value("wayland"), None, value(":0")).unwrap(),
        SessionKind::Wayland
    );
    assert_eq!(
        detect_session_from(None, value("wayland-0"), value(":0")).unwrap(),
        SessionKind::Wayland
    );
    assert_eq!(
        detect_session_from(value("x11"), None, value(":0")).unwrap(),
        SessionKind::X11
    );
}

#[test]
fn headless_session_has_an_explicit_error_for_cli_fallback() {
    let error = detect_session_from(None, None, None).unwrap_err();
    assert!(matches!(error, SessionError::Headless));
    assert!(error.to_string().contains("CLI"));
}
