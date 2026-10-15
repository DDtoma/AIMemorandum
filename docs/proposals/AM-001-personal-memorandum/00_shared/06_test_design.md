# 测试设计与代码样例

下面是未来测试的写法与 fixture 约定，不是当前可运行的产品测试；被测服务、数据库迁移、Swift 类型和 testkit 尚未实现。实施时先落测试夹具，再实现最小业务。本文不把示例代码写进现有源码目录。

## 1. 测试分层

| 层级 | 方式 | AC |
|---|---|---|
| 纯规则 | 固定时钟、跨语言 JSON 样本 | AC-DOM-001/002/003、AC-NOT-004、AC-API-001 |
| Mac 存储 | 临时 SQLite、真实文件、提交前后故障点 | AC-CAP-001/005/006/007 |
| Mac 输入 | marked text 单测 + 中文输入法实机 | AC-CAP-003、AC-MAC-001 |
| 服务端集成 | 真实 PostgreSQL，两个设备身份，事务屏障 | AC-SYNC-001 至 AC-SYNC-005、AC-AUTH-001 |
| 图片流 | 真实临时卷，断连、改内容、超限、删除竞态 | AC-ATT-001/002、AC-CAP-002/004 |
| 通知适配 | fake NotificationClient 验证差集 + 签名包真机 | AC-NOT-001 至 AC-NOT-004 |
| 灾难恢复 | 空数据库/空附件卷恢复，manifest 哈希核对 | AC-OPS-001 |
| 性能 | 固定种子、signpost、原始样本统计 | AC-PERF-001、AC-SEARCH-001 |

## 2. 服务端关键测试

计划 fixture API：`testkit.NewPostgresFixture(t)` 启动/连接隔离 PostgreSQL schema，提供 A/B 两个同 owner 设备、OtherOwner、固定 Clock、HTTP 方法包装和直接计数助手；包装必须走真实 HTTP/领域事务，不可在 fake map 中验证幂等。

```go
// 设计样例，目标：services/api/tests/integration/idempotency_test.go
func Test_AC_SYNC_001_ResponseLostThenRetry(t *testing.T) {
    f := testkit.NewPostgresFixture(t)
    req := f.CreateRequest("检查接口超时")
    // 故障点位于 PostgreSQL commit 之后、HTTP response 写出之前。
    f.DropNextResponseAfterCommit()
    _, err := f.A.Create(req)
    require.Error(t, err)
    for i := 0; i < 20; i++ {
        got, err := f.A.Create(req)
        require.NoError(t, err)
        require.Equal(t, req.Item.ID, got.Item.ID)
    }
    require.Equal(t, 1, f.CountItems(req.Item.ID))
    require.Equal(t, 1, f.CountCreateEvents(req.Item.ID))
    req.Item.BodyMD = "同 ID 不同内容"
    require.Equal(t, "IDEMPOTENCY_KEY_REUSED", f.A.CreateErrorCode(req))
}

func Test_AC_SYNC_002_ConcurrentEditsKeepBoth(t *testing.T) {
    f := testkit.NewPostgresFixture(t)
    item := f.CreateTask("原文")
    a := f.PatchRequest(item, "A 的正文")
    b := f.PatchRequest(item, "B 的正文")
    responses := f.SubmitConcurrently(a, b)
    require.ElementsMatch(t, []int{200, 409}, statusCodes(responses))
    require.Equal(t, item.Revision+1, f.LoadItem(item.ID).Revision)
    // Mac 集成层另断言失败方全文仍在本地 conflicts，服务端不持有那份本地稿。
}

func Test_AC_NOT_002_CompleteRejectsOldSnooze(t *testing.T) {
    f := testkit.NewPostgresFixture(t)
    item, rule := f.TaskWithReminder()
    require.NoError(t, f.A.Complete(item.ID, item.Revision))
    code := f.B.SnoozeErrorCode(item.ID, item.Revision, rule.ScheduleRevision)
    require.Equal(t, "REVISION_CONFLICT", code)
    require.Zero(t, f.CountEnabledReminders(item.ID))
    require.Equal(t, "completed", f.LoadItem(item.ID).TaskState)
}
```

游标测试不能只测 seq 单调：设置 barrier 让事务 A 取用户锁后暂停，启动 B，期间运行 sync；断言 B 尚不能分配可提交的 seq；释放 A，再确认两条变更最终各被拉到一次。每个事务开始、提交、拉取之间随机切换 100 轮，覆盖 AC-SYNC-003。

