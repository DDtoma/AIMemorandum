import Foundation
import AppKit
import MemorandumCore

// 端到端冒烟：对真实运行的服务端执行
// 注册设备 → 文字创建 → 截图创建 → 附件上传 → 设提醒动作 → 推送 → 拉取 → 提醒判定。
// 用法：MEMO_URL=http://127.0.0.1:8787 MEMO_ENROLL=<注册码> swift run memorandum-smoke
// 已注册过设备时可用 MEMO_TOKEN=<设备凭据> 跳过注册。

func tinyPNG() -> Data {
    let image = NSImage(size: NSSize(width: 64, height: 40))
    image.lockFocus()
    NSColor.systemBlue.setFill()
    NSBezierPath(rect: NSRect(x: 0, y: 0, width: 64, height: 40)).fill()
    image.unlockFocus()
    guard let tiff = image.tiffRepresentation,
          let rep = NSBitmapImageRep(data: tiff),
          let png = rep.representation(using: .png, properties: [:]) else { fatalError("无法生成 PNG") }
    return png
}

final class AwaitBox<T> { var value: T?; var error: Error? }

func awaitSync<T>(_ body: @escaping () async throws -> T) throws -> T {
    let semaphore = DispatchSemaphore(value: 0)
    let box = AwaitBox<T>()
    Task {
        do { box.value = try await body() } catch { box.error = error }
        semaphore.signal()
    }
    _ = semaphore.wait(timeout: .now() + 15)
    if let error = box.error { throw error }
    guard let value = box.value else { throw StoreError.message("异步操作超时") }
    return value
}

struct SmokeFailure: Error { let message: String }
func XCTAssertEqual<T: Equatable>(_ actual: T, _ expected: T, file: StaticString = #filePath, line: UInt = #line) throws {
    if actual != expected { throw SmokeFailure(message: "\(file):\(line) expected \(expected), got \(actual)") }
}
func XCTAssertNotNil<T>(_ value: T?, file: StaticString = #filePath, line: UInt = #line) throws {
    guard value != nil else { throw SmokeFailure(message: "\(file):\(line) unexpected nil") }
}
func XCTUnwrap<T>(_ value: T?) throws -> T {
    guard let value else { throw SmokeFailure(message: "unexpected nil") }
    return value
}

var failures = 0
func step(_ name: String, _ body: () throws -> Void) {
    do { try body(); print("PASS \(name)") }
    catch { failures += 1; print("FAIL \(name): \(error)") }
}

let base = URL(string: ProcessInfo.processInfo.environment["MEMO_URL"] ?? "http://127.0.0.1:8787")!

final class Context { var store: LocalStore!; var token = ""; var engine: SyncEngine!; var scheduler: ReminderScheduler! }
let ctx = Context()

step("设备注册/凭据") {
    if let token = ProcessInfo.processInfo.environment["MEMO_TOKEN"] {
        ctx.token = token
    } else {
        guard let code = ProcessInfo.processInfo.environment["MEMO_ENROLL"] else {
            throw StoreError.message("缺少 MEMO_ENROLL 注册码或 MEMO_TOKEN 凭据")
        }
        ctx.token = try awaitSync { try await APIClient.enroll(baseURL: base, code: code, deviceName: "smoke-\(Int(Date().timeIntervalSince1970))") }
    }
}

let root = FileManager.default.temporaryDirectory.appendingPathComponent("memo-smoke-\(UUID().uuidString)")
try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
ctx.store = try LocalStore(root: root)
ctx.engine = SyncEngine(store: ctx.store, api: APIClient(baseURL: base, token: ctx.token))
ctx.scheduler = ReminderScheduler(store: ctx.store)

var textItem: Item?
var shotItem: Item?

step("离线创建文字事项") {
    textItem = try ctx.store.createText("冒烟：明天上午确认预算数字")
    try XCTAssertEqual(try ctx.store.pending().count, 1)
}

step("离线创建截图事项") {
    shotItem = try ctx.store.createImages([tinyPNG()], method: "screenshot")
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.attachments.count, 1)
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.attachments.first?.state, "pending")
}

step("离线设置提醒（capture_and_remind）") {
    try ctx.store.setReminder(shotItem!.id, at: Date().addingTimeInterval(3600))
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.kind, "task")
}

step("同步：推送+上传附件+拉取") {
    var last = ""
    for _ in 0..<10 {
        last = try awaitSync { try await ctx.engine.runOnce() }
        let pending = try ctx.store.pending().filter { $0.state != "acknowledged" }
        let allReady = try ctx.store.items().allSatisfy { item in item.attachments.allSatisfy { $0.state == "ready" } || item.attachments.isEmpty }
        let cursor = try ctx.store.meta("cursor")
        if pending.isEmpty && allReady && cursor != nil { break }
        Thread.sleep(forTimeInterval: 0.3)
    }
    print("     sync result: \(last)")
    try XCTAssertEqual(try ctx.store.pending().filter { $0.state != "acknowledged" }.count, 0)
    let revision = try ctx.store.item(textItem!.id)?.revision ?? 0
    try XCTAssertEqual(revision >= 1, true)
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.attachments.first?.state, "ready")
    try XCTAssertNotNil(try ctx.store.meta("cursor"))
}

step("服务端确有附件内容（可回读）") {
    let attachment = try XCTUnwrap(try ctx.store.item(shotItem!.id)?.attachments.first)
    var components = URLComponents(url: base, resolvingAgainstBaseURL: false)!
    components.path = "/api/v1/attachments/\(attachment.id)/content"
    var request = URLRequest(url: components.url!)
    request.setValue("Bearer \(ctx.token)", forHTTPHeaderField: "Authorization")
    let (data, response) = try awaitSync { () -> (Data, URLResponse) in
        let (data, response) = try await URLSession.shared.data(for: request)
        return (data, response)
    }
    try XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
    try XCTAssertEqual(data.count, attachment.byteSize)
}

step("提醒未到期不弹出") {
    try XCTAssertEqual(try ctx.scheduler.due().count, 0)
}

step("完成待办后提醒失效") {
    try ctx.store.complete(shotItem!.id)
    try awaitSync { try await ctx.engine.runOnce() }
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.taskState, "completed")
    try XCTAssertEqual(try ctx.store.item(shotItem!.id)?.activeReminders.count, 0)
}

try? FileManager.default.removeItem(at: root)
print(failures == 0 ? "SMOKE OK" : "SMOKE FAILED \(failures) step(s)")
exit(failures == 0 ? 0 : 1)
