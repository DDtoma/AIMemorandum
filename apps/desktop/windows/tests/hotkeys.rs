#![cfg(windows)]

use aimemorandum_windows::{CliAction, HotkeyBindings, NativeHotkeys};

#[test]
#[ignore = "uses real Windows global hotkey registration"]
fn native_hotkeys_register_and_report_a_real_conflict() {
    let first = NativeHotkeys::new().expect("register real native hotkeys");
    let conflict = match NativeHotkeys::new() {
        Ok(_) => panic!("second registration must conflict"),
        Err(error) => error,
    };
    println!("real hotkey conflict: {conflict}");
    drop(first);
}
use global_hotkey::HotKeyState;

#[test]
fn default_hotkeys_map_only_pressed_events_to_one_action() {
    let bindings = HotkeyBindings::defaults();
    let expected = [
        CliAction::TogglePanel,
        CliAction::Screenshot,
        CliAction::SaveClipboard,
        CliAction::ShowItems,
    ];
    for (binding, action) in bindings.iter().zip(expected) {
        assert_eq!(
            bindings.action_for(binding.id, HotKeyState::Pressed),
            Some(action)
        );
        assert_eq!(bindings.action_for(binding.id, HotKeyState::Released), None);
    }
    assert_eq!(bindings.action_for(u32::MAX, HotKeyState::Pressed), None);
}
