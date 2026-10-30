use aimemorandum_linux::{TrayItem, TrayMenuModel};

#[test]
fn tray_limits_entries_and_unicode_title_length_without_losing_identity() {
    let items = (0..14)
        .map(|index| TrayItem {
            id: format!("任务-{index}-🙂"),
            title: format!("{index}-{}", "汉🙂".repeat(30)),
        })
        .collect::<Vec<_>>();
    let menu = TrayMenuModel::new(items);
    assert_eq!(menu.entries().len(), 12);
    assert!(menu
        .entries()
        .iter()
        .all(|entry| entry.title.chars().count() == 36));
    let selected = menu.entries().first().unwrap();
    assert_eq!(menu.item_for_menu(selected.menu_id), Some("任务-0-🙂"));
    assert_eq!(menu.item_for_menu(9999), None);
}
