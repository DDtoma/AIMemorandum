use aimemorandum_ui::TrayEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuEntry {
    pub menu_id: u32,
    pub item_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Default)]
pub struct TrayMenuModel {
    entries: Vec<TrayMenuEntry>,
}

impl TrayMenuModel {
    pub fn new(items: Vec<TrayEntry>) -> Self {
        let entries = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| TrayMenuEntry {
                menu_id: index as u32 + 1,
                item_id: item.id,
                title: item.title,
            })
            .collect();
        Self { entries }
    }

    pub fn entries(&self) -> &[TrayMenuEntry] {
        &self.entries
    }

    pub fn item_for_menu(&self, menu_id: u32) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.menu_id == menu_id)
            .map(|entry| entry.item_id.as_str())
    }
}
