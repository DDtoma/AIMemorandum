# AIMemorandum 术语表（GLOSSARY）

单一上下文（single-context）：全仓库共用本表；ADR 见 docs/adr/。
裁决原则：**术语与语义以代码实现为准**（ADR-001，2026-10-27 维护者裁决）。AM-001 设计文档含未实现功能，旧词以本表「禁用旧词」为准矫正，历史文档正文暂不逐字回改；新文档与代码注释必须使用本表规范词。

状态标记：**已实现** = 代码现行事实；**未实现** = 仅设计文档存在，代码没有；**兼容保留** = 枚举/字段仍在但客户端不产生；**已过时** = 曾被文档或契约使用，现已作废。

## 1. 核心实体

### 记录 / Item（已实现）
一条备忘录实体：文字和/或图片附件，带状态、提醒、附件与同步元数据。
代码：Swift `Item`（apps/macos/Sources/MemorandumCore/Models.swift:26）；Go `Item`（services/api/internal/domain/domain.go:37）；openapi `Item`/`ItemInput`。
UI 对应「我的记录」（ItemsWindow.swift:368）。作名词时「记录」专指 Item；作动词仅出现在菜单功能名（见 §3）。
禁用旧词：事项、条目。

### 类型 / kind（已实现 + 兼容保留）
枚举 `capture / task / memo`（Go domain.go:60）。
**当前产品语义：一切保存皆为待办（task）**，面板文字、剪贴板文字、截图、剪贴板图片、拖入图片全部存为 task（CapturePanel.swift:172-183；App.swift:203；LocalStore.swift:91）。
- `task` = 待办：当前唯一活跃类型。
- `capture` = 随手记：**兼容保留**，仅 Item.fresh(task:false) 默认值路径（Models.swift:40-43），现行 UI 不产生；历史/兼容数据的徽标若启用叫「随手记」，不得再叫「记录」。
- `memo` = 普通备忘：**兼容保留**，客户端从不产生，仅服务端接受。
AM-001 的「三类型并存」设计标记为已变更。

### 待办状态 / task_state（已实现）
`todo` 待处理 / `in_progress` 进行中 / `waiting` 等待中 / `paused` 已暂停 / `completed` 已完成 / `canceled` 已取消。
已知偏差：`paused` 仅 Go 端支持，Swift UI 无对应徽标分支（ItemsWindow.swift:616-626 落兜底）。
注意：UI 徽标「待办」特指 `todo`（ItemsWindow.swift:619），与类型「待办」不是同一层含义。

### 菜单「待办（N）」口径（已实现）
未结束且 kind=task 的记录，**包含** in_progress / waiting，点击即完成（App.swift:128-132）。词表中「待办列表」默认指此口径。

### 已结束 / ended（已实现）
`task_state ∈ {completed, canceled}` 或 `deleted_at ≠ nil`；不再进入待办列表、不可再完成或提醒（Swift Models.swift:32）。
已知偏差：Go 端把 deleted 单独先校验，ended 变量只含 completed/canceled（domain.go:83-84），实现路径不同，词表定义以本条为准。

### 重开 / reopen（已实现）
仅 `canceled` 可重开为 `todo`（LocalStore.swift:147）；UI 按钮文案「重新打开」（ItemsWindow.swift:531）。
禁用旧说法：「重开 completed 或 canceled」（RUN.md 旧表述）。

### 附件 / Attachment（已实现）
`state = pending / ready / failed / deleted`；pending=待上传，与 outbox 的「待推送」是两回事。
上限：每条 10 张、单张 20MiB、40MP、总量 50MiB（LocalStore.swift:80-88 = server.go:112-116 = domain.go:66-73）。

### 来源 / source.input_method（已实现）
`text / screenshot / clipboard / drop / api`；现行客户端用前四个。

### 提醒 / Reminder（已实现）
一条提醒规则（数据）：`remind_at + enabled + schedule_revision`（Models.swift:21-22；Go domain.go:33）。`schedule_revision` 是 UUID 字符串，用于乐观校验，不是递增版本号。
**「提醒」单独使用时默认指此规则。**
禁用旧词：ReminderRule、reminder_rules（openapi/文档）；ReminderOccurrence、NotificationDelivery 为**未实现**设计。

### 通知 / notification（已实现）
提醒到期后的弹窗事件；当前为自绘弹窗（Notifier.swift），**不是** macOS UserNotifications 系统通知。

### 延后提醒 / snooze（已实现）
修改**同一条**提醒规则的时间（通知弹窗「稍后10分钟」= snooze 600 秒，Notifier.swift:57,110）。
区别于「+2小时提醒」= `set_reminder` **新增**规则（ItemsWindow.swift:329,600-602）。两者不得再统称「稍后提醒」。

## 2. 同步与冲突

### 同步 / sync（已实现）
一轮 = `pushPending → uploadAttachments → pullChanges`（SyncEngine.swift:16-20）。
「已同步」徽标 = 无待推送操作且附件全部 ready（LocalStore.swift:226），不代表「远端已确认所有语义」。

### 待推送队列 / outbox（已实现）
本地 `PendingOperation`（Models.swift:46）+ outbox 表（LocalStore.swift:22）；`op.state = queued / acknowledged / conflict / invalid`。服务端对应 ops 表（store.go:64）。

### 动作 / action（已实现，集合以代码为准）
现行 8 个：`complete / convert / capture_and_remind / set_reminder / snooze / remove_reminder / reopen / delete`（domain.go:81）。
AM-001 文档的 20 个动作中其余 12 个一律标**未实现**。

