# Mac 客户端架构

## 1. 技术与模块

建议最低 macOS 14，SwiftUI 提供内容视图，AppKit 管理 NSStatusItem、NSPanel、键盘和焦点；SQLite 保存结构化数据，本地文件保存图片。首版使用签名、公证安装包；App Store 与沙箱发布模式后续另行验证，不把开发环境权限结果当成分发环境结果。

| 模块 / 未来路径 | 职责 | 依赖方向 |
|---|---|---|
| App/StatusItemController.swift | 菜单栏图标与数量 | ViewModel |
| App/PanelCoordinator.swift | 打开、关闭、固定、扩大、返回原应用 | DraftStore、AppKit |
| Capture/CaptureCoordinator.swift | 汇总文字、截图、粘贴、拖放到统一保存命令 | ItemStore、FileStore |
| Capture/ScreenshotService.swift | 权限、区域、单帧获取、编码 | ScreenCaptureKit、ImageIO |
| Capture/ClipboardImporter.swift | 用户触发时读取、类型选择、防抖 | NSPasteboard |
| Domain/ItemReducer.swift | 本地领域校验与乐观投影 | 纯 Swift，禁止依赖视图 |
| Persistence/Database.swift | SQLite 事务、迁移、FTS/字面搜索 | SQLite |
| Persistence/AttachmentStore.swift | 文件持久化、缩略图、校验与恢复 | Foundation、ImageIO |
| Sync/SyncEngine.swift | Outbox、影子版本、拉取、冲突、快照 | APIClient、数据库 |
| Notifications/NotificationReconciler.swift | 目标通知集合与系统登记差集 | UserNotifications |
| Security/CredentialStore.swift | 设备令牌、所属设备本地引用 | Keychain |

目录均相对于未来 `apps/macos/Sources/`。异步业务用 actor 隔离数据库写入与同步队列，UI 更新在 MainActor；图片编码、哈希、网络和全文检索不在主线程执行。首版不引入 Electron/Tauri 或内嵌 Web 界面。

## 2. 本地数据库与文件布局

```text
Application Support/AIMemorandum/
  memorandum.sqlite
  attachments/<attachment_id>/original.png
  attachments/<attachment_id>/thumbnail.jpg
  staging/<capture_session_id>/manifest.json
  staging/<capture_session_id>/image.tmp
  recovery/                       # 冲突/迁移修复材料，按显式操作清理
```

SQLite 表：server_items、local_projection、attachments、local_files、drafts、outbox、operation_results、conflicts、sync_state、local_notification_state、review_state。草稿可有附件但不是 Item；不能在同步时自动把草稿上传。

启用 WAL、外键与合理 busy_timeout；“保存到本机”的事务使用 `synchronous=FULL`，要实测写入耗时，不能为了性能改成异步内存确认。设置 schema_version，迁移前备份数据库并校验；失败保持旧库可打开，不能清空重建。WAL 运行中的导出/备份用 SQLite backup API，不只拷贝主 db 文件。

### 图片保存的崩溃一致性

1. 为捕获生成固定 capture_session_id、Item ID、Attachment ID、operation_id，在 staging 写 manifest。
2. 写图片临时文件，校验格式/大小/哈希，flush，按平台支持执行持久化同步，原子 rename 到正式路径；同步目录元数据的支持情况须实测。
3. 一个 SQLite 事务写 Item 投影、附件引用和 Outbox，使用 capture_session_id 唯一约束防重。
4. 事务完成后才显示成功，清理 staging manifest。
5. 重启扫描 manifest：已有对应 Item 则清理残留；文件完整但事务未提交则恢复为待确认记录或完成同一 capture_session 的本地提交，不能新建第二条；文件不完整则显示恢复失败，保留可诊断信息。

本地删除不同步立即 unlink。保留附件到回收站到期且无 Outbox、冲突、草稿引用后再清理。首次版本不自动驱逐已下载原图，以保证离线查看；存储设置显示占用，后续才引入可控缓存淘汰。

## 3. 截图实现与权限

ScreenCaptureKit 的 `SCScreenshotManager` 提供单帧抓取；显示器区域使用 `SCStreamConfiguration.sourceRect`，窗口选择使用 `SCContentSharingPicker` 返回的过滤器。[单帧截图](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager)、[sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)

