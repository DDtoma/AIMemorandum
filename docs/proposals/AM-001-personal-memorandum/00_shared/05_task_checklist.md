# 开发任务清单

下列为原设计的全量开发任务；已有实现尚未逐项复核勾选。本次截图问题修复记在 T029。AC 定义见[性能与验收](03_performance_and_acceptance.md)。依赖列指先完成的任务；每项以测试与可运行证据结束。

## Phase 0

- [ ] T001：建立 `apps/macos/`、`services/api/`、`infra/`；固定工具链、构建和CI。覆盖 AC-API-001，依赖无。
- [ ] T002：`apps/macos/Sources/App/HotkeyManager.swift`、`PanelCoordinator.swift`；热键、IME、焦点实机试验。覆盖 AC-CAP-003、AC-MAC-001，依赖 T001。
- [ ] T003：`apps/macos/Sources/Capture/ScreenshotService.swift`；权限、单帧、Retina/副屏矩形映射。覆盖 AC-CAP-002、AC-CAP-006、AC-MAC-001，依赖 T001。
- [ ] T004：`apps/macos/Sources/Notifications/NotificationProbe.swift`；签名包内实测退出、权限、取消、容量。覆盖 AC-NOT-003/004，依赖 T001。
- [ ] T005：`contracts/openapi.json`、`contracts/fixtures/attention.json`；固定动作、枚举、时间和限额。覆盖 AC-API-001、AC-NOT-004，依赖 T002/T003/T004。

## Phase 1

- [ ] T006：`apps/macos/Sources/Persistence/Database.swift`、`Migrations/`；WAL、迁移、投影和 Outbox 原子保存。覆盖 AC-CAP-001，依赖 T005。
- [ ] T007：`apps/macos/Sources/Persistence/DraftStore.swift`、`Capture/TextEditor.swift`；草稿与 marked text。覆盖 AC-CAP-003，依赖 T006。
- [ ] T008：`apps/macos/Sources/Persistence/AttachmentStore.swift`、`RecoveryScanner.swift`；staging manifest、原图、哈希、崩溃恢复。覆盖 AC-CAP-005/006，依赖 T003/T006。
- [ ] T009：`apps/macos/Sources/Capture/CaptureCoordinator.swift`；文字与截图共用保存事务、收条、撤销。覆盖 AC-CAP-001/002，依赖 T007/T008。
- [ ] T010：`apps/macos/Sources/Capture/ClipboardImporter.swift`、`DropTarget.swift`；速存、类型择一、2秒防抖、多图、限制。覆盖 AC-CAP-004/007，依赖 T009。

## Phase 2

- [ ] T011：`services/api/internal/auth/`、`apps/macos/Sources/Security/CredentialStore.swift`；单次注册、设备 scope、撤销、Keychain。覆盖 AC-AUTH-001，依赖 T005。
- [ ] T012：`services/api/migrations/001_domain.sql`、`internal/items/`；模型、动作、事件、ItemIdentity。覆盖 AC-DOM-001、AC-API-001，依赖 T011。
- [ ] T013：`services/api/internal/sync/transaction.go`、`receipt.go`；用户锁、幂等、提交序号。覆盖 AC-SYNC-001/003，依赖 T012。
- [ ] T014：`services/api/internal/sync/pull.go`、`snapshot.go`、`sse.go`；固定快照、游标过期和提示流。覆盖 AC-SYNC-003/004，依赖 T013。
- [ ] T015：`apps/macos/Sources/Sync/SyncEngine.swift`、`Outbox.swift`；依赖队列、影子版本、原子拉取游标、重试。覆盖 AC-SYNC-001/003，依赖 T009/T014。
- [ ] T016：`services/api/internal/attachments/`、`apps/macos/Sources/Sync/AttachmentTransfer.swift`；图片流、ready、校验、下载与 GC。覆盖 AC-ATT-001/002，依赖 T008/T013。
- [ ] T017：`apps/macos/Sources/Sync/ConflictStore.swift`、`Views/ConflictView.swift`；本地副本、最新版本、删除冲突与另存。覆盖 AC-SYNC-002/004、AC-NOT-002，依赖 T015/T016。

## Phase 3

