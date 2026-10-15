import Foundation

// 服务端错误信封：{"error":{"name","message"},"item":{...可选当前版本...}}
public struct APIError: Error {
    public let status: Int
    public let name: String
    public let message: String
    public let item: Item?
    public var isConflict: Bool { status == 409 }
}

/// 与 Go 服务端通信的薄客户端。操作体直接复用 LocalStore Outbox 冻结后的 JSON，
/// 保证"本地排队的内容"与"实际发出的内容"逐字节一致。
public struct APIClient {
    public let baseURL: URL
    public var token: String
    public let session: URLSession

    public init(baseURL: URL, token: String, session: URLSession = .shared) {
        self.baseURL = baseURL
        self.token = token
        self.session = session
    }

    @discardableResult
    private func request(_ method: String, _ url: URL, body: Data? = nil, contentType: String = "application/json") async throws -> (Int, Data) {
        var req = URLRequest(url: url, timeoutInterval: 15)
        req.httpMethod = method
        req.httpBody = body
        req.setValue(contentType, forHTTPHeaderField: "Content-Type")
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await session.data(for: req)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        return (status, data)
    }

    private func url(_ path: String, query: [String: String] = [:]) -> URL {
        var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)!
        var trimmed = Substring(path)
        while trimmed.hasPrefix("/") { trimmed = trimmed.dropFirst() }
        components.path = "/" + trimmed
        if !query.isEmpty {
            components.queryItems = query.map { URLQueryItem(name: $0.key, value: $0.value) }.sorted { $0.name < $1.name }
        }
        return components.url!
    }

    private func parseError(_ status: Int, _ data: Data) -> APIError {
        var name = "HTTP_\(status)"
        var message = "服务端返回 \(status)"
        var item: Item?
        if let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            if let error = object["error"] as? [String: Any] {
                name = (error["name"] as? String) ?? name
                message = (error["message"] as? String) ?? message
            }
            if let remote = object["item"] as? [String: Any], let remoteData = try? JSONSerialization.data(withJSONObject: remote) {
                item = try? Wire.decoder.decode(Item.self, from: remoteData)
            }
        }
        return APIError(status: status, name: name, message: message, item: item)
    }

    private func decodeItem(_ data: Data) throws -> Item {
        let object = try JSONSerialization.jsonObject(with: data)
        guard let dict = object as? [String: Any], let itemValue = dict["item"] ?? (dict["id"] != nil ? object : nil) else {
            throw StoreError.message("服务端响应缺少事项")
        }
        let itemData = try JSONSerialization.data(withJSONObject: itemValue)
        return try Wire.decoder.decode(Item.self, from: itemData)
    }

    /// 提交一条 Outbox 操作（创建或动作），返回服务端版本的事项。
    public func submit(operationPath: String, body: String) async throws -> Item {
        let (status, data) = try await request("POST", url(operationPath), body: Data(body.utf8))
        guard (200..<300).contains(status) else { throw parseError(status, data) }
        return try decodeItem(data)
    }

    /// 创建冲突（事项已在服务端存在，如恢复安装）时读取当前版本。
    public func item(_ id: String) async throws -> Item {
        let (status, data) = try await request("GET", url("api/v1/items/\(id)"))
        guard status == 200 else { throw parseError(status, data) }
        return try decodeItem(data)
    }

    public func uploadAttachment(id: String, data: Data) async throws {
        let (status, responseData) = try await request("PUT", url("api/v1/attachments/\(id)/content"), body: data, contentType: "application/octet-stream")
        guard (200..<300).contains(status) else { throw parseError(status, responseData) }
    }

    public func pull(cursor: String, limit: Int = 200) async throws -> (items: [Item], cursor: String) {
        let (status, data) = try await request("GET", url("api/v1/sync", query: ["cursor": cursor]))
        guard status == 200 else { throw parseError(status, data) }
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let newCursor = object["cursor"] as? String,
              let rows = object["items"] as? [[String: Any]] else {
            throw StoreError.message("同步响应无效")
        }
        var items: [Item] = []
        for row in rows {
            let rowData = try JSONSerialization.data(withJSONObject: row)
            items.append(try Wire.decoder.decode(Item.self, from: rowData))
        }
        return (items, newCursor)
    }

    /// 用注册码换取设备凭据。
    public static func enroll(baseURL: URL, code: String, deviceName: String, session: URLSession = .shared) async throws -> String {
        var components = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)!
        components.path = components.path.hasSuffix("/") ? components.path + "api/v1/device-enrollments" : "/" + "api/v1/device-enrollments"
        var req = URLRequest(url: components.url!, timeoutInterval: 15)
        req.httpMethod = "POST"
        req.httpBody = try Wire.encoder.encode(["enrollment_code": code, "device_name": deviceName])
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await session.data(for: req)
        guard (response as? HTTPURLResponse)?.statusCode == 201,
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let token = object["device_token"] as? String else {
            throw StoreError.message("注册失败：请检查服务地址与注册码")
        }
        return token
    }
}