区域路径：用户主动触发 → 检查系统许可 → 当前屏幕透明区域选择层 → 获取用户矩形 → 隐藏自身面板和选区层并等待窗口变化完成 → 把 AppKit 全局点坐标转换为目标显示器局部坐标 → 配置 sourceRect 与实际像素输出尺寸 → 抓取 PNG。禁止先后台持续录屏再挑一帧。

AppKit 与屏幕图像的坐标原点、缩放和显示器布局不同；封装 CoordinateMapper，用真实截图验证 Retina/非 Retina、负坐标副屏、顶部菜单栏、全屏 Space 和缩放切换。首版不跨显示器拼图，避免缩放混合错误。不能只乘一个全局 scaleFactor。

区域交互属于选区层；窗口选择器采用系统提供的 picker，不复制其共享授权界面。权限策略和系统提示以目标系统实际表现为准；不承诺永久无再次授权提示，不依赖废弃截图 API 或私有 API。获取失败保持剪贴板导入可用。

截图默认不读取前台浏览器 URL、文档内容或窗口标题；如果用户粘贴了链接则保存该链接。截图附带捕获时间和 input_method 即可。首版不做 OCR；未来本机 Vision OCR 是异步派生数据，需独立开关，失败不影响原图，不改变下一步或日期。[Vision 文本识别](https://developer.apple.com/documentation/vision/vnrecognizetextrequest)

## 4. 窗口与输入

NSStatusItem 仅控制菜单栏外壳，NSPanel 内嵌 SwiftUI 视图；快捷键注册使用可明确报告冲突的系统机制，Phase 0 验证布局与系统保留键。不要依靠辅助功能全局事件监听来实现普通全局热键。[NSStatusItem](https://developer.apple.com/documentation/appkit/nsstatusitem)

输入用可桥接 NSTextView 的编辑组件，通过 `hasMarkedText` 和 composing 状态屏蔽提交；不要只在 SwiftUI onSubmit 处理 Enter。关闭前同步保存草稿，日常输入以不超过 200ms 的防抖保存；崩溃可能丢最后一小段尚未落盘草稿，已明确保存的 Item 不应丢失。失焦不能清空草稿。

PanelCoordinator 显式维护 transient/pinned/expanded/child_dialog/capturing 状态。日期控件、文件选择和输入法候选不是“用户放弃”。快捷保存成功之后恢复原应用，不对第三个新焦点应用强行 activate。

登录启动使用 `SMAppService`，用户开启后按实际授权状态展示；启动只恢复图标、数据库、同步和通知校正。[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)

## 5. 搜索

首版 10,000 条事项以确定性字面子串搜索为正确性基线：对标题、正文、next_action、resume_note、waiting_for、附件 caption 构建规范化检索列，大小写折叠与 Unicode NFC；保留标点，不按英文分词要求用户输入。数据库绑定参数，LIKE 的 `%`、`_` 和转义符按字面处理。

较长查询可增加 FTS5 trigram 加速；1–2 个汉字与代码标点使用参数化子串检索兜底。FTS5 trigram 的全文查询对少于 3 个 Unicode 字符的子串存在限制，不能声称开启 FTS 就覆盖中文短词。[SQLite FTS5](https://www.sqlite.org/fts5.html)

首版不要求强制绑定新版系统 SQLite；验证目标系统实际编译特性，缺少 trigram 时保留正确性路径。分页返回 50 条，异步检索取消过时请求；不允许先截断候选到前 100 条再搜索，造成漏结果。

## 6. 测试与项目验收

以[总验收表](../00_shared/03_performance_and_acceptance.md)为准。Mac 必须通过 AC-CAP-001 至 AC-CAP-007、AC-MAC-001、AC-NOT-001 至 AC-NOT-004、AC-PERF-001。关键实机项是中文输入法、屏幕授权、截图坐标、系统通知、退出/唤醒和快捷键冲突；HTML 示意不能替代这些验证。

Swift 测试设计见[测试设计](../00_shared/06_test_design.md)。性能测量用 signpost/Instruments，统计快捷键回调至输入可用、保存动作至 SQLite commit 和收条显示，不用“服务端请求耗时”代替本机交互延迟。
