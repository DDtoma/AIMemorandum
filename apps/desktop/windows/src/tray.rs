use aimemorandum_ui::{TrayEntry, UiError};
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::CliAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Command(CliAction),
    CompleteItem(String),
    Quit,
}

pub fn menu_action(id: &str) -> Option<TrayAction> {
    match id {
        "toggle-panel" => Some(TrayAction::Command(CliAction::TogglePanel)),
        "screenshot" => Some(TrayAction::Command(CliAction::Screenshot)),
        "save-clipboard" => Some(TrayAction::Command(CliAction::SaveClipboard)),
        "items" => Some(TrayAction::Command(CliAction::ShowItems)),
        "quit" => Some(TrayAction::Quit),
        _ => id
            .strip_prefix("complete:")
            .filter(|item_id| !item_id.is_empty())
            .map(|item_id| TrayAction::CompleteItem(item_id.to_owned())),
    }
}

pub struct NativeTray {
    tray: TrayIcon,
}

impl NativeTray {
    pub fn new() -> Result<Self, UiError> {
        let menu = build_menu(&[])?;
        let icon = application_icon()?;
        // tray-icon 0.26 does not include the GUID in NIM_MODIFY tooltip updates.
        // Use its HWND/internal-id identity so dynamic open-count tooltips work.
        let tray = TrayIconBuilder::new()
            .with_tooltip("AIMemorandum")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .build()
            .map_err(|error| UiError::Platform(format!("create tray icon: {error}")))?;
        Ok(Self { tray })
    }

    pub fn set_entries(&self, entries: &[TrayEntry], open_count: usize) -> Result<(), UiError> {
        self.tray.set_menu(Some(Box::new(build_menu(entries)?)));
        self.tray
            .set_tooltip(Some(format!("AIMemorandum · {open_count} 条待处理")))
            .map_err(|error| UiError::Platform(format!("update tray tooltip: {error}")))
    }
}

fn build_menu(entries: &[TrayEntry]) -> Result<Menu, UiError> {
    let menu = Menu::new();
    for entry in entries {
        let item = MenuItem::with_id(
            format!("complete:{}", entry.id),
            format!("完成 · {}", entry.title),
            true,
            None,
        );
        append(&menu, &item)?;
    }
    if !entries.is_empty() {
        append(&menu, &PredefinedMenuItem::separator())?;
    }
    for (id, title) in [
        ("toggle-panel", "快速记录"),
        ("screenshot", "截图记录"),
        ("save-clipboard", "剪贴板速存"),
        ("items", "查看记录"),
        ("quit", "退出"),
    ] {
        append(&menu, &MenuItem::with_id(id, title, true, None))?;
    }
    Ok(menu)
}

fn append(menu: &Menu, item: &dyn tray_icon::menu::IsMenuItem) -> Result<(), UiError> {
    menu.append(item)
        .map_err(|error| UiError::Platform(format!("build tray menu: {error}")))
}

fn application_icon() -> Result<Icon, UiError> {
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let border = x < 3 || y < 3 || x >= 29 || y >= 29;
            let check = (12..=20).contains(&x) && y == 24 - x / 2;
            let [r, g, b] = if border || check {
                [245, 247, 250]
            } else {
                [39, 108, 176]
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Icon::from_rgba(rgba, 32, 32)
        .map_err(|error| UiError::Platform(format!("create tray icon pixels: {error}")))
}
