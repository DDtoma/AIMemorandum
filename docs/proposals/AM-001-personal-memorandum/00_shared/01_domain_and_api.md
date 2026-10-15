# 领域模型与接口契约

## 1. 契约边界

共享行为以本文件和[同步规则](02_sync_and_reminders.md)为准；[OpenAPI](../contracts/openapi.json)提供机器可读的传输结构。首版 `/api/v1`，JSON UTF-8，图片使用二进制流。新增可选字段保持兼容；新增动作、枚举必须经 capabilities 协商，旧客户端不可把未知状态降级成 todo 并回写。

UUID 由客户端生成，服务端校验；时间点 RFC 3339，传输统一 UTC `Z`，日期为 `YYYY-MM-DD`。时区使用 IANA 名称。`owner_id`、操作者、服务端 revision 和时间戳只由服务端产生；客户端 `source` 仅描述输入方式，不能授予身份。

## 2. Item

| 字段 | 类型 | 约束与解释 |
|---|---|---|
| id / owner_id | UUID | id 在用户范围内永久标识同一事项 |
| kind | capture/task/memo | capture、memo 的 task_state 必须为空 |
| task_state | 可空枚举 | todo/in_progress/waiting/paused/completed/canceled |
| title | string | 最多 200 字符；纯图片可生成标题 |
| title_origin | user/generated | 保留生成标题与用户标题的区别 |
| body_md | string | UTF-8 最多 256 KiB；空文字必须有附件 |
| group_id | UUID? | 可选单分组；必须同 owner |
| next_action / resume_note / waiting_for | string | 各最多 8 KiB；waiting 状态要求 waiting_for 非空 |
| planned_for | date? | 计划日期 |
| due_at / due_date | timestamp?/date? | 至多一个非空 |
| review_at | timestamp? | 复查时间，和通知独立 |
| timezone | string | 用于日期解释与时间展示，不随设备自动改动 |
| revision | int64 | 服务端从 1 开始；每次修改聚合加 1 |
| created_at / updated_at | timestamp | 服务端时间；另存 captured_at 作为设备报告的记录时间 |
| last_progress_at | timestamp? | 仅明确进展、检查清单实质推进或用户确认的下一步变更更新 |
| last_reviewed_at / next_revisit_on | timestamp?/date? | 仅明确回顾决策更新，查看/搜索不更新 |
| completed_at / canceled_at / deleted_at | timestamp? | 对应当前完成、取消、软删除；历史事件保留原值 |
| source | object | input_method、reported_app 可选；不自动读取网页 URL/窗口标题 |
| external_refs | array | type/system/value；goal_id、run_id、url、related_item 等引用 |
| attachments | array | 有顺序的 Attachment 元数据引用，不放 base64 |

保存合法性：title/body_md 中有非空用户内容，或至少一个合法 attachment；系统生成标题不能单独让空白记录合法。创建 task 默认 todo；创建 capture/memo 不允许带待办状态或任务时间。状态变更只能通过领域动作，PATCH 不能绕过结束、转类型或提醒校验。

`last_progress_at` 使用服务端收到明确进展的时间，历史事件另存客户端 reported_at；离线时本地投影可临时使用设备时间并注明待同步。改错别字、改时间、接收 OCR 派生文本均不算进展。不会因某次图片上传完成而重置停滞时间。

## 3. 其他实体

| 实体 | 字段与约束 |
|---|---|
| Group | id、owner_id、name、revision、deleted_at；删除分组不删除事项，原子解除关联 |
| ItemEvent | id、item_id、operation_id、event_type、actor、occurred_at、reported_at、变更摘要和可恢复版本；正文不写普通日志 |
| Attachment | id、owner_id、item_id、mime、byte_size、sha256、pixel_width/height、position、caption、state、revision |
| ReminderRule | id、item_id、remind_at、schedule_revision(UUID)、purpose(explicit/follow_up)、enabled、revision；首版一次性，最多每项 8 条有效规则 |
| ReminderOccurrence | id、rule_id、schedule_revision、occurrence_at、status；唯一(rule_id,schedule_revision,occurrence_at) |
| NotificationDelivery | occurrence_id、device_id、request_id、registration_state、interaction、observed_at；不能用 registered 推断 viewed |
| Device | id、owner_id、name、token_hash、scopes、revoked_at、last_seen_at、sync_cursor、通知能力 |
| OperationReceipt | owner_id、operation_id、actor_id、request_hash、HTTP 结果、result_revision、created_at；唯一(owner_id,operation_id) |
| ChangeLog | owner_id、seq、entity_type、entity_id、revision、change_type、payload；唯一(owner_id,seq) |
| UserSyncState | owner_id、last_seq、sync_epoch、min_retained_seq；按 owner 行锁保证提交顺序 |
| Snapshot | id、owner_id、sync_epoch、high_watermark、expires_at、分块内容；固定快照，不用多页实时查询冒充一致快照 |
| ItemIdentity | owner_id、item_id、purged_at；永久保留最小身份记录，防止清理后相同 ID 复活 |