### 影子版本 / shadow（已实现）
本地保存的最近已知远端版本，用于冲突检测（LocalStore.swift:21,56）。
禁用旧词：server_shadow、local_projection、远端投影。

### 冲突副本 / conflict（已实现）
本地与远端冲突时保留的本地副本，等待人工裁决；409 `REVISION_CONFLICT`（store.go:186）；conflicts 表（LocalStore.swift:24）。
唯一实现的裁决动作：`use_remote` = 用远端版本覆盖本地（LocalStore.swift:225）；其余裁决动作为**未实现**。
禁用旧说法：取本地取远端、以远端为准、接受远端、采用远端等混用表述。

### 游标 / cursor（已实现）
客户端存 `meta("cursor")`（SyncEngine.swift:93），服务端为 `events.seq`（store.go:61）；拉取响应为 `{cursor, items}`（server.go:341）。
openapi 的 `SyncPage{sync_epoch, changes, next_cursor, has_more}` 为**未实现**。

### 修订号 / revision（已实现）
记录的单调递增版本号。必须与 `schedule_revision`（UUID）、`sync_epoch`（仅设计）区分，三者不可混称「版本」。

## 3. 保存与界面

- **快速记录 / 截图记录 / 剪贴板速存 / 查看记录**（已实现）：菜单功能名（App.swift:117-120）。统一词序「剪贴板速存」，禁用「速存剪贴板」。
- **截图 / screenshot**（已实现）：区域截图功能。**「屏幕录制」仅是 macOS 权限名**（Screenshot.swift:7,28），不是功能名，也不是本产品行为描述；AM-002 的 Linux ScreenCast portal 属另一概念，不入本表。
- **OCR 文字提取**（已实现）：本机 Vision 提取图片文字写入正文供检索（Screenshot.swift:86；LocalStore.swift:92）。AM-001 ADR-010「首版不做 OCR」为**已过时**决策。
- **收条 / toast**（已实现）：保存后的小浮条，如「已存为待办 ✓」（CapturePanel.swift:101,219）。
- **输入面板 / CapturePanel**、**记录窗口 / ItemsWindow**、**状态栏菜单 / status item**：UI 组件名与代码类型对应。
- **托盘菜单 / tray menu**（已实现）：Windows 通知区域与 Linux 状态栏菜单，沿用「快速记录 / 截图记录 / 剪贴板速存 / 查看记录」四个规范功能名，完成项以「完成 · {标题}」在前；代码：apps/desktop/ui/src/model.rs、apps/desktop/windows/src/tray.rs、apps/desktop/linux/src/tray_service.rs。
- **Frame Timer**（已实现）：桌面外壳的周期事件泵，处理界面更新、提醒、托盘、全局快捷键与单实例命令；代码：apps/desktop/ui/src/slint_shell.rs、apps/desktop/windows/src/main.rs。
- **Slint**（已实现）：Windows/Linux 共享桌面界面的 UI 工具包；代码：apps/desktop/ui/ui/。

## 4. 删除与回收

- **软删除 / soft delete**（已实现）：写 `deleted_at`，UI 直接隐藏（ItemsWindow.swift:498-499）。
- **回收站 / trash**：当前**仅软删除语义**；回收站界面、restore 动作、30 天保留期均为**未实现**（RUN.md:74 已自认）。提到「回收站」不得暗示已有界面或可恢复。

## 5. 设备与接口契约

- **注册码 / enrollment_code**（已实现）：服务端启动时打印，UI 输入框「注册码」（App.swift:317；Go EnrollCode，server.go:21），用于换取设备凭据。
- **设备凭据 / device_token**（已实现）：注册后下发，前缀 `dvc_`（server.go:104）。openapi 的 `access_token` 为**已过时**契约用词。
- **PUT /items/{id}**（已实现）：更新记录（server.go:32,211）。文档的 PATCH 为**已过时**。
- **错误信封 error.name**（已实现）：错误码字段叫 `name`（server.go:44,51；domain.go:13）。文档的 `error.code` 为**已过时**。
- **captured_at**：**实现未保留**——Go 输入结构有该字段但转换时丢弃（domain.go:49,51-56），Swift 模型无此字段。捕获时间以 `created_at` 为准，文档不得再把 captured_at 当独立事实字段。

## 6. 禁用旧词速查

| 禁用旧词 | 规范词 |
|---|---|
| 事项、条目 | 记录（Item） |
| ReminderRule、reminder_rules | 提醒（Reminder） |
| access_token | device_token |
| error.code | error.name |
| PATCH /items/{id} | PUT /items/{id} |
| SyncPage、sync_epoch（现行同步语义下） | {cursor, items} |
| server_shadow、local_projection、远端投影 | 影子版本（shadow） |
| 速存剪贴板 | 剪贴板速存 |
| 稍后提醒（笼统） | 延后提醒（snooze）/ 新增提醒（set_reminder） |
| 用「屏幕录制」指功能 | 截图；屏幕录制仅指权限 |
| 用「记录」指 kind=capture 徽标 | 随手记（兼容保留） |

## 7. 范围外

AM-002 的 Linux 专有术语（ScreenCast/RemoteDesktop portal、PortalHotkeys 等）暂不入表，见其方案文档 docs/proposals/AM-002-desktop-port/README.md；Linux 落地时再并入。
