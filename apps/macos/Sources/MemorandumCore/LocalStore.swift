import Foundation
import CryptoKit
import ImageIO
import CSQLite

public final class LocalStore {
    public let root: URL
    private var db: OpaquePointer?
    private let lock = NSRecursiveLock()
    public private(set) var recoveryErrors: [String] = []
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    public init(root: URL) throws {
        self.root = root
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        guard sqlite3_open_v2(root.appendingPathComponent("memorandum.sqlite").path, &db, SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX, nil) == SQLITE_OK else { throw StoreError.message("无法打开本地数据库") }
        try execute("PRAGMA journal_mode=WAL")
        try execute("PRAGMA synchronous=FULL")
        try execute("PRAGMA busy_timeout=5000")
        try execute("CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, json TEXT NOT NULL)")
        try execute("CREATE TABLE IF NOT EXISTS shadow (id TEXT PRIMARY KEY, json TEXT NOT NULL)")
        try execute("CREATE TABLE IF NOT EXISTS outbox (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL, item_id TEXT NOT NULL, json TEXT NOT NULL, state TEXT NOT NULL, result_revision INTEGER)")
        try execute("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        try execute("CREATE TABLE IF NOT EXISTS conflicts (item_id TEXT PRIMARY KEY, local_json TEXT NOT NULL, remote_json TEXT NOT NULL)")
        try recover()
    }
    deinit { sqlite3_close(db) }
    private func guarded<T>(_ body: () throws -> T) rethrows -> T { lock.lock(); defer { lock.unlock() }; return try body() }
    private func execute(_ sql: String, _ params: [String] = []) throws { _ = try rows(sql, params) }
    private func rows(_ sql: String, _ params: [String] = []) throws -> [[String: String]] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { throw StoreError.message(String(cString: sqlite3_errmsg(db))) }
        defer { sqlite3_finalize(statement) }
        for (index, value) in params.enumerated() { sqlite3_bind_text(statement, Int32(index + 1), value, -1, transient) }
        var values: [[String: String]] = []
        while true {
            let result = sqlite3_step(statement)
            if result == SQLITE_DONE { return values }
            guard result == SQLITE_ROW else { throw StoreError.message(String(cString: sqlite3_errmsg(db))) }
            var row: [String: String] = [:]
            for column in 0..<sqlite3_column_count(statement) {
                if let value = sqlite3_column_text(statement, column) { row[String(cString: sqlite3_column_name(statement, column))] = String(cString: value) }
            }
            values.append(row)
        }
    }
    private func transaction<T>(_ body: () throws -> T) throws -> T {
        try execute("BEGIN IMMEDIATE")
        do { let result = try body(); try execute("COMMIT"); return result }
        catch { try? execute("ROLLBACK"); throw error }
    }
    private func encoded<T: Encodable>(_ value: T) throws -> String { String(decoding: try Wire.encoder.encode(value), as: UTF8.self) }
    private func saveItem(_ item: Item, table: String = "items") throws { try execute("INSERT INTO \(table)(id,json) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET json=excluded.json", [item.id, try encoded(item)]) }
    public func items() throws -> [Item] { try guarded { try rows("SELECT json FROM items").map { try Wire.decoder.decode(Item.self, from: Data($0["json"]!.utf8)) }.sorted { $0.createdAt > $1.createdAt } } }
    public func item(_ id: String) throws -> Item? { try guarded { try lookup(id, table: "items") } }
    public func shadow(_ id: String) throws -> Item? { try guarded { try lookup(id, table: "shadow") } }
    private func lookup(_ id: String, table: String) throws -> Item? { guard let json = try rows("SELECT json FROM \(table) WHERE id=?", [id]).first?["json"] else { return nil }; return try Wire.decoder.decode(Item.self, from: Data(json.utf8)) }
    public func meta(_ key: String) throws -> String? { try guarded { try rows("SELECT value FROM meta WHERE key=?", [key]).first?["value"] } }
    public func setMeta(_ key: String, value: String) throws { try guarded { try execute("INSERT INTO meta(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key,value]) } }
    public func pending() throws -> [PendingOperation] { try guarded { try rows("SELECT json FROM outbox WHERE state <> 'acknowledged' ORDER BY seq").map { try Wire.decoder.decode(PendingOperation.self, from: Data($0["json"]!.utf8)) } } }
    public func conflictItems() throws -> [String] { try guarded { try rows("SELECT item_id FROM conflicts").compactMap { $0["item_id"] } } }
    private func queue(_ operation: PendingOperation) throws { try execute("INSERT INTO outbox(id,item_id,json,state) VALUES (?,?,?,?)", [operation.id,operation.itemId,try encoded(operation),operation.state]) }
    private func creation(_ item: Item) throws -> PendingOperation {
        let id = Wire.id()
        return PendingOperation(id: id, itemId: item.id, path: "/api/v1/items", body: try Wire.json(["operation_id":id,"item":item.input]), frozen: false, state: "queued")
    }
    @discardableResult public func createText(_ text: String, task: Bool = true) throws -> Item {
        try guarded {
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw StoreError.message("请输入内容") }
            guard text.utf8.count <= 262144 else { throw StoreError.message("正文最多256 KiB") }
            let item = Item.fresh(text: text, task: task)
            try transaction { try saveItem(item); try queue(creation(item)) }
            return item
        }
    }
    public func attachmentURL(_ attachment: Attachment) -> URL { root.appendingPathComponent("attachments").appendingPathComponent(attachment.id).appendingPathComponent(attachment.mime == "image/png" ? "original.png" : "original.jpg") }
    private struct CaptureManifest: Codable { let item: Item; let operation: PendingOperation }
    public func createImages(_ data: [Data], method: String = "screenshot", body: String? = nil) throws -> Item {
        try guarded {
            guard !data.isEmpty && data.count <= 10 && data.reduce(0, { $0 + $1.count }) <= 52428800 else { throw StoreError.message("每条最多10张图片，总量最多50 MiB") }
            var attachments: [Attachment] = []
            for (index, bytes) in data.enumerated() {
                guard bytes.count <= 20971520, let image = CGImageSourceCreateWithData(bytes as CFData, nil), CGImageSourceGetCount(image) == 1,
                      let type = CGImageSourceGetType(image) as String?, ["public.png", "public.jpeg"].contains(type),
                      let info = CGImageSourceCopyPropertiesAtIndex(image, 0, nil) as? [String: Any],
                      let width = info[kCGImagePropertyPixelWidth as String] as? Int, let height = info[kCGImagePropertyPixelHeight as String] as? Int,
                      width > 0, height > 0, width * height <= 40000000,
                      CGImageSourceCreateImageAtIndex(image, 0, nil) != nil else { throw StoreError.message("只接受完整 PNG/JPEG；每张最多20 MiB、40百万像素") }
                attachments.append(Attachment(id: Wire.id(), mime: type == "public.png" ? "image/png" : "image/jpeg", byteSize: bytes.count, sha256: SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined(), pixelWidth: width, pixelHeight: height, position: index, state: "pending", revision: 1))
            }
            var item = Item.fresh(text: "", method: method, attachments: attachments, task: true)
            // 识别文字（OCR）写入正文，标题保持自动生成的"截图 · 时间"。
            if let body, !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                item.bodyMd = String(body.prefix(262144))
            }
            let manifest = CaptureManifest(item: item, operation: try creation(item))
            let staging = root.appendingPathComponent("staging").appendingPathComponent(item.id)
            try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
            try durableWrite(try Wire.encoder.encode(manifest), to: staging.appendingPathComponent("manifest.json"))
            for (index, attachment) in attachments.enumerated() { try durableWrite(data[index], to: attachmentURL(attachment)) }
            try transaction { try saveItem(item); try queue(manifest.operation) }
            try? FileManager.default.removeItem(at: staging)
            return item
        }
    }
    private func durableWrite(_ bytes: Data, to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try bytes.write(to: url, options: .atomic)
        let handle = try FileHandle(forWritingTo: url); defer { try? handle.close() }; try handle.synchronize()
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
    private func recover() throws {
        let staging = root.appendingPathComponent("staging")
        guard let entries = try? FileManager.default.contentsOfDirectory(at: staging, includingPropertiesForKeys: nil) else { return }
        for entry in entries {
            do {
                let manifest = try Wire.decoder.decode(CaptureManifest.self, from: Data(contentsOf: entry.appendingPathComponent("manifest.json")))
                if try item(manifest.item.id) == nil {
                    for a in manifest.item.attachments { let data = try Data(contentsOf: attachmentURL(a)); guard data.count == a.byteSize && SHA256.hash(data: data).map({ String(format: "%02x", $0) }).joined() == a.sha256 else { throw StoreError.message("恢复图片校验失败") } }
                    try transaction { try saveItem(manifest.item); try queue(manifest.operation) }
                }
                try FileManager.default.removeItem(at: entry)
            } catch { recoveryErrors.append("一条截图尚未恢复：\(error.localizedDescription)") }
        }
    }
    public func setReminder(_ id: String, at: Date, reminderId: String? = nil, expectedSchedule: String? = nil) throws {
        guard at > Date() else { throw StoreError.message("请选择未来的提醒时间") }
        try guarded {
            guard var item = try item(id), !item.ended else { throw StoreError.message("事项已结束") }
            let ruleId = reminderId ?? Wire.id(); let schedule = Wire.id()
            let name = expectedSchedule != nil ? "snooze" : (item.kind == "task" ? "set_reminder" : "capture_and_remind")
            var payload: [String: Any] = ["reminder_id":ruleId,"remind_at":Wire.timestamp(at),"schedule_revision":schedule]
            if let expectedSchedule { payload["expected_schedule_revision"] = expectedSchedule }
            if name == "snooze", !item.reminders.contains(where: { $0.id == ruleId && $0.enabled && $0.scheduleRevision == expectedSchedule }) { throw StoreError.message("这条提醒已经更新") }
            item.kind = "task"; item.taskState = "todo"
            let rule = Reminder(id: ruleId, itemId: id, remindAt: Wire.timestamp(at), scheduleRevision: schedule, purpose: "explicit", enabled: true, revision: (item.reminders.first(where: { $0.id == ruleId })?.revision ?? 0) + 1)
            if let index = item.reminders.firstIndex(where: { $0.id == ruleId }) { item.reminders[index] = rule } else { guard item.activeReminders.count < 8 else { throw StoreError.message("每项最多8条提醒") }; item.reminders.append(rule) }
            try enqueueAction(&item, name: name, payload: payload)
        }
    }
    public func complete(_ id: String) throws {
        try guarded { guard var item = try item(id), item.kind == "task", !item.ended else { throw StoreError.message("只有未结束待办可以完成") }; item.taskState = "completed"; item.completedAt = Wire.timestamp(); for i in item.reminders.indices { item.reminders[i].enabled = false }; try enqueueAction(&item, name: "complete", payload: [:]) }
    }
    /// 重开已结束待办（回收站中的除外）。
    public func reopen(_ id: String) throws {
        try guarded {
            guard var item = try item(id), item.kind == "task", item.deletedAt == nil,
                  item.taskState == "completed" || item.taskState == "canceled" else { throw StoreError.message("只有已结束的待办可以重开") }
            item.taskState = "todo"; item.completedAt = nil
            try enqueueAction(&item, name: "reopen", payload: [:])
        }
    }
    /// 删除进回收站：停用提醒并入队 delete 动作，服务端同语义。
    public func deleteItem(_ id: String) throws {
        try guarded {
            guard var item = try item(id), item.deletedAt == nil else { throw StoreError.message("事项不存在或已删除") }
            item.deletedAt = Wire.timestamp()
            for i in item.reminders.indices { item.reminders[i].enabled = false }
            try enqueueAction(&item, name: "delete", payload: [:])
        }
    }
    /// 更新标题与正文（详情页编辑保存）：本地立即生效并入队 PUT，同步时推送服务端。
    public func update(_ id: String, title: String, body: String) throws {
        try guarded {
            guard var item = try item(id), item.deletedAt == nil else { throw StoreError.message("事项不存在或已删除") }
            let trimmedTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
            let trimmedBody = body.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmedTitle.isEmpty || !trimmedBody.isEmpty else { throw StoreError.message("标题与正文不能同时为空") }
            item.title = trimmedTitle.isEmpty ? String(trimmedBody.prefix(200)) : trimmedTitle
            item.titleOrigin = "user"
            item.bodyMd = trimmedBody.isEmpty ? trimmedTitle : trimmedBody
            let previous = try pending().last(where: { $0.itemId == item.id })
            let opId = Wire.id()
            let base = try shadow(item.id)?.revision ?? item.revision
            let body = try Wire.json(["operation_id": opId, "base_revision": base, "title": item.title, "body_md": item.bodyMd])
            let operation = PendingOperation(id: opId, itemId: item.id, path: "/api/v1/items/\(item.id)", body: body, dependsOn: previous?.id, frozen: false, state: "queued")
            item.updatedAt = Wire.timestamp()
            try transaction { try saveItem(item); try queue(operation) }
        }
    }
    public func convertToTask(_ id: String) throws { try guarded { guard var item = try item(id), !item.ended else { throw StoreError.message("事项已结束") }; item.kind = "task"; item.taskState = "todo"; try enqueueAction(&item, name: "convert", payload: ["kind":"task"]) } }
    private func enqueueAction(_ item: inout Item, name: String, payload: [String: Any]) throws {
        let previous = try pending().last(where: { $0.itemId == item.id })
        let opId = Wire.id()
        let base = try shadow(item.id)?.revision ?? item.revision
        let operation = PendingOperation(id: opId, itemId: item.id, path: "/api/v1/items/\(item.id)/actions", body: try Wire.json(["operation_id":opId,"base_revision":base,"action":name,"payload":payload]), dependsOn: previous?.id, frozen: false, state: "queued")
        item.updatedAt = Wire.timestamp()
        try transaction { try saveItem(item); try queue(operation) }
    }
    public func prepare(_ operation: PendingOperation) throws -> PendingOperation? {
        try guarded {
            guard operation.state == "queued" else { return nil }
            if operation.frozen { return operation }
            var frozen = operation
            if let dependency = operation.dependsOn {
                guard let row = try rows("SELECT result_revision,state FROM outbox WHERE id=?", [dependency]).first, row["state"] == "acknowledged", let revision = row["result_revision"].flatMap(Int.init) else { return nil }
                var json = try JSONSerialization.jsonObject(with: Data(operation.body.utf8)) as! [String: Any]; json["base_revision"] = revision; frozen.body = try Wire.json(json)
            }
            frozen.frozen = true
            try execute("UPDATE outbox SET json=? WHERE id=?", [try encoded(frozen), frozen.id])
            return frozen
        }
    }
    public func acknowledge(_ operation: PendingOperation, remote: Item) throws {
        try guarded { try transaction {
            try execute("UPDATE outbox SET state='acknowledged',result_revision=? WHERE id=?", [String(remote.revision),operation.id])
            try saveItem(remote, table: "shadow")
            if try rows("SELECT id FROM outbox WHERE item_id=? AND state<>'acknowledged'", [remote.id]).isEmpty { try saveItem(remote) }
        } }
    }
    public func fail(_ operation: PendingOperation, message: String, conflict: Item? = nil, permanent: Bool = false) throws {
        try guarded { var changed = operation; changed.error = message; changed.state = conflict != nil ? "conflict" : (permanent ? "invalid" : "queued")
            try execute("UPDATE outbox SET state=?,json=? WHERE id=?", [changed.state,try encoded(changed),changed.id])
            if let conflict, let local = try item(operation.itemId) { try saveItem(conflict,table:"shadow"); try execute("INSERT INTO conflicts(item_id,local_json,remote_json) VALUES (?,?,?) ON CONFLICT(item_id) DO UPDATE SET remote_json=excluded.remote_json", [local.id,try encoded(local),try encoded(conflict)]) }
        }
    }
    public func applyRemote(_ remote: Item) throws {
        try guarded { try saveItem(remote,table:"shadow"); if try rows("SELECT id FROM outbox WHERE item_id=? AND state<>'acknowledged'", [remote.id]).isEmpty { try saveItem(remote) } }
    }
    public func applyPage(_ items: [Item], cursor: String) throws { try guarded { try transaction { for item in items { try applyRemote(item) }; try setMeta("cursor", value: cursor) } } }
    public func updateAttachment(_ attachment: Attachment) throws {
        try guarded { for table in ["items","shadow"] { if var item = try lookup(attachmentItem(attachment.id), table: table), let index = item.attachments.firstIndex(where: { $0.id == attachment.id }) { item.attachments[index] = attachment; try saveItem(item,table:table) } } }
    }
    private func attachmentItem(_ id: String) throws -> String { try items().first(where: { $0.attachments.contains(where: { $0.id == id }) })?.id ?? "" }
    public func useRemote(_ id: String) throws { try guarded { guard let remote = try shadow(id) else { return }; try transaction { try execute("UPDATE outbox SET state='acknowledged' WHERE item_id=?",[id]);try saveItem(remote);try execute("DELETE FROM conflicts WHERE item_id=?",[id]) } } }
    public func synchronized(_ item: Item) throws -> Bool { try guarded { try rows("SELECT id FROM outbox WHERE item_id=? AND state<>'acknowledged'",[item.id]).isEmpty && item.attachments.allSatisfy { $0.state == "ready" } && item.revision > 0 } }
}
