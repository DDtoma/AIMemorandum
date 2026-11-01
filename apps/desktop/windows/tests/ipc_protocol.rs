#![cfg(windows)]

use aimemorandum_windows::{decode_command, encode_command, CliAction};

#[test]
fn command_frames_round_trip_with_a_version() {
    for action in [
        CliAction::Launch,
        CliAction::TogglePanel,
        CliAction::Screenshot,
        CliAction::SaveClipboard,
        CliAction::ShowItems,
        CliAction::Background,
    ] {
        let frame = encode_command(action).unwrap();
        assert!(frame.ends_with(b"\n"));
        assert_eq!(decode_command(&frame).unwrap(), action);
    }
}

#[test]
fn malformed_and_wrong_version_frames_are_rejected() {
    assert!(decode_command(b"not json\n").is_err());
    assert!(decode_command(b"{\"version\":2,\"action\":\"launch\"}\n").is_err());
    assert!(decode_command(b"{\"version\":1,\"action\":\"launch\"}").is_err());
}
