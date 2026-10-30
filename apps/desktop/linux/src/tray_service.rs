use crate::TrayMenuModel;
use aimemorandum_ui::{ShellEvent, ViewModel};
use ksni::{blocking::TrayMethods as _, menu::StandardItem, MenuItem};
use std::sync::mpsc::{self, Receiver, Sender};

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
        let mut menu = vec![
            self.action(
                format!("记录（待办 {}）", self.open_count),
                ShellEvent::ShowCapture,
            ),
            self.action("截图", ShellEvent::Screenshot),
            self.action("保存剪贴板", ShellEvent::SaveClipboard),
            self.action("查看记录", ShellEvent::ShowItems),
            self.action("设置", ShellEvent::ShowSettings),
            MenuItem::Separator,
        ];
        menu.extend(self.model.entries().iter().map(|entry| {
            self.action(
                entry.title.clone(),
                ShellEvent::CompleteItem(entry.item_id.clone()),
            )
        }));
        menu.push(MenuItem::Separator);
        menu.push(self.action("退出", ShellEvent::Quit));
        menu
    }
}

pub struct TrayService {
    updates: Sender<ViewModel>,
}

impl TrayService {
    pub fn start(events: Sender<ShellEvent>) -> Result<Self, ksni::Error> {
        let tray = AppTray {
            model: TrayMenuModel::default(),
            open_count: 0,
            events,
        };
        let handle = tray.spawn()?;
        let (updates, receiver) = mpsc::channel::<ViewModel>();
        std::thread::spawn(move || {
            while let Ok(view) = receiver.recv() {
                let view = latest_pending(&receiver, view);
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
        let _ = self.updates.send(view.clone());
    }
}

fn latest_pending(receiver: &Receiver<ViewModel>, mut latest: ViewModel) -> ViewModel {
    while let Ok(newer) = receiver.try_recv() {
        latest = newer;
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::latest_pending;
    use aimemorandum_ui::ViewModel;
    use std::sync::mpsc;

    #[test]
    fn pending_tray_updates_keep_the_newest_view() {
        let (sender, receiver) = mpsc::channel();
        for open_count in [1, 2, 3] {
            let mut view = ViewModel::default();
            view.tray.open_count = open_count;
            sender.send(view).unwrap();
        }
        let first = receiver.recv().unwrap();
        assert_eq!(latest_pending(&receiver, first).tray.open_count, 3);
    }
}
