import Foundation
import CryptoKit

/// 推送 Outbox → 上传附件 → 拉取变更。一次 runOnce 完成全部三段；
/// 任何网络失败向上抛出，由调用方决定重试节奏；本地数据不受影响。
public final class SyncEngine {
    private let store: LocalStore
    public let api: APIClient

    public init(store: LocalStore, api: APIClient) {
        self.store = store
        self.api = api
    }

    @discardableResult
    public func runOnce() async throws -> String {
        let pushed = try await pushPending()
        let uploaded = try await uploadAttachments()
        let pulled = try await pullChanges()
        return "pushed=\(pushed) uploaded=\(uploaded) pulled=\(pulled)"
    }

    // MARK: - 推送

    private func pushPending() async throws -> Int {
        var pushed = 0
        while pushed < 100 {
            guard let queued = try store.pending().first(where: { $0.state == "queued" }) else { break }
            guard let operation = try store.prepare(queued) else { break } // 依赖未确认，等待下一轮
            do {
                let remote = try await api.submit(operationPath: operation.path, body: operation.body)
                try store.acknowledge(operation, remote: remote)
                pushed += 1
            } catch let error as APIError {
                switch (error.status, error.name) {
                case (409, "REVISION_CONFLICT"):
                    // 远端已前进：把服务端当前版本作为冲突副本保留，等待人工裁决。
                    try store.fail(operation, message: error.message, conflict: error.item)
                case (409, "ITEM_EXISTS"):
                    // 本地重建/重装场景：直接采纳服务端版本。
                    let remote: Item?
                    if let known = error.item {
                        remote = known
                    } else {
                        remote = try? await api.item(operation.itemId)
                    }
                    if let remote {
                        try store.acknowledge(operation, remote: remote)
                        pushed += 1
                    } else {
                        try store.fail(operation, message: error.message, permanent: true)
                    }
                case (404, "NOT_FOUND"):
                    try store.fail(operation, message: error.message, permanent: true)
                case (422, "INVALID_STATE"), (422, "UNSUPPORTED_ACTION"), (422, "STALE_SCHEDULE"):
                    try store.fail(operation, message: error.message, permanent: true)
                default:
                    throw error // 网络/服务端临时错误：保持 queued，下一轮重试
                }
            }
        }
        return pushed
    }

    // MARK: - 附件上传

    private func uploadAttachments() async throws -> Int {
        var uploaded = 0
        for item in try store.items() {
            guard item.attachments.contains(where: { $0.state == "pending" }) else { continue }
            // 事项必须已在服务端登记（创建操作已确认）才能上传内容。
            guard try store.shadow(item.id)?.revision ?? 0 > 0 else { continue }
            for attachment in item.attachments where attachment.state == "pending" {
                let url = store.attachmentURL(attachment)
                let data = try Data(contentsOf: url)
                let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
                guard data.count == attachment.byteSize, digest == attachment.sha256 else {
                    throw StoreError.message("本地图片损坏，暂停上传：\(attachment.id)")
                }
                try await api.uploadAttachment(id: attachment.id, data: data)
                var ready = attachment
                ready.state = "ready"
                try store.updateAttachment(ready)
                uploaded += 1
            }
        }
        return uploaded
    }

    // MARK: - 拉取

    private func pullChanges() async throws -> Int {
        let cursor = (try store.meta("cursor")) ?? "0"
        let page = try await api.pull(cursor: cursor)
        guard !page.items.isEmpty || page.cursor != cursor else { return 0 }
        try store.applyPage(page.items, cursor: page.cursor)
        return page.items.count
    }

    /// 面板展示用的简明状态。
    public func summary() -> String {
        let pending = (try? store.pending())?.count ?? 0
        let conflicts = (try? store.conflictItems())?.count ?? 0
        if conflicts > 0 { return "\(conflicts) 条冲突待处理" }
        if pending > 0 { return "\(pending) 条待同步" }
        return "已同步"
    }
}