- [ ] T018：`apps/macos/Sources/Domain/ItemReducer.swift`、`Views/ItemDetail.swift`；等待/收到结果/进展/焦点、类型转换。覆盖 AC-DOM-001/003，依赖 T012/T015。
- [ ] T019：`services/api/internal/items/review.go`、`apps/macos/Sources/Domain/ReviewPolicy.swift`；回顾、总量、稳定轮换与不重置进展。覆盖 AC-DOM-002，依赖 T018。
- [ ] T020：`services/api/internal/reminders/`、`background_jobs/`；未来 occurrence、失效、租约与失败重试。覆盖 AC-NOT-001/002/003，依赖 T013/T018。
- [ ] T021：`apps/macos/Sources/Notifications/NotificationReconciler.swift`；稳定 ID、差集、旧通知动作校验、遗漏汇总。覆盖 AC-NOT-001 至 AC-NOT-004，依赖 T004/T015/T020。
- [ ] T022：`apps/macos/Sources/Search/`、`services/api/internal/items/search.go`；中文短词和代码字面检索。覆盖 AC-SEARCH-001，依赖 T006/T012。
- [ ] T023：`services/api/internal/history/`、`apps/macos/Sources/Views/HistoryView.swift`、`TrashView.swift`、`SettingsView.swift`；历史/回收站/分组/撤销/设置。覆盖 AC-DOM-003、AC-API-001，依赖 T017/T018。

## Phase 4

- [ ] T024：`services/api/internal/export/`、`apps/macos/Sources/Export/`；JSON/Markdown/图片包、缺失清单、待同步标识。覆盖 AC-OPS-001，依赖 T016/T023。
- [ ] T025：`infra/compose.yaml`、`infra/backup/`、`infra/runbooks/restore.md`；备份与附件一致性、restore epoch、实际恢复演练。覆盖 AC-OPS-001、AC-SYNC-005，依赖 T014/T016/T024。
- [ ] T026：`services/api/tests/integration/`、`apps/macos/Tests/SyncTests/`；双设备、断网、响应丢失、进程崩溃、上传/删除竞态。覆盖 AC-SYNC-001 至 AC-SYNC-005、AC-ATT-001/002，依赖 T017/T025。
- [ ] T027：`apps/macos/Tests/PerformanceTests/`、`services/api/tests/load/`；固定数据基准与原始统计。覆盖 AC-PERF-001，依赖 T019/T021/T022。
- [ ] T028：`infra/release/`、`docs/acceptance/`；签名、公证、升级回滚、真实系统行为、完整 AC 证据。重点 AC-MAC-001、AC-NOT-003、AC-OPS-001，并汇总全部 AC，依赖 T026/T027。

## 完成记录模板

每项完成后填写：实际文件、构建 SHA、对应测试命令/实机步骤、结果位置、未解决问题。不能仅勾选“已写代码”。方案本身的链接与契约结构校验不代表以上开发验收已通过。

## 试用问题修复

- [x] T029：修复 `apps/macos/Sources/AIMemorandum/Screenshot.swift` 的选区坐标与显示器匹配；`App.swift` 在截图前隐藏记录窗口、捕获期间暂缓提醒；新增 `Sources/MemorandumCore/ScreenshotGeometry.swift` 和 `Tests/MemorandumCoreTests/ScreenshotTests.swift`。覆盖 AC-CAP-002、AC-MAC-001 的相关场景：15 项回归通过，原生框选/取消时记录窗口保持隐藏。真实像素检查因当前诊断进程无录屏权限跳过，多屏实机尚未验证。详见 [2026-10-15 修复记录](../../../implementation/screenshot-fix-2026-10-15.md)。
- [x] T030：通过 `apps/macos/Bundle/Info.plist`、`Sources/AIMemorandum/AppLauncher.swift`、`scripts/build-macos-app.sh` 和 `scripts/install-macos-app.sh` 打包并安装独立 `.app`，固定 Bundle ID，避免终端启动的权限归属问题。覆盖 AC-MAC-001 的应用身份场景：签名、运行时身份、LaunchServices 和独立进程路径已验证；首次录屏授权仍由用户确认。详见 [独立应用记录](../../../implementation/standalone-app-2026-10-15.md)。
