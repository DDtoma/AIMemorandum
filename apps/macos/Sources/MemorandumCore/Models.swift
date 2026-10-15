import Foundation

public enum Wire {
    public static let encoder: JSONEncoder = { let e = JSONEncoder(); e.keyEncodingStrategy = .convertToSnakeCase; e.outputFormatting = [.sortedKeys]; return e }()
    public static let decoder: JSONDecoder = { let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase; return d }()
    public static func timestamp(_ date: Date = Date()) -> String { ISO8601DateFormatter().string(from: date) }
    public static func date(_ value: String) -> Date? {
        let f = ISO8601DateFormatter(); f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
    public static func id() -> String { UUID().uuidString.lowercased() }
    public static func json(_ value: Any) throws -> String { String(decoding: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), as: UTF8.self) }
}

public struct Source: Codable { public var inputMethod: String; public init(_ method: String) { inputMethod = method } }
public struct Attachment: Codable, Identifiable {
    public var id: String; public var mime: String; public var byteSize: Int; public var sha256: String
    public var pixelWidth: Int; public var pixelHeight: Int; public var position: Int; public var state: String; public var revision: Int
    public var input: [String: Any] { ["id": id, "mime": mime, "byte_size": byteSize, "sha256": sha256, "pixel_width": pixelWidth, "pixel_height": pixelHeight, "position": position] }
}
public struct Reminder: Codable, Identifiable {
    public var id: String; public var itemId: String; public var remindAt: String; public var scheduleRevision: String
    public var purpose: String; public var enabled: Bool; public var revision: Int
    public var requestId: String { "\(id):\(scheduleRevision):\(remindAt)" }
}
public struct Item: Codable, Identifiable {
    public var id: String; public var ownerId: String?; public var kind: String; public var taskState: String?
    public var title: String; public var titleOrigin: String; public var bodyMd: String; public var timezone: String; public var revision: Int
    public var createdAt: String; public var updatedAt: String; public var completedAt: String?; public var deletedAt: String?
    public var dueAt: String?; public var dueDate: String?; public var plannedFor: String?; public var reviewAt: String?
    public var nextAction: String; public var resumeNote: String; public var source: Source; public var attachments: [Attachment]; public var reminders: [Reminder]
    public var ended: Bool { taskState == "completed" || taskState == "canceled" || deletedAt != nil }
    public var activeReminders: [Reminder] { ended || kind != "task" ? [] : reminders.filter(\.enabled) }
    public var needsAttention: Bool { !ended && activeReminders.contains { (Wire.date($0.remindAt) ?? .distantFuture) <= Date() } }
    public var input: [String: Any] {
        var data: [String: Any] = ["id": id, "kind": kind, "title": title, "title_origin": titleOrigin, "body_md": bodyMd, "timezone": timezone, "source": ["input_method": source.inputMethod], "attachments": attachments.map(\.input)]
        if kind == "task" { data["task_state"] = "todo" }
        return data
    }
    public static func fresh(text: String, method: String = "text", attachments: [Attachment] = [], task: Bool = false) -> Item {
        let stamp = Wire.timestamp()
        let title = text.isEmpty ? "截图 · " + DateFormatter.localizedString(from: Date(), dateStyle: .short, timeStyle: .short) : String(text.prefix(200))
        return Item(id: Wire.id(), kind: task ? "task" : "capture", taskState: task ? "todo" : nil, title: title, titleOrigin: text.isEmpty ? "generated" : "user", bodyMd: text, timezone: TimeZone.current.identifier, revision: 0, createdAt: stamp, updatedAt: stamp, nextAction: "", resumeNote: "", source: Source(method), attachments: attachments, reminders: [])
    }
}
public struct PendingOperation: Codable, Identifiable {
    public var id: String; public var itemId: String; public var path: String; public var body: String
    public var dependsOn: String?; public var frozen: Bool; public var state: String; public var error: String?
}
public enum InputPolicy {
    public static func shouldSubmit(keyCode: UInt16, shift: Bool, marked: Bool) -> Bool { (keyCode == 36 || keyCode == 76) && !shift && !marked }
}
public enum StoreError: LocalizedError {
    case message(String)
    public var errorDescription: String? { if case .message(let s) = self { return s }; return nil }
}
