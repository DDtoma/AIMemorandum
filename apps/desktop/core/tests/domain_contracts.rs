use aimemorandum_core::{Attachment, InputPolicy, Item, ItemKind, SubmitKey, TaskState};
use chrono::{TimeZone, Utc};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap()
}

#[test]
fn ac_cap_003_composition_and_shift_do_not_submit() {
    assert!(!InputPolicy::should_submit(SubmitKey::Enter, false, true));
    assert!(!InputPolicy::should_submit(SubmitKey::Enter, true, false));
    assert!(InputPolicy::should_submit(SubmitKey::Enter, false, false));
    assert!(InputPolicy::should_submit(
        SubmitKey::NumpadEnter,
        false,
        false
    ));
    assert!(!InputPolicy::should_submit(SubmitKey::Other, false, false));
}

#[test]
fn attachment_revision_starts_at_one_for_upload_protocol() {
    let attachment = Attachment::new("image/png", 4, "a".repeat(64), 1024, 768, 0);
    assert_eq!(attachment.revision, 1);
}

#[test]
fn ac_cap_004_ocr_body_is_retained_and_title_is_generated() {
    let item = Item::new_task(
        "发票金额 128.50 元
税率 13%",
        "screenshot",
        now(),
        "Asia/Shanghai",
        vec![aimemorandum_core::Attachment::new(
            "image/png",
            4,
            "a".repeat(64),
            1024,
            768,
            0,
        )],
    )
    .unwrap();

    assert_eq!(
        item.body_md,
        "发票金额 128.50 元
税率 13%"
    );
    assert!(item.title.starts_with("截图 · "));
    assert_eq!(item.title_origin, "generated");
    assert_eq!(item.kind, ItemKind::Task);
    assert_eq!(item.task_state, Some(TaskState::Todo));
}

#[test]
fn ocr_body_limit_matches_server_utf8_byte_semantics() {
    let over_limit = "字".repeat(87_382); // 262,146 UTF-8 bytes, only 87,382 chars.
    let error = Item::new_task(over_limit, "screenshot", now(), "Asia/Shanghai", vec![])
        .expect_err("server counts body_md in UTF-8 bytes");
    assert_eq!(error.to_string(), "body_md exceeds 262144 UTF-8 bytes");
}

#[test]
fn item_fixture_uses_server_snake_case_contract() {
    let fixture = include_str!("../../fixtures/item.json");
    let item: Item = serde_json::from_str(fixture).unwrap();
    assert_eq!(item.id, "11111111-1111-4111-8111-111111111111");
    assert_eq!(item.body_md, "确认服务器续费价格");
    assert_eq!(item.attachments[0].byte_size, 4);
    assert_eq!(item.reminders[0].item_id, item.id);

    let encoded = serde_json::to_value(&item).unwrap();
    assert!(encoded.get("body_md").is_some());
    assert!(encoded.get("bodyMd").is_none());
    assert_eq!(encoded["source"]["input_method"], "text");
    assert_eq!(encoded["task_state"], "todo");
}