Attachment 的 state 为 pending/ready/failed/deleted；客户端另有 local_state，不从服务端覆盖。正文修改、附件关联变更、提醒变化均增加 Item revision；仅上传从 pending 转 ready 增加 Attachment revision，不占用 Item revision，避免上传完成制造编辑冲突。所有可同步变化都有自己的 change_log 条目。

本机文件引用：external_refs 中保存 device_id 和显示路径；security-scoped bookmark 只留所属设备 Keychain/本地受保护存储，不上传。外部 URL 允许 https/http 以及显式白名单方案，不执行任意 shell 或 javascript URL。

## 4. 领域动作

请求统一带 `operation_id`、`base_revision`、`action`、`payload`。对不同动作使用 OpenAPI oneOf，不接受任意字段组合。

| action | payload | 行为 |
|---|---|---|
| start | 空 | 未结束 task → in_progress |
| wait | waiting_for、review_at 可空、next_action 可选、notification 可选 | → waiting；空复查时间加入日常回顾；notification 存在时在同一事务建立 purpose=follow_up 的提醒，时间须等于非空 review_at |
| pause | review_at 可空 | → paused；无复查时间也回顾 |
| receive_result | resume_note 可选 | waiting → todo；清除 waiting_for/review_at、失效 purpose=follow_up 的提醒，独立 explicit 提醒保留，历史保留 |
| append_progress | next_action/resume_note 至少一项非空 | 更新进展并设置 last_progress_at；不改变终态 |
| complete / cancel | 空 | 结束 task；失效全部提醒和关注 |
| reopen | 空 | completed/canceled → todo；不自动恢复旧提醒 |
| convert | kind | capture/task/memo 转换；目标 task 默认 todo；转 memo 失效任务时间和提醒 |
| set_reminder | reminder_id、remind_at、schedule_revision | task 上新建/替换一次性提醒；校验数量和未来时间 |
| capture_and_remind | reminder_id、remind_at、schedule_revision | capture/memo 原子转 task 并创建提醒，UI 明示该转换 |
| snooze | reminder_id、expected_schedule_revision、remind_at、schedule_revision | 校验当前规则后替换调度；不改 due/planned/review |
| remove_reminder | reminder_id、expected_schedule_revision | 失效指定规则与实例 |
| review | decision、planned_for/review_at/next_revisit_on 按决策提供 | today、tomorrow_review、set_review、keep_waiting；记录明确回顾 |
| delete / restore | 空 | 软删/回收站恢复；restore 不自动补发过去通知 |
| attach | attachments | 追加合法附件元数据；图片可随后上传 |
| detach | attachment_id | 解除关联并软删附件，进入回收流程 |
| undo | target_operation_id | 撤销可逆操作，必须基于当前 revision；不逆转已物理清理的内容 |

禁止规则：任何提醒动作不得使 completed/canceled/memo/deleted 的事项重新产生提醒；旧通知携带的 schedule_revision 已变更时返回 `STALE_SCHEDULE`。终态上 append_progress 返回 `INVALID_STATE`，若需继续则明确 reopen。删除项除读取回收站、恢复外不接受普通修改。

## 5. API 划分

| 接口 | 作用 |
|---|---|
| GET /api/v1/capabilities | 协议版本、支持动作、附件限制、sync_epoch |
| GET/PATCH /api/v1/settings | home_timezone、日常回顾偏好，带版本与操作 ID，同步传播 |
| POST /api/v1/device-enrollments | 一次性注册码换设备凭据 |
| GET /api/v1/devices；POST /devices/{id}/revoke | 查看设备、撤销设备 |
| POST /api/v1/items | 新建 Item，可含待上传附件元数据 |
| GET /api/v1/items；GET /items/{id} | 查询列表、详情、必要关联；搜索 q 按字面子串 |
| PATCH /api/v1/items/{id} | 修改文本、分组、时间字段；省略不变，null 清空，受领域约束 |
| POST /api/v1/items/{id}/actions | 上述领域动作 |
| GET /api/v1/items/{id}/events | 查看历史，稳定游标分页 |
| GET/POST /api/v1/groups；PATCH/DELETE /groups/{id} | 分组管理；DELETE 也带操作与版本 |
| PUT /api/v1/attachments/{id}/content | 图片流上传，校验哈希；可整文件重试 |
| GET /api/v1/attachments/{id}/content | 鉴权读取 ready 图片，支持 Range |
| GET /api/v1/sync | 增量变更分页 |
| POST /api/v1/sync/snapshots；GET /sync/snapshots/{id} | 生成/分页读取固定快照 |
| GET /api/v1/events/stream | SSE 提示，不承载可靠业务数据 |
| POST /api/v1/notification-deliveries | 设备登记状态和用户交互事件，幂等 |
| POST /api/v1/exports；GET /exports/{id}；GET /exports/{id}/content | 创建导出任务、状态、鉴权下载 |

