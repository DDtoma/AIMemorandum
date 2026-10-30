use aimemorandum_ui::UiError;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyManager, HotKeyState,
};

use crate::CliAction;

#[derive(Debug, Clone, Copy)]
pub struct HotkeyBinding {
    pub id: u32,
    pub action: CliAction,
    hotkey: HotKey,
}

#[derive(Debug)]
pub struct HotkeyBindings {
    bindings: Vec<HotkeyBinding>,
}

impl HotkeyBindings {
    pub fn defaults() -> Self {
        let modifiers = Modifiers::CONTROL | Modifiers::ALT;
        let bindings = [
            (Code::KeyM, CliAction::TogglePanel),
            (Code::KeyS, CliAction::Screenshot),
            (Code::KeyV, CliAction::SaveClipboard),
            (Code::KeyL, CliAction::ShowItems),
        ]
        .into_iter()
        .map(|(code, action)| {
            let hotkey = HotKey::new(Some(modifiers), code);
            HotkeyBinding {
                id: hotkey.id(),
                action,
                hotkey,
            }
        })
        .collect();
        Self { bindings }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, HotkeyBinding> {
        self.bindings.iter()
    }

    pub fn action_for(&self, id: u32, state: HotKeyState) -> Option<CliAction> {
        if state != HotKeyState::Pressed {
            return None;
        }
        self.bindings
            .iter()
            .find(|binding| binding.id == id)
            .map(|binding| binding.action)
    }
}

pub struct NativeHotkeys {
    _manager: GlobalHotKeyManager,
    bindings: HotkeyBindings,
}

impl NativeHotkeys {
    pub fn new() -> Result<Self, UiError> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|error| UiError::Platform(format!("create hotkey manager: {error}")))?;
        let bindings = HotkeyBindings::defaults();
        for binding in bindings.iter() {
            manager.register(binding.hotkey).map_err(|error| {
                UiError::Platform(format!(
                    "register global hotkey for {:?}: {error}; another application may own it",
                    binding.action
                ))
            })?;
        }
        Ok(Self {
            _manager: manager,
            bindings,
        })
    }

    pub fn bindings(&self) -> &HotkeyBindings {
        &self.bindings
    }
}
