use aimemorandum_linux::TrayMenuModel;
use aimemorandum_ui::TrayEntry;

#[test]
fn tray_preserves_shared_titles_and_stable_menu_identity() {
    let shared_title = "汉🙂".repeat(18);
    assert_eq!(shared_title.chars().count(), 36);
    let menu = TrayMenuModel::new(vec![TrayEntry {
        id: "任务-0-🙂".to_owned(),
        title: shared_title.clone(),
    }]);
    let selected = menu.entries().first().unwrap();
    assert_eq!(selected.title, shared_title);
    assert_eq!(menu.item_for_menu(selected.menu_id), Some("任务-0-🙂"));
    assert_eq!(menu.item_for_menu(9999), None);
}
