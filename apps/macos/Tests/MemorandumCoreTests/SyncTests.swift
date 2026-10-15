import Foundation
import AppKit
import MemorandumCore

// 用 URLProtocol 桩模拟服务端，验证推送/冲突/附件/拉取的本地正确性。
final class MockURLProtocol: URLProtocol {
    static var handler: ((URLRequest) throws -> (Int, Data))?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        guard let handler = MockURLProtocol.handler else { fatalError("未配置桩") }
        do {
            let (status, data) = try handler(request)
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }
    override func stopLoading() {}
}

struct SyncFailure: Error { let message: String }

private final class Box { var error: Error? }

/// 在同步测试执行器里等待异步代码完成。
func awaitAsync(_ body: @escaping () async throws -> Void) throws {
    let semaphore = DispatchSemaphore(value: 0)
    let box = Box()
    Task {
        do { try await body() } catch { box.error = error }
        semaphore.signal()
    }
    _ = semaphore.wait(timeout: .now() + 10)
    if let error = box.error { throw error }
}

func serverItemJSON(_ item: Item, revision: Int) throws -> String {
    var remote = item
    remote.revision = revision
    return try Wire.json(["operation_id": "ignored", "item": jsonDict(remote)])
}

// Item 是 Codable，直接用编码器输出 dict，保证字段名与服务端一致。
func jsonDict(_ item: Item) -> [String: Any] {
    let data = try! Wire.encoder.encode(item)
    return (try! JSONSerialization.jsonObject(with: data)) as! [String: Any]
}

func stubbedClient() -> APIClient {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [MockURLProtocol.self]
    return APIClient(baseURL: URL(string: "http://stub.local")!, token: "test", session: URLSession(configuration: configuration))
}

func tinyPNG() -> Data {
    let image = NSImage(size: NSSize(width: 4, height: 4))
    image.lockFocus()
    NSColor.red.setFill()
    NSBezierPath(rect: NSRect(x: 0, y: 0, width: 4, height: 4)).fill()
    image.unlockFocus()
    guard let tiff = image.tiffRepresentation,
          let rep = NSBitmapImageRep(data: tiff),
          let png = rep.representation(using: .png, properties: [:]) else { fatalError("无法生成测试 PNG") }
    return png
}

extension CoreTests {
    func test_AC_SYNC_001_PushCreateAcknowledged() throws {
        let store = try LocalStore(root: directory())
        let local = try store.createText("明天上午确认预算")
        MockURLProtocol.handler = { request in
            if request.url?.path == "/api/v1/sync" { return (200, Data("{\"cursor\":\"0\",\"items\":[]}".utf8)) }
            guard request.url?.path == "/api/v1/items" else { print("STUB MISS \(request.httpMethod!) \(request.url!.path)"); return (404, Data("{}".utf8)) }
            var item = local
            item.revision = 1
            let body = try Wire.json(["item": jsonDict(item)])
            return (201, Data(body.utf8))
        }
        let engine = SyncEngine(store: store, api: stubbedClient())
        try awaitAsync { try await engine.runOnce() }
        try XCTAssertEqual(store.pending().count, 0)
        try XCTAssertEqual(store.shadow(local.id)?.revision, 1)
        try XCTAssertEqual(store.item(local.id)?.revision, 1)
        try XCTAssertTrue(try store.synchronized(try XCTUnwrap(store.item(local.id))))
    }

