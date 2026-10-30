use aimemorandum_ui::UiError;

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub(crate) fn platform(error: impl std::fmt::Display) -> UiError {
    UiError::Platform(error.to_string())
}