AC-AUTH-001 必须枚举 Item、Attachment、Snapshot、Export、Device、Settings 全部资源，检查跨 owner 读写；source={actor:"user"} 不能扩大 scope；已撤销令牌不能取得旧 receipt 内容。

## 3. Mac 测试样例

```swift
// 设计样例，目标：apps/macos/Tests/CaptureTests.swift
func test_AC_CAP_003_markedTextDoesNotSubmit() async throws {
    let h = try CaptureHarness.temporary()
    h.editor.setMarkedTextForTest("zhong")
    await h.press(.enter)
    XCTAssertEqual(try h.database.itemCount(), 0)
    h.editor.commitTextForTest("中")
    await h.press(.enter)
    XCTAssertEqual(try h.database.itemCount(), 1)
}

func test_AC_CAP_005_recoveryIsIdempotent() async throws {
    let h = try CaptureHarness.temporary()
    let capture = try h.pngCapture(width: 1600, height: 900)
    h.faults.failAt = .afterFileRenameBeforeDatabaseCommit
    await XCTAssertThrowsErrorAsync { try await h.capture.save(capture) }
    let relaunched = try h.relaunchFromSameDirectory()
    try await relaunched.recovery.run()
    try await relaunched.recovery.run()
    XCTAssertEqual(try relaunched.database.count(session: capture.sessionID), 1)
    XCTAssertEqual(try relaunched.database.outboxCount(session: capture.sessionID), 1)
    XCTAssertEqual(try relaunched.files.hash(capture.attachmentID), capture.sha256)
}

func test_AC_NOT_001_dismissDoesNotComplete() async throws {
    let h = try NotificationHarness.temporary()
    let item = try h.waitingTaskWithDueReminder()
    try await h.handle(.dismissed, itemID: item.id)
    XCTAssertEqual(try h.load(item.id).taskState, .waiting)
    XCTAssertTrue(try h.attentionIDs().contains(item.id))
}
```

`CaptureHarness` 需使用真实 SQLite 和临时文件，只有屏幕帧输入可用固定 PNG；`NotificationHarness` 注入固定 Clock 和 fake OS 通知中心。实机用真正的输入法执行同样操作，因为 setMarkedText 的单测无法证明系统候选窗口行为。

## 4. 共享规则样本

把这些输入展开成 `contracts/fixtures/attention.json`，Go 和 Swift 都读取，不各写一套不同期望：

| AC | 固定输入 | 期望 |
|---|---|---|
| AC-NOT-004 | due_date=2026-10-13，timezone=Asia/Shanghai，now=2026-10-13T08:00:00Z | due_today，非 overdue |
| AC-NOT-004 | 相同 due_date，now=2026-10-13T16:00:00Z | overdue |
| AC-DOM-001 | waiting、review_at=2026-10-13T08:00:00Z，now 相同 | follow_up |
| AC-DOM-002 | capture、无任何时间、未整理 | 日常回顾候选，不增加明确到期数量 |
| AC-NOT-002 | completed、有旧 due occurrence | 无关注、无有效通知安排 |
| AC-DOM-002 | 逾期 due_at，加明天 next_revisit_on | overdue 仍保留 |

## 5. 必须注入的故障点

本地：编码前、文件写一半、rename 后、SQLite commit 前、commit 后、收条前。服务端：用户锁后、Item 更新后、change_log 后、commit 后响应前、图片 rename 后 DB ready 前。网络：断线、重复响应、延迟、乱序 SSE；不能乱序应用持久化 change_log。恢复：过期 cursor、备份回退、sync_epoch 改变、损坏附件、设备被撤销。

每个故障用 AC-CAP-005、AC-SYNC-001/003/004/005 或 AC-ATT-001 对应断言：条数、版本、原始文本/哈希、Outbox 状态、游标和有效通知集合。测试成功条件不是“没有 crash”。

## 6. 实机执行清单

截图与焦点：中文拼音/五笔（若安装）、英文、系统听写，单屏/副屏、不同缩放、全屏、菜单栏自动隐藏、拒绝权限后重试。通知：允许/拒绝、专注、锁屏、退出、重启、睡眠、改时区、系统拒绝登记、旧通知交互。保留每个步骤的实际结果，不承诺模拟器能覆盖 macOS 权限。

恢复与性能：空环境恢复、两设备重连、待同步截图、10,000 条固定数据、重复 500 次热打开。实施时按[性能与验收](03_performance_and_acceptance.md)生成结果，不在本次设计交付填写“通过”。
