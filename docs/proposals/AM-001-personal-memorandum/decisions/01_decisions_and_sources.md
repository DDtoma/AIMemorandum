# 决策、依据与待验证项

查询日期：2026-10-13。以下将外部 API 能力与本产品的设计选择分开；官方资料不等于本产品已经验证成功。

## 1. 主要决策

| ID | 选择 | 原因 / 代价 |
|---|---|---|
| ADR-001 | 原生 Mac，SwiftUI 内容 + AppKit 外壳 | 热键、焦点、菜单栏是主要交互；其他平台以后各做客户端 |
| ADR-002 | 截图与 PNG/JPEG 纳入首版 | 回应多样、快捷记录要求；增加附件持久化、同步、备份工作量 |
| ADR-003 | 截图松手即存、速存不弹完整编辑器 | 减少必须的操作；误操作通过收条撤销与回收站处理 |
| ADR-004 | 默认 capture，明确整理后才变 memo | 纯图片也有回顾归属；不能把没整理当作无需处理 |
| ADR-005 | 本地优先、影子版本 + Outbox | 断网不阻塞；实现复杂度高于直接远程 CRUD |
| ADR-006 | 同 Item 首版保守版本冲突 | 不静默丢正文；同时编辑不同字段也可能需要用户处理 |
| ADR-007 | per-owner 行锁与提交序号 | 个人规模简单可靠；不适合无限增长的高写入租户，需实际瓶颈再演进 |
| ADR-008 | 单体 Go + PostgreSQL + 文件卷 | 保持部署单元简单；图片一致备份与 GC 必须专门设计 |
| ADR-009 | 服务端保存提醒状态，Mac 展示系统通知 | 无重复发送渠道；其他设备离线时旧通知无法立即撤销 |
| ADR-010 | OCR 后置、首版无模型调用 | 保证先保存图；搜索首版只覆盖已有文字与手写说明 |
| ADR-011 | 暂定 macOS 14+ | 使用现代单帧截图 API；仍需最低版本实机与 SDK availability 检查 |
| ADR-012 | 不自动获取选中文本/浏览器上下文 | 避免为了少一步引入广泛辅助功能权限；复制后速存作为首版路径 |

## 2. 官方依据

| 资料 | 可支持的结论 | 不能由此推断 |
|---|---|---|
| [Apple：ScreenCaptureKit 新功能，WWDC23](https://developer.apple.com/videos/play/wwdc2023/10136/) | 可单帧截图；可用系统 picker 获取过滤器 | 系统 picker 等于任意区域选择器；所有权限提示永久只出现一次 |
| [SCScreenshotManager](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager) | 提供截图 API | 本产品已经适配所有系统/硬件 |
| [SCStreamConfiguration.sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect) | 指定捕获区域；单窗口捕获不使用该区域规则 | AppKit 全局矩形可不转换直接传入 |
| [SCContentSharingPicker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker) | 系统内容选择器 | 自制共享选择器具有相同授权行为 |
| [NSStatusItem](https://developer.apple.com/documentation/appkit/nsstatusitem) | 菜单栏项目接口 | SwiftUI 单一场景可以自动处理全部焦点细节 |
| [SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice) | 登录项/服务管理能力 | 应用可以绕过用户授权 |
| [安排本地通知](https://developer.apple.com/documentation/usernotifications/scheduling-a-notification-locally-from-your-app) | 请求由 UserNotifications 管理 | 用户一定看到、权限与专注模式不起作用 |
| [取消未投递请求](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/removependingnotificationrequests(withidentifiers:)) | 可按 ID 撤销 pending 请求 | 可以撤销用户已经看见的内容 |
| [Mac 通知设置](https://support.apple.com/guide/mac-help/notifications-settings-mh40583/mac) | 用户可控制通知呈现等设置 | 应用可保证准时弹窗 |
| [Vision 文本识别](https://developer.apple.com/documentation/vision/vnrecognizetextrequest) | 可提取图中文字，语言支持需运行时检查 | OCR 永远准确，或可以替代用户确认下一步 |
| [PostgreSQL SELECT](https://www.postgresql.org/docs/current/sql-select.html) | SKIP LOCKED 可用于队列类领取场景 | 单用序列即可保证事务提交顺序 |
| [SQLite FTS5](https://www.sqlite.org/fts5.html) | trigram 的短子串限制等搜索行为 | 单独 FTS 就覆盖所有中文短词和代码标点 |

用户原方案引用的旧版 Apple Local and Remote Notification Programming Guide 含历史平台说明；实施与兼容性判断应优先使用当前 UserNotifications 文档和目标 macOS 实机结果，避免直接复制旧指南中的平台断言。

## 3. 假设与决策时机

| 未确定内容 | 当前处理 | 何时确认 |
|---|---|---|
| 使用者 Mac 型号、系统版本 | macOS 14+ 作为设计基线；性能机为建议 | Phase 0 |
| Developer ID、团队和分发渠道 | 先设计签名公证安装包，不虚构可用证书 | Phase 0 |
| 服务端域名、部署主机、备份位置 | 只给部署规格与步骤 | 开始部署前 |
| Go/PostgreSQL/Xcode 固定版本 | 实施时选受维护版本并记录，不写“永远最新” | 工程初始化 |
| 用户是否需要语音录音/自动转写 | 首版只有系统听写输入；音频附件后置 | 下一版范围讨论 |
| 既有 Agent 的接口与身份体系 | 没有读取该代码，只有外部引用与 scopes 预留 | 真正接入前 |

这些未知项不妨碍本次设计落文件，也不要求用户现在完成一轮基础设施配置。

## 4. 兼容演进与影响

接口新增可选字段、附件 MIME 白名单和能力列表优先增量变化；未知枚举只读保留。状态、时间、游标和幂等变更必须升级契约与跨语言样本。服务端恢复产生新 epoch 属于明确重同步边界。添加 OCR、分享扩展或 Agent 时依次更新入口、领域动作、存储、授权、变更日志、导出备份与验收，不能只加按钮。

首版增加图片后，影响跨越 Capture → LocalFiles → Outbox → API → BlobStore → Backup；每一段都有崩溃点和显示状态。它增加开发成本，但符合本次“少步骤、多方式记录”的核心要求。图片限制与后置功能已经在产品设计、接口和计划中同步。
