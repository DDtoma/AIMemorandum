use std::ffi::OsString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    X11,
    Wayland,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("未检测到 X11/Wayland 图形会话；请通过 CLI 与已运行实例交互")]
    Headless,
}

pub fn detect_session() -> Result<SessionKind, SessionError> {
    detect_session_from(
        std::env::var_os("XDG_SESSION_TYPE"),
        std::env::var_os("WAYLAND_DISPLAY"),
        std::env::var_os("DISPLAY"),
    )
}

pub fn detect_session_from(
    session_type: Option<OsString>,
    wayland_display: Option<OsString>,
    display: Option<OsString>,
) -> Result<SessionKind, SessionError> {
    match session_type
        .as_deref()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wayland") => return Ok(SessionKind::Wayland),
        Some("x11") => return Ok(SessionKind::X11),
        _ => {}
    }
    if wayland_display.is_some() {
        return Ok(SessionKind::Wayland);
    }
    if display.is_some() {
        return Ok(SessionKind::X11);
    }
    Err(SessionError::Headless)
}
