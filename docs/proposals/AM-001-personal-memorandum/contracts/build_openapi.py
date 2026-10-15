"""Generate the design-only OpenAPI document. Standard library, no network."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def ref(name):
    return {"$ref": f"#/components/schemas/{name}"}


def obj(properties, required=(), **extra):
    return {"type": "object", "properties": properties, "required": list(required),
            "additionalProperties": False, **extra}


def array(value, **extra):
    return {"type": "array", "items": value, **extra}


def enum(*values):
    return {"type": "string", "enum": list(values)}


def nullable(value):
    return {"anyOf": [value, {"type": "null"}]}


STR = {"type": "string"}
UUID = {"type": "string", "format": "uuid"}
TIME = {"type": "string", "format": "date-time", "description": "UTC RFC3339，输出使用 Z"}
DATE = {"type": "string", "format": "date"}
REV = {"type": "integer", "minimum": 1, "format": "int64"}
BOOL = {"type": "boolean"}
NOTE = {"type": "string", "maxLength": 8192, "x-max-utf8-bytes": 8192}
KIND = enum("capture", "task", "memo")
STATE = enum("todo", "in_progress", "waiting", "paused", "completed", "canceled")
schemas = {}
schemas["Error"] = obj({
    "error": obj({"code": STR, "message": STR}, ["code", "message"]),
    "operation_id": UUID, "request_id": STR, "current_revision": REV,
}, ["error", "request_id"])
schemas["Source"] = obj({"input_method": enum("text", "screenshot", "clipboard", "drop", "api"),
                         "reported_app": STR}, ["input_method"])
schemas["ExternalRef"] = obj({"type": STR, "system": STR, "value": STR, "device_id": UUID},
                             ["type", "system", "value"])
attachment_input = {
    "id": UUID, "mime": enum("image/png", "image/jpeg"),
    "byte_size": {"type": "integer", "minimum": 1, "maximum": 20971520},
    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
    "pixel_width": {"type": "integer", "minimum": 1},
    "pixel_height": {"type": "integer", "minimum": 1},
    "position": {"type": "integer", "minimum": 0}, "caption": NOTE,
}
schemas["AttachmentInput"] = obj(attachment_input,
    ["id", "mime", "byte_size", "sha256", "pixel_width", "pixel_height", "position"],
    description="像素乘积最多40000000；每 Item 最多10张，总字节最多52428800，领域层校验。")
schemas["Attachment"] = obj({**attachment_input, "owner_id": UUID, "item_id": UUID,
    "state": enum("pending", "ready", "failed", "deleted"), "revision": REV,
    "updated_at": TIME}, [key for key in attachment_input if key != "caption"] +
    ["owner_id", "item_id", "state", "revision", "updated_at"])
schemas["ReminderRule"] = obj({"id": UUID, "item_id": UUID, "remind_at": TIME,
    "schedule_revision": UUID, "purpose": enum("explicit", "follow_up"), "enabled": BOOL, "revision": REV},
    ["id", "item_id", "remind_at", "schedule_revision", "purpose", "enabled", "revision"])
schemas["Occurrence"] = obj({"id": UUID, "rule_id": UUID, "item_id": UUID,
    "schedule_revision": UUID, "occurrence_at": TIME,
    "status": enum("scheduled", "due", "superseded", "canceled"), "revision": REV},
    ["id", "rule_id", "item_id", "schedule_revision", "occurrence_at", "status", "revision"])
edit_fields = {
    "title": {"type": "string", "maxLength": 200}, "title_origin": enum("user", "generated"),
    "body_md": {"type": "string", "maxLength": 262144, "x-max-utf8-bytes": 262144},
    "group_id": nullable(UUID), "next_action": NOTE, "resume_note": NOTE,
    "planned_for": nullable(DATE), "due_at": nullable(TIME), "due_date": nullable(DATE),
    "review_at": nullable(TIME), "timezone": STR,
    "external_refs": array(ref("ExternalRef"), maxItems=100),
}
create_fields = {**edit_fields, "id": UUID, "kind": KIND,
    "task_state": nullable(enum("todo")), "source": ref("Source"),
    "captured_at": TIME, "attachments": array(ref("AttachmentInput"), maxItems=10)}
schemas["ItemInput"] = obj(create_fields, ["id", "kind", "timezone", "source"],
    description="新建 task 默认 todo；capture/memo 状态为空。空用户文字且无附件禁止，生成标题不单独满足非空约束。due_at/due_date 二选一。")
item_fields = {**edit_fields, "id": UUID, "owner_id": UUID, "kind": KIND,
    "task_state": nullable(STATE), "waiting_for": NOTE, "revision": REV,
    "created_at": TIME, "updated_at": TIME, "captured_at": nullable(TIME),
    "last_progress_at": nullable(TIME), "last_reviewed_at": nullable(TIME),
    "next_revisit_on": nullable(DATE), "completed_at": nullable(TIME),
    "canceled_at": nullable(TIME), "deleted_at": nullable(TIME),
    "source": ref("Source"), "attachments": array(ref("Attachment")),
    "reminders": array(ref("ReminderRule"))}
schemas["Item"] = obj(item_fields,
    ["id", "owner_id", "kind", "task_state", "title", "body_md", "timezone", "revision",
     "created_at", "updated_at", "attachments", "reminders"])
schemas["CreateItem"] = obj({"operation_id": UUID, "item": ref("ItemInput")}, ["operation_id", "item"])
schemas["PatchItem"] = obj({"operation_id": UUID, "base_revision": REV,
    "patch": obj(edit_fields, minProperties=1)}, ["operation_id", "base_revision", "patch"],
    description="省略不变，允许 null 的字段用 null 清空；状态、类型、进展时间不能通过 PATCH 绕过领域动作。")


def action(name, fields=None, required=(), **extra):
    return obj({"operation_id": UUID, "base_revision": REV, "action": {"const": name},
                "payload": obj(fields or {}, required, **extra)},
               ["operation_id", "base_revision", "action", "payload"])


actions = [action(name) for name in ["start", "complete", "cancel", "reopen", "delete", "restore"]]
actions += [action("wait", {"waiting_for": {"type": "string", "minLength": 1, "maxLength": 8192},
                          "review_at": nullable(TIME), "next_action": NOTE,
                          "notification": obj({"reminder_id": UUID, "remind_at": TIME, "schedule_revision": UUID},
                                              ["reminder_id", "remind_at", "schedule_revision"])}, ["waiting_for", "review_at"]),
    action("pause", {"review_at": nullable(TIME)}, ["review_at"]),
    action("receive_result", {"resume_note": NOTE}),
    action("append_progress", {"next_action": NOTE, "resume_note": NOTE}, minProperties=1),
    action("convert", {"kind": KIND}, ["kind"])]
reminder_fields = {"reminder_id": UUID, "remind_at": TIME, "schedule_revision": UUID}
actions += [action(name, reminder_fields, reminder_fields.keys())
            for name in ["set_reminder", "capture_and_remind"]]
actions += [action("snooze", {**reminder_fields, "expected_schedule_revision": UUID},
                   [*reminder_fields, "expected_schedule_revision"]),
    action("remove_reminder", {"reminder_id": UUID, "expected_schedule_revision": UUID},
           ["reminder_id", "expected_schedule_revision"]),
    action("attach", {"attachments": array(ref("AttachmentInput"), minItems=1, maxItems=10)}, ["attachments"]),
    action("detach", {"attachment_id": UUID}, ["attachment_id"]),
    action("undo", {"target_operation_id": UUID}, ["target_operation_id"])]
for decision, fields, required in [
    ("today", {"planned_for": DATE}, ["planned_for"]),
    ("tomorrow_review", {"next_revisit_on": DATE}, ["next_revisit_on"]),
    ("set_review", {"review_at": TIME}, ["review_at"]),
    ("keep_waiting", {"review_at": nullable(TIME), "next_revisit_on": nullable(DATE)}, ["review_at"]),
]:
    actions.append(action("review", {"decision": {"const": decision}, **fields}, ["decision", *required]))
schemas["ItemAction"] = {"oneOf": actions,
    "description": "action + payload 决策严格匹配；领域层还校验非空进展、时间未来性、状态与 scope。"}
schemas["WriteResult"] = obj({"operation_id": UUID, "sync_epoch": UUID, "item": ref("Item")},
                             ["operation_id", "sync_epoch", "item"])
schemas["Group"] = obj({"id": UUID, "name": {"type": "string", "minLength": 1, "maxLength": 100},
    "revision": REV, "deleted_at": nullable(TIME)}, ["id", "name", "revision"])
schemas["Device"] = obj({"id": UUID, "name": STR, "scopes": array(STR), "revision": REV,
    "revoked_at": nullable(TIME), "last_seen_at": nullable(TIME)}, ["id", "name", "scopes", "revision"])
settings_fields = {"home_timezone": STR, "daily_review_enabled": BOOL,
    "daily_review_time": nullable({"type": "string", "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$"}),
    "stale_after_days": {"type": "integer", "minimum": 1, "maximum": 365}}
schemas["Settings"] = obj({**settings_fields, "revision": REV}, [*settings_fields, "revision"])
schemas["PatchSettings"] = obj({"operation_id": UUID, "base_revision": REV,
    "patch": obj(settings_fields, minProperties=1)}, ["operation_id", "base_revision", "patch"])
schemas["VersionOperation"] = obj({"operation_id": UUID, "base_revision": REV}, ["operation_id", "base_revision"])
schemas["Operation"] = obj({"operation_id": UUID}, ["operation_id"])
schemas["OperationResult"] = obj({"operation_id": UUID, "sync_epoch": UUID}, ["operation_id", "sync_epoch"])
schemas["Entity"] = obj({"entity_type": enum("item", "attachment", "reminder_rule", "occurrence", "group", "settings", "device"),
    "entity_id": UUID, "revision": REV, "change_type": enum("upsert", "delete"),
    "data": {"oneOf": [ref(n) for n in ["Item", "Attachment", "ReminderRule", "Occurrence", "Group", "Settings", "Device"]] + [{"type": "null"}]}},
    ["entity_type", "entity_id", "revision", "change_type", "data"],
    description="entity_type 与 data 类型需匹配；delete 采用 tombstone，data=null；settings 的 entity_id 为 owner_id。")
schemas["Change"] = obj({**schemas["Entity"]["properties"], "seq": {"type": "integer", "minimum": 1}},
                        [*schemas["Entity"]["required"], "seq"])
schemas["SyncPage"] = obj({"sync_epoch": UUID, "changes": array(ref("Change")), "next_cursor": STR,
    "has_more": BOOL, "server_time": TIME}, ["sync_epoch", "changes", "next_cursor", "has_more", "server_time"])
schemas["SnapshotJob"] = obj({"id": UUID, "state": enum("pending", "ready", "failed"),
    "sync_epoch": UUID, "expires_at": TIME, "high_watermark_cursor": nullable(STR)},
    ["id", "state", "sync_epoch", "expires_at", "high_watermark_cursor"])
schemas["SnapshotPage"] = obj({"snapshot": ref("SnapshotJob"), "entities": array(ref("Entity")),
    "next_page_token": nullable(STR), "has_more": BOOL}, ["snapshot", "entities", "next_page_token", "has_more"])
schemas["Event"] = obj({"id": UUID, "item_id": UUID, "operation_id": UUID, "event_type": STR,
    "actor": obj({"type": enum("user", "device", "integration", "system"), "id": STR}, ["type", "id"]),
    "occurred_at": TIME, "reported_at": nullable(TIME), "summary": STR,
    "before": nullable(ref("Item")), "after": nullable(ref("Item"))},
    ["id", "item_id", "operation_id", "event_type", "actor", "occurred_at", "summary"])
schemas["Export"] = obj({"id": UUID, "state": enum("queued", "running", "ready", "failed"),
    "expires_at": nullable(TIME), "missing_attachment_ids": array(UUID), "error_code": nullable(STR)},
    ["id", "state", "expires_at", "missing_attachment_ids", "error_code"])
schemas["DeliveryInput"] = obj({"operation_id": UUID, "occurrence_id": UUID, "request_id": STR,
    "schedule_revision": UUID, "registration_state": enum("accepted", "rejected", "removed", "unknown"),
    "interaction": nullable(enum("opened", "dismissed", "complete_selected", "snooze_selected")),
    "observed_at": TIME}, ["operation_id", "occurrence_id", "request_id", "schedule_revision", "registration_state", "observed_at"],
    description="device 身份来自认证；交互日志本身不完成事项，完成与稍后须提交相应领域动作。")
schemas["Enrollment"] = obj({"operation_id": UUID, "device_id": UUID, "device_name": STR,
    "enrollment_code": {"type": "string", "minLength": 16}}, ["operation_id", "device_id", "device_name", "enrollment_code"])
schemas["EnrollmentResult"] = obj({"device": ref("Device"), "access_token": STR, "sync_epoch": UUID},
                                   ["device", "access_token", "sync_epoch"])
schemas["Capabilities"] = obj({"api_version": {"const": "v1"}, "sync_epoch": UUID,
    "actions": array(STR), "attachment_mimes": array(STR), "max_attachment_bytes": {"const": 20971520},
    "max_item_attachment_bytes": {"const": 52428800}, "max_attachments_per_item": {"const": 10},
    "max_attachment_pixels": {"const": 40000000}},
    ["api_version", "sync_epoch", "actions", "attachment_mimes", "max_attachment_bytes",
     "max_item_attachment_bytes", "max_attachments_per_item", "max_attachment_pixels"])


def page(name):
    return obj({"data": array(ref(name)), "next_cursor": nullable(STR),
                "total": {"type": "integer", "minimum": 0}}, ["data", "next_cursor", "total"])


for n in ["Item", "Group", "Device", "Event"]:
    schemas[n + "Page"] = page(n)

paths = {}


def param(name, where, schema, required=False, description=""):
    return {"name": name, "in": where, "required": required, "schema": schema, "description": description}


PAGINATION = [param("cursor", "query", STR), param("limit", "query", {"type": "integer", "minimum": 1, "maximum": 200, "default": 50})]


def add(path, method, operation_id, summary, response, request=None, params=None,
        status="200", content_type="application/json", public=False, description=""):
    operation = {"operationId": operation_id, "summary": summary, "description": description,
        "responses": {status: {"description": "成功；不代表通知已被用户看到", "content": {content_type: {"schema": response}}}},
        "parameters": list(params or [])}
    if "{id}" in path:
        operation["parameters"].insert(0, param("id", "path", UUID, True))
    if method in ["post", "put", "patch", "delete"] and not public:
        operation["parameters"].append(param("X-Sync-Epoch", "header", UUID, True,
            "提交前取得当前 epoch；不匹配返回410，暂停旧操作并重建同步。"))
    if method in ["post", "put", "patch", "delete"]:
        operation["responses"][status]["headers"] = {
            "X-Operation-ID": {"schema": UUID, "description": "本次幂等操作标识"},
            "X-Sync-Epoch": {"schema": UUID, "description": "当前同步代际"}}
    if request:
        operation["requestBody"] = {"required": True, "content": {"application/json": {"schema": request}}}
    if public:
        operation["security"] = []
    for code, label in [("400", "请求结构错误"), ("401", "未授权"), ("403", "范围不足"),
                        ("404", "资源不可见"), ("409", "版本/幂等/调度冲突"),
                        ("410", "需重建同步或数据已清理"), ("413", "大小超限"),
                        ("422", "领域约束不满足"), ("429", "限流，检查Retry-After"), ("503", "临时不可用")]:
        operation["responses"][code] = {"description": label, "content": {"application/json": {"schema": ref("Error")}}}
    paths.setdefault("/api/v1" + path, {})[method] = operation
    return operation


add("/capabilities", "get", "getCapabilities", "读取能力与当前同步代际", ref("Capabilities"))
add("/device-enrollments", "post", "enrollDevice", "一次性注册码换设备凭据", ref("EnrollmentResult"), ref("Enrollment"), public=True,
    description="同 operation_id 重试应返回同一设备凭据；短期加密保存注册结果以应对响应丢失，不重复消耗注册码。")
add("/devices", "get", "listDevices", "查看授权设备", ref("DevicePage"), params=PAGINATION)
add("/devices/{id}/revoke", "post", "revokeDevice", "撤销设备", ref("OperationResult"), ref("VersionOperation"))
add("/settings", "get", "getSettings", "读取同步的账号回顾设置", ref("Settings"))
add("/settings", "patch", "patchSettings", "修改账号回顾设置", ref("Settings"), ref("PatchSettings"))
add("/items", "post", "createItem", "创建文字或图片事项", ref("WriteResult"), ref("CreateItem"), status="201")
add("/items", "get", "listItems", "查询事项与搜索", ref("ItemPage"), params=PAGINATION + [
    param("q", "query", STR, description="按字面搜索，不解释标点为操作符"),
    param("view", "query", enum("all", "attention", "review", "waiting", "completed", "trash")),
    param("kind", "query", KIND), param("group_id", "query", UUID)])
add("/items/{id}", "get", "getItem", "读取事项及其提醒和附件", ref("Item"))
add("/items/{id}", "patch", "patchItem", "修改事项字段", ref("WriteResult"), ref("PatchItem"))
add("/items/{id}/actions", "post", "actOnItem", "执行有版本校验的领域动作", ref("WriteResult"), ref("ItemAction"))
add("/items/{id}/events", "get", "listItemEvents", "读取事项历史", ref("EventPage"), params=PAGINATION)
add("/groups", "get", "listGroups", "查询分组", ref("GroupPage"), params=PAGINATION)
add("/groups", "post", "createGroup", "创建可选分组", ref("Group"), obj({"operation_id": UUID, "id": UUID,
    "name": schemas["Group"]["properties"]["name"]}, ["operation_id", "id", "name"]), status="201")
add("/groups/{id}", "patch", "patchGroup", "重命名分组", ref("Group"), obj({"operation_id": UUID,
    "base_revision": REV, "name": schemas["Group"]["properties"]["name"]}, ["operation_id", "base_revision", "name"]))
add("/groups/{id}", "delete", "deleteGroup", "删除分组并解除事项关联", ref("OperationResult"), ref("VersionOperation"))
upload = add("/attachments/{id}/content", "put", "uploadAttachment", "上传已声明的图片，哈希一致才 ready", ref("Attachment"),
    params=[param("Idempotency-Key", "header", UUID, True),
            param("X-Content-SHA256", "header", {"type": "string", "pattern": "^[0-9a-f]{64}$"}, True)])
upload["requestBody"] = {"required": True, "content": {m: {"schema": {"type": "string", "format": "binary"}}
    for m in ["image/png", "image/jpeg"]}}
download = add("/attachments/{id}/content", "get", "downloadAttachment", "鉴权读取图片", {"type": "string", "format": "binary"},
    params=[param("Range", "header", STR)], content_type="image/png")
download["responses"]["200"]["content"]["image/jpeg"] = {"schema": {"type": "string", "format": "binary"}}
download["responses"]["206"] = {"description": "部分内容", "headers": {"Content-Range": {"schema": STR}},
    "content": {"application/octet-stream": {"schema": {"type": "string", "format": "binary"}}}}
download["responses"]["416"] = {"description": "Range 不可满足"}
add("/sync", "get", "pullChanges", "按持久化游标拉取变更", ref("SyncPage"), params=[
    param("cursor", "query", STR, True), param("limit", "query", {"type": "integer", "minimum": 1, "maximum": 500, "default": 200})])
add("/sync/snapshots", "post", "createSnapshot", "创建固定一致快照", ref("SnapshotJob"), ref("Operation"), status="202")
add("/sync/snapshots/{id}", "get", "getSnapshot", "读取同一固定快照的分页", ref("SnapshotPage"), params=[param("page_token", "query", STR)])
add("/events/stream", "get", "streamHints", "SSE变更提示，可靠数据必须拉取sync", STR, content_type="text/event-stream")
add("/notification-deliveries", "post", "reportDelivery", "登记本设备的可观测通知状态", ref("OperationResult"), ref("DeliveryInput"))
add("/exports", "post", "createExport", "导出JSON、Markdown和图片为ZIP", ref("Export"), obj({"operation_id": UUID,
    "include_events": BOOL}, ["operation_id", "include_events"]), status="202")
add("/exports/{id}", "get", "getExport", "读取导出进度与缺图清单", ref("Export"))
add("/exports/{id}/content", "get", "downloadExport", "鉴权下载已完成导出", {"type": "string", "format": "binary"}, content_type="application/zip")

document = {
    "openapi": "3.1.0", "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
    "info": {"title": "AIMemorandum API 设计草案", "version": "0.1.0-design",
        "description": "未实现。领域约束见00_shared/01_domain_and_api.md。认证确定owner与actor；所有写操作幂等，已有实体修改携版本。结构校验不能代替状态、时间与权限校验。"},
    "servers": [{"url": "https://memorandum.example.invalid", "description": "占位地址，非已部署服务"}],
    "security": [{"DeviceBearer": []}], "paths": paths,
    "components": {"securitySchemes": {"DeviceBearer": {"type": "http", "scheme": "bearer",
        "description": "独立设备凭据，scopes由服务端检查，Keychain保存。"}}, "schemas": schemas},
}
target = ROOT / "openapi.json"
assert target.parent == ROOT
target.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"Generated {target.name}: {len(paths)} paths, {sum(len(v) for v in paths.values())} operations, {len(schemas)} schemas")
