use aimemorandum_windows::autostart_command;
use std::path::Path;

#[test]
fn autostart_command_quotes_the_executable_and_starts_in_background() {
    assert_eq!(
        autostart_command(Path::new(r"C:\Program Files\AIMemorandum\aimemorandum.exe")),
        r#""C:\Program Files\AIMemorandum\aimemorandum.exe" --background"#
    );
}
