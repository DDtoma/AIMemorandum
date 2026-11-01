#![cfg(windows)]

use aimemorandum_ui::TrayEntry;
use aimemorandum_windows::{menu_action, CliAction, NativeTray, TrayAction};

#[test]
#[ignore = "creates a real Windows notification-area icon"]
fn native_tray_accepts_twelve_unicode_entries() {
    let tray = NativeTray::new().expect("create real native tray icon");
    let entries = (0..12)
        .map(|index| TrayEntry {
            id: format!("item-{index}"),
            title: if index == 0 {
                format!("{}汉", "汉字🙂abcd".repeat(5))
            } else {
                format!("待办 {index}")
            },
        })
        .collect::<Vec<_>>();
    assert_eq!(entries[0].title.chars().count(), 36);
    tray.set_entries(&entries, 12)
        .expect("update real native tray menu");
}

#[test]
fn tray_menu_preserves_item_identity_and_fixed_actions() {
    let id = "01J4ZR8H2V6WQ7F3N9K0M1P4AB";
    assert_eq!(
        menu_action(&format!("complete:{id}")),
        Some(TrayAction::CompleteItem(id.to_owned()))
    );
    assert_eq!(
        menu_action("toggle-panel"),
        Some(TrayAction::Command(CliAction::TogglePanel))
    );
    assert_eq!(
        menu_action("screenshot"),
        Some(TrayAction::Command(CliAction::Screenshot))
    );
    assert_eq!(
        menu_action("save-clipboard"),
        Some(TrayAction::Command(CliAction::SaveClipboard))
    );
    assert_eq!(
        menu_action("items"),
        Some(TrayAction::Command(CliAction::ShowItems))
    );
    assert_eq!(menu_action("quit"), Some(TrayAction::Quit));
    assert_eq!(menu_action("complete:"), None);
    assert_eq!(menu_action("unknown"), None);
}
