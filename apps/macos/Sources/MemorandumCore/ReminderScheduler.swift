import Foundation

public struct DueReminder {
    public let item: Item
    public let reminder: Reminder
    public var remindAt: Date { Wire.date(reminder.remindAt) ?? .distantPast }
}

/// 本地提醒到期判定。通知只负责"再出现一次"，不代替完成；
/// 完成事项后其提醒被禁用，自然从到期集合中消失（同事务规则）。
public final class ReminderScheduler {
    private let store: LocalStore
    private static let firedKey = "fired_reminders"

    public init(store: LocalStore) {
        self.store = store
    }

    public static func key(item: Item, reminder: Reminder) -> String {
        "\(item.id):\(reminder.id):\(reminder.scheduleRevision):\(reminder.remindAt)"
    }

    public func due(now: Date = Date()) throws -> [DueReminder] {
        let fired = try firedKeys()
        var result: [DueReminder] = []
        for item in try store.items() {
            for reminder in item.activeReminders {
                guard let at = Wire.date(reminder.remindAt), at <= now else { continue }
                if !fired.contains(Self.key(item: item, reminder: reminder)) {
                    result.append(DueReminder(item: item, reminder: reminder))
                }
            }
        }
        return result.sorted { $0.remindAt < $1.remindAt }
    }

    public func markFired(item: Item, reminder: Reminder) throws {
        var keys = try firedKeys()
        keys.insert(Self.key(item: item, reminder: reminder))
        try store.setMeta(Self.firedKey, value: try Wire.json(["keys": keys.sorted()]))
    }

    /// 提醒被改期或事项结束后清理失效的已触发记录，防止集合无限增长。
    public func pruneFired() throws {
        var active = Set<String>()
        for item in try store.items() {
            for reminder in item.activeReminders {
                active.insert(Self.key(item: item, reminder: reminder))
            }
        }
        var keys = try firedKeys()
        let before = keys.count
        keys = keys.intersection(active)
        if keys.count != before {
            try store.setMeta(Self.firedKey, value: try Wire.json(["keys": keys.sorted()]))
        }
    }

    private func firedKeys() throws -> Set<String> {
        guard let raw = try store.meta(Self.firedKey),
              let object = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String: Any],
              let list = object["keys"] as? [String] else { return [] }
        return Set(list)
    }
}