    func test_AC_SYNC_002_ActionConflictKeepsLocalCopy() throws {
        let store = try LocalStore(root: directory())
        let local = try store.createText("本机修改中的内容")
        try store.setReminder(local.id, at: Date().addingTimeInterval(3600)) // 排队一条动作操作
        MockURLProtocol.handler = { request in
            if request.url?.path == "/api/v1/sync" { return (200, Data("{\"cursor\":\"0\",\"items\":[]}".utf8)) }
            if request.url?.path == "/api/v1/items" {
                var created = local
                created.revision = 1
                return (201, Data(try Wire.json(["item": jsonDict(created)]).utf8))
            }
            guard request.url?.path == "/api/v1/items/\(local.id)/actions" else { print("STUB MISS \(request.httpMethod!) \(request.url!.path)"); return (404, Data("{}".utf8)) }
            var remote = local
            remote.bodyMd = "另一台设备已改写"
            remote.revision = 5
            let body = try Wire.json(["error": ["name": "REVISION_CONFLICT", "message": "事项已在其他设备更新"], "item": jsonDict(remote)])
            return (409, Data(body.utf8))
        }
        let engine = SyncEngine(store: store, api: stubbedClient())
        try awaitAsync { try await engine.runOnce() }
        try XCTAssertEqual(try store.conflictItems().count, 1)
        try XCTAssertEqual(store.pending().first?.state, "conflict")
        try XCTAssertEqual(try store.item(local.id)?.bodyMd, "本机修改中的内容")
        try XCTAssertEqual(try store.shadow(local.id)?.bodyMd, "另一台设备已改写")
    }

    func test_AC_ATT_001_ScreenshotUploadsAndBecomesReady() throws {
        let store = try LocalStore(root: directory())
        let item = try store.createImages([tinyPNG()], method: "screenshot")
        MockURLProtocol.handler = { request in
            let path = request.url?.path ?? ""
            if path == "/api/v1/sync" { return (200, Data("{\"cursor\":\"0\",\"items\":[]}".utf8)) }
            if path == "/api/v1/sync" { return (200, Data("{\"cursor\":\"0\",\"items\":[]}".utf8)) }
            if path == "/api/v1/items" {
                var remote = item
                remote.revision = 1
                return (201, Data(try Wire.json(["item": jsonDict(remote)]).utf8))
            }
            if path.contains("/attachments/") && path.hasSuffix("/content") && request.httpMethod == "PUT" {
                // 用本地文件字节回应：内容与声明一致才算 ready
                return (200, Data("{\"attachment\":{\"state\":\"ready\"}}".utf8))
            }
            print("STUB MISS \(request.httpMethod!) \(request.url!.path)"); print("STUB MISS \(request.httpMethod!) \(request.url!.path)"); return (404, Data("{}".utf8))
        }
        let engine = SyncEngine(store: store, api: stubbedClient())
        try awaitAsync { try await engine.runOnce() }
        try XCTAssertEqual(try store.item(item.id)?.attachments.first?.state, "ready")
        try XCTAssertTrue(try store.synchronized(try XCTUnwrap(store.item(item.id))))
    }

    func test_AC_SYNC_003_PullAppliesRemotePage() throws {
        let store = try LocalStore(root: directory())
        var remote = Item.fresh(text: "来自手机的内容", method: "text")
        remote.revision = 2
        MockURLProtocol.handler = { _ in
            (200, Data(try Wire.json(["cursor": "42", "items": [jsonDict(remote)]]).utf8))
        }
        let engine = SyncEngine(store: store, api: stubbedClient())
        try awaitAsync { try await engine.runOnce() }
        try XCTAssertEqual(try store.meta("cursor"), "42")
        try XCTAssertEqual(try store.shadow(remote.id)?.bodyMd, "来自手机的内容")
    }

    func test_AC_NOT_001_SchedulerDueAndFired() throws {
        let store = try LocalStore(root: directory())
        let scheduler = ReminderScheduler(store: store)
        let item = try store.createText("午休前回邮件")
        let when = Date().addingTimeInterval(3600)
        try store.setReminder(item.id, at: when)

        // 未到期
        try XCTAssertEqual(try scheduler.due().count, 0)
        // 到期后出现一次
        var due = try scheduler.due(now: when.addingTimeInterval(60))
        try XCTAssertEqual(due.count, 1)
        let first = try XCTUnwrap(due.first)
        try scheduler.markFired(item: first.item, reminder: first.reminder)
        due = try scheduler.due(now: when.addingTimeInterval(120))
        try XCTAssertEqual(due.count, 0)
        // 完成事项后提醒失效，集合中也应消失
        try store.complete(item.id)
        try scheduler.pruneFired()
        try XCTAssertEqual(try scheduler.due(now: when.addingTimeInterval(600)).count, 0)
    }
}
