import Foundation
import MemorandumCore

final class CoreTests {
    var cleanups: [() -> Void] = []
    func addTeardownBlock(_ callback: @escaping () -> Void) { cleanups.append(callback) }
    deinit { cleanups.forEach { $0() } }
    func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func test_AC_CAP_001_OfflineCaptureSurvivesReopen() throws {
        let root = try directory()
        var store: LocalStore? = try LocalStore(root: root)
        let item = try store!.createText("确认服务器续费价格")
        let operation = try XCTUnwrap(store!.pending().first)
        store = nil
        let reopened = try LocalStore(root: root)
        try XCTAssertEqual(try reopened.items().first?.id, item.id)
        try XCTAssertEqual(try reopened.pending().first?.id, operation.id)
        try XCTAssertEqual(try reopened.items().first?.bodyMd, "确认服务器续费价格")
    }

    func test_AC_NOT_001_CompleteCancelsLocalReminderWithoutChangingDeadline() throws {
        let store = try LocalStore(root: directory())
        let item = try store.createText("检查测试结果")
        let when = Date().addingTimeInterval(600)
        try store.setReminder(item.id, at: when)
        try XCTAssertEqual(try store.item(item.id)?.kind, "task")
        try XCTAssertEqual(try store.item(item.id)?.activeReminders.count, 1)
        try store.complete(item.id)
        try XCTAssertEqual(try store.item(item.id)?.taskState, "completed")
        try XCTAssertEqual(try store.item(item.id)?.activeReminders.count, 0)
        try XCTAssertEqual(try store.pending().count, 3)
    }

    func test_AC_SYNC_002_RemoteVersionDoesNotOverwritePendingLocalText() throws {
        let store = try LocalStore(root: directory())
        let local = try store.createText("本机未确认的正文")
        var remote = local
        remote.bodyMd = "另一个设备的正文"
        remote.revision = 2
        try store.applyRemote(remote)
        try XCTAssertEqual(try store.item(local.id)?.bodyMd, "本机未确认的正文")
        try XCTAssertEqual(try store.shadow(local.id)?.bodyMd, "另一个设备的正文")
    }

    func test_AC_CAP_003_CompositionDoesNotSubmit() throws {
        try XCTAssertFalse(InputPolicy.shouldSubmit(keyCode: 36, shift: false, marked: true))
        try XCTAssertFalse(InputPolicy.shouldSubmit(keyCode: 36, shift: true, marked: false))
        try XCTAssertTrue(InputPolicy.shouldSubmit(keyCode: 36, shift: false, marked: false))
    }

    func test_AC_CAP_004_OCRTextStoredAsBodyWithAutoTitle() throws {
        let store = try LocalStore(root: directory())
        let item = try store.createImages([tinyPNG()], method: "screenshot", body: "发票金额 128.50 元\n税率 13%")
        let saved = try XCTUnwrap(store.item(item.id))
        try XCTAssertEqual(saved.bodyMd, "发票金额 128.50 元\n税率 13%")
        try XCTAssertEqual(saved.title.hasPrefix("截图 · "), true)
        try XCTAssertEqual(saved.titleOrigin, "generated")
    }

    func test_AC_DOM_003_DeleteStopsRemindersAndQueuesAction() throws {
        let store = try LocalStore(root: directory())
        let item = try store.createText("要删掉的记录")
        try store.setReminder(item.id, at: Date().addingTimeInterval(3600))
        try store.deleteItem(item.id)
        let deleted = try XCTUnwrap(store.item(item.id))
        try XCTAssertEqual(deleted.deletedAt != nil, true)
        try XCTAssertEqual(deleted.activeReminders.count, 0)
        try XCTAssertEqual(try store.pending().last?.body.contains("\"delete\""), true)
    }

    func test_AC_DOM_004_UpdateQueuesPutOperation() throws {
        let store = try LocalStore(root: directory())
        let item = try store.createText("原始内容")
        // 默认全部存为待办
        try XCTAssertEqual(item.kind, "task")
        try XCTAssertEqual(item.taskState, "todo")
        try store.update(item.id, title: "改后的标题", body: "改后的正文")
        let saved = try XCTUnwrap(store.item(item.id))
        try XCTAssertEqual(saved.title, "改后的标题")
        try XCTAssertEqual(saved.bodyMd, "改后的正文")
        try XCTAssertEqual(saved.titleOrigin, "user")
        let op = try XCTUnwrap(store.pending().last)
        try XCTAssertEqual(op.path, "/api/v1/items/\(item.id)")
        try XCTAssertTrue(op.body.contains("改后的标题"))
        // 标题为空时回退用正文做标题
        let second = try store.createText("第二条的原文")
        try store.update(second.id, title: "  ", body: "只有正文")
        let saved2 = try XCTUnwrap(store.item(second.id))
        try XCTAssertEqual(saved2.title, "只有正文")
        try XCTAssertEqual(saved2.bodyMd, "只有正文")
        // 标题与正文全空应拒绝
        var rejected = false
        do { try store.update(second.id, title: " ", body: "  ") } catch { rejected = true }
        try XCTAssertTrue(rejected)
    }
}