正文、动作、附件关联均通过统一服务校验；未来 Agent 不得写数据库。账号的 home_timezone、daily_review_time、daily_review_enabled、stale_after_days 和 revision 通过 settings 实体同步；通知隐私、热键、当前焦点只留本机。daily_review_time 是当地时刻，默认未启用通知，且与事项明确提醒分别登记。

## 6. 示例与错误

```json
{
  "operation_id": "6cbaae24-0156-4608-9490-e628ea578b40",
  "base_revision": 7,
  "action": "wait",
  "payload": {
    "waiting_for": "Agent 完成修改",
    "review_at": "2026-10-13T08:00:00Z",
    "next_action": "运行测试，确认后再关闭事项"
  }
}
```

```json
{
  "error": {"code": "REVISION_CONFLICT", "message": "事项已有新版本"},
  "operation_id": "6cbaae24-0156-4608-9490-e628ea578b40",
  "current_revision": 8,
  "request_id": "request-opaque-id"
}
```

| HTTP | code | 客户端动作 |
|---|---|---|
| 401 | UNAUTHORIZED | 停止网络提交，保留本地内容，重新授权 |
| 403 | SCOPE_DENIED | 保留操作并显示权限不足 |
| 404 | NOT_FOUND | 不泄露其他 owner 的资源存在性 |
| 409 | REVISION_CONFLICT / STALE_SCHEDULE | 获取最新版本，保留本地副本；不得无提示重放旧提醒 |
| 409 | IDEMPOTENCY_KEY_REUSED | 同 operation_id 内容不同；标记客户端错误，不重试修改后的内容 |
| 410 | SYNC_RESET_REQUIRED / ITEM_PURGED | 重拉快照或阻止复活；本地稿件可另存新 ID |
| 413 | ATTACHMENT_TOO_LARGE | 内容保留本地，提示限制 |
| 422 | INVALID_STATE / INVALID_TIME / INVALID_CONTENT / HASH_MISMATCH | 需要修正，不能无限自动重试 |
| 429/5xx | RATE_LIMITED / TEMPORARILY_UNAVAILABLE | 按 Retry-After 或有上限指数退避重试 |

成功写入通过 `X-Operation-ID`、`X-Sync-Epoch` 响应头回传操作标识和当前代际；Item 响应体另含权威实体、版本和 `sync_epoch`。所有已经授权的写请求携带 `X-Sync-Epoch`，不匹配即返回 410，避免检查后服务端恰好恢复的竞态。写入响应中的变更位置只用于诊断，客户端不能直接把 pull cursor 跳到那里，否则可能跳过其他设备的变更。

## 7. 身份与幂等

设备令牌是独立、可撤销的随机凭据，macOS 放 Keychain；服务端只存哈希。范围包括 items:read、items:write、progress:append、reminders:write、items:close、attachments:read、attachments:write、devices:manage、account:manage、exports:create。复合动作必须同时满足涉及的权限；wait 携 notification 需要事项和提醒写权限；导出图片还需 attachments:read。Mac 的常规授权可包含全部自用范围，未来 Agent 单独授权。

幂等键按 owner 全局唯一，request_hash 包含规范化方法、路径、body 与 actor 身份。认证并检查当前权限后，在用户串行事务中先查 receipt，再检查 base_revision。相同 actor、相同请求返回原结果；不同请求拒绝。已撤销设备不得借 receipt 重新读取旧结果。错误结果也可以记录；遇到版本冲突后用户重新作出的操作必须生成新 operation_id。

operation_receipts 的最小去重记录和 ItemIdentity 首版不自动清理。若将来压缩，必须定义最大离线期、客户端重建和操作作废协议，不能只用“保留 30 天”后继续无限重试。
