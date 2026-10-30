use aimemorandum_linux::set_autostart_at;
use std::path::Path;

#[test]
fn autostart_writes_a_background_desktop_entry_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("autostart/aimemorandum.desktop");
    let executable = Path::new("/opt/AI Memorandum/aimemorandum");
    set_autostart_at(&path, executable, true).unwrap();
    let first = std::fs::read_to_string(&path).unwrap();
    assert!(first.contains("Type=Application\n"));
    assert!(first.contains("Exec=\"/opt/AI Memorandum/aimemorandum\" --background\n"));
    assert!(first.contains("X-GNOME-Autostart-enabled=true\n"));
    assert!(!first.contains("enrollment"));
    set_autostart_at(&path, executable, true).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
}

#[test]
fn disabling_autostart_removes_only_the_desktop_entry() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("autostart/aimemorandum.desktop");
    let executable = Path::new("/usr/bin/aimemorandum");
    set_autostart_at(&path, executable, true).unwrap();
    set_autostart_at(&path, executable, false).unwrap();
    assert!(!path.exists());
    set_autostart_at(&path, executable, false).unwrap();
    assert!(temp.path().join("autostart").exists());
}
