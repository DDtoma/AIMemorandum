#![cfg(windows)]

use aimemorandum_windows::{parse_args, CliAction};

#[test]
fn no_args_launches_the_resident_app() {
    assert_eq!(parse_args(Vec::<String>::new()).unwrap(), CliAction::Launch);
}

#[test]
fn parses_each_supported_action() {
    for (flag, action) in [
        ("--toggle-panel", CliAction::TogglePanel),
        ("--screenshot", CliAction::Screenshot),
        ("--save-clipboard", CliAction::SaveClipboard),
        ("--items", CliAction::ShowItems),
        ("--background", CliAction::Background),
    ] {
        assert_eq!(parse_args([flag]).unwrap(), action);
    }
}

#[test]
fn rejects_unknown_or_multiple_actions() {
    assert!(parse_args(["--unknown"]).is_err());
    assert!(parse_args(["--items", "--screenshot"]).is_err());
}
