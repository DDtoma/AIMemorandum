#![cfg(windows)]

use aimemorandum_windows::{autostart_command, autostart_registry_value};
use std::path::Path;

const EXECUTABLE: &str = r"C:\Program Files\AIMemorandum\aimemorandum.exe";

#[test]
fn autostart_command_quotes_the_executable_and_starts_in_background() {
    assert_eq!(
        autostart_command(Path::new(EXECUTABLE)),
        r#""C:\Program Files\AIMemorandum\aimemorandum.exe" --background"#
    );
}

#[test]
fn autostart_registry_value_includes_the_reg_sz_nul_terminator() {
    let value = autostart_registry_value(Path::new(EXECUTABLE));
    assert_eq!(value.last(), Some(&0));
    assert_eq!(
        String::from_utf16(&value[..value.len() - 1]).unwrap(),
        autostart_command(Path::new(EXECUTABLE))
    );
}

#[test]
fn background_autostart_executable_uses_the_windows_gui_subsystem() {
    let bytes = std::fs::read(env!("CARGO_BIN_EXE_aimemorandum-windows")).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0");
    let optional_header = pe + 24;
    let magic = u16::from_le_bytes(
        bytes[optional_header..optional_header + 2]
            .try_into()
            .unwrap(),
    );
    assert!(matches!(magic, 0x10b | 0x20b));
    let subsystem = u16::from_le_bytes(
        bytes[optional_header + 68..optional_header + 70]
            .try_into()
            .unwrap(),
    );
    assert_eq!(subsystem, 2, "expected IMAGE_SUBSYSTEM_WINDOWS_GUI");
}
