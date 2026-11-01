use crate::{latest::Latest, TrayMenuModel};
use aimemorandum_ui::{
    tray_completion_label, tray_quick_capture_label, ShellEvent, ViewModel, TRAY_CLIPBOARD,
    TRAY_ITEMS, TRAY_QUIT, TRAY_SCREENSHOT,
};
use ksni::{blocking::TrayMethods as _, menu::StandardItem, MenuItem};
use std::sync::{mpsc::Sender, Arc};

struct AppTray {
    model: TrayMenuModel,
    open_count: usize,
    events: Sender<ShellEvent>,
}

impl AppTray {
    fn update(&mut self, view: ViewModel) {
        self.open_count = view.tray.open_count;
        self.model = TrayMenuModel::new(view.tray.entries);
    }

    fn action(&self, label: impl Into<String>, event: ShellEvent) -> MenuItem<Self> {
        let events = self.events.clone();
        StandardItem {
            label: label.into(),
            activate: Box::new(move |_| {
                let _ = events.send(event.clone());
            }),
            ..Default::default()
        }
        .into()
    }
}

impl ksni::Tray for AppTray {
    fn id(&self) -> String {
        "aimemorandum".to_owned()
    }

    fn title(&self) -> String {
        "AIMemorandum".to_owned()
    }

    fn icon_name(&self) -> String {
        "aimemorandum".to_owned()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.events.send(ShellEvent::ToggleCapture);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu = self
            .model
            .entries()
            .iter()
            .map(|entry| {
                self.action(
                    tray_completion_label(&entry.title),
                    ShellEvent::CompleteItem(entry.item_id.clone()),
                )
            })
            .collect::<Vec<_>>();
        if !menu.is_empty() {
            menu.push(MenuItem::Separator);
        }
        menu.push(self.action(
            tray_quick_capture_label(self.open_count),
            ShellEvent::ShowCapture,
        ));
        menu.push(self.action(TRAY_SCREENSHOT, ShellEvent::Screenshot));
        menu.push(self.action(TRAY_CLIPBOARD, ShellEvent::SaveClipboard));
        menu.push(self.action(TRAY_ITEMS, ShellEvent::ShowItems));
        menu.push(self.action("设置", ShellEvent::ShowSettings));
        menu.push(MenuItem::Separator);
        menu.push(self.action(TRAY_QUIT, ShellEvent::Quit));
        menu
    }
}

pub struct TrayService {
    updates: Arc<Latest<ViewModel>>,
}

impl TrayService {
    pub fn start(events: Sender<ShellEvent>) -> Result<Self, ksni::Error> {
        let tray = AppTray {
            model: TrayMenuModel::default(),
            open_count: 0,
            events,
        };
        let handle = tray.spawn()?;
        let updates = Arc::new(Latest::new());
        let worker_updates = Arc::clone(&updates);
        std::thread::spawn(move || {
            while let Some(view) = worker_updates.wait() {
                if handle
                    .update(|tray| {
                        tray.update(view);
                    })
                    .is_none()
                {
                    break;
                }
            }
            handle.shutdown().wait();
        });
        Ok(Self { updates })
    }

    pub fn publish(&self, view: &ViewModel) {
        self.updates.publish(view.clone());
    }
}

impl Drop for TrayService {
    fn drop(&mut self) {
        self.updates.close();
    }
}
