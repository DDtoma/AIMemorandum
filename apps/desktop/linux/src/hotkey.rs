use crate::SessionKind;
use aimemorandum_ui::{
    ShellEvent, TRAY_CLIPBOARD, TRAY_ITEMS, TRAY_QUICK_CAPTURE, TRAY_SCREENSHOT,
};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::{sync::mpsc::Sender, thread::JoinHandle};

pub enum HotkeyService {
    X11 { _manager: GlobalHotKeyManager },
    Wayland { _portal: PortalShortcutService },
}

pub struct PortalShortcutService {
    _thread: JoinHandle<()>,
}

pub fn start_hotkeys(
    session: SessionKind,
    events: Sender<ShellEvent>,
) -> Result<HotkeyService, String> {
    match session {
        SessionKind::X11 => start_x11_hotkeys(events)
            .map(|manager| HotkeyService::X11 { _manager: manager })
            .map_err(|error| error.to_string()),
        SessionKind::Wayland => {
            start_wayland_hotkeys(events).map(|portal| HotkeyService::Wayland { _portal: portal })
        }
    }
}

pub fn start_x11_hotkeys(
    events: Sender<ShellEvent>,
) -> Result<GlobalHotKeyManager, global_hotkey::Error> {
    let manager = GlobalHotKeyManager::new()?;
    let modifiers = Modifiers::CONTROL | Modifiers::ALT;
    let bindings = [
        (
            ShellEvent::ToggleCapture,
            HotKey::new(Some(modifiers), Code::KeyM),
        ),
        (
            ShellEvent::Screenshot,
            HotKey::new(Some(modifiers), Code::KeyS),
        ),
        (
            ShellEvent::SaveClipboard,
            HotKey::new(Some(modifiers), Code::KeyV),
        ),
        (
            ShellEvent::ShowItems,
            HotKey::new(Some(modifiers), Code::KeyL),
        ),
    ];
    let mut registered = 0_usize;
    let mut last_error = None;
    for (action, hotkey) in &bindings {
        match manager.register(*hotkey) {
            Ok(()) => registered += 1,
            Err(error) => {
                eprintln!("全局快捷键 {action:?} ({hotkey:?}) 注册失败：{error}");
                last_error = Some(error);
            }
        }
    }
    if registered == 0 {
        return Err(last_error.expect("at least one hotkey binding exists"));
    }
    let receiver = GlobalHotKeyEvent::receiver().clone();
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some((action, _)) = bindings.iter().find(|(_, hotkey)| hotkey.id() == event.id) {
                if events.send(action.clone()).is_err() {
                    break;
                }
            }
        }
    });
    Ok(manager)
}

pub fn start_wayland_hotkeys(events: Sender<ShellEvent>) -> Result<PortalShortcutService, String> {
    let thread = std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| runtime.block_on(run_portal_shortcuts(events)));
        if let Err(error) = result {
            eprintln!("Wayland 全局快捷键不可用：{error}；CLI 仍可使用");
        }
    });
    // Portal authorization is user-driven and must never delay application startup.
    Ok(PortalShortcutService { _thread: thread })
}

async fn run_portal_shortcuts(events: Sender<ShellEvent>) -> Result<(), String> {
    let proxy = GlobalShortcuts::new().await.map_err(display)?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(display)?;
    let mut activated = proxy.receive_activated().await.map_err(display)?;
    let shortcuts = [
        NewShortcut::new("toggle-panel", TRAY_QUICK_CAPTURE).preferred_trigger("CTRL+ALT+M"),
        NewShortcut::new("screenshot", TRAY_SCREENSHOT).preferred_trigger("CTRL+ALT+S"),
        NewShortcut::new("save-clipboard", TRAY_CLIPBOARD).preferred_trigger("CTRL+ALT+V"),
        NewShortcut::new("items", TRAY_ITEMS).preferred_trigger("CTRL+ALT+L"),
    ];
    let bound = proxy
        .bind_shortcuts(&session, &shortcuts, None, Default::default())
        .await
        .map_err(display)?
        .response()
        .map_err(display)?;
    for shortcut in bound.shortcuts() {
        eprintln!(
            "Wayland 快捷键 {}：{}",
            shortcut.id(),
            shortcut.trigger_description()
        );
    }
    while let Some(event) = activated.next().await {
        let action = match event.shortcut_id() {
            "toggle-panel" => ShellEvent::ToggleCapture,
            "screenshot" => ShellEvent::Screenshot,
            "save-clipboard" => ShellEvent::SaveClipboard,
            "items" => ShellEvent::ShowItems,
            _ => continue,
        };
        if events.send(action).is_err() {
            return Ok(());
        }
    }
    Err("Wayland 全局快捷键事件流已结束".to_owned())
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}
