import AppKit
import MemorandumCore

// 应用装配：菜单栏常驻 + 全局热键 + 后台同步/提醒循环。
// 日常路径全部一步直达：⌥⌘M 文字、⌥⌘S 截图、⌥⌘V 剪贴板速存。
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var store: LocalStore!
    private var engine: SyncEngine!
    private var scheduler: ReminderScheduler!
    private var panel: CapturePanel!
    private var notifier: ReminderNotifier!
    private var screenshot: ScreenshotCoordinator!
    private var itemsWindow: ItemsWindow?
    private var statusItem: NSStatusItem!
    private var syncTask: Task<Void, Never>?
    private var captureError: String?

    private var serverURL: URL? {
        guard let raw = try? store.meta("server_url"), let url = URL(string: raw) else { return nil }
        return url
    }

    private var token: String? { try? store.meta("device_token") }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        let root = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("AIMemorandum", isDirectory: true)
        do {
            store = try LocalStore(root: root)
            if let recoveryErrors = try? store.recoveryErrors, !recoveryErrors.isEmpty {
                NSLog("崩溃恢复提示: \(recoveryErrors.joined(separator: "; "))")
            }
        } catch {
            fatalAlert("本地数据库初始化失败：\(error.localizedDescription)")
            return
        }
        engine = SyncEngine(store: store, api: APIClient(baseURL: serverURL ?? defaultServerURL(), token: token ?? ""))
        scheduler = ReminderScheduler(store: store)

        panel = CapturePanel()
        panel.onSaveText = { [weak self] text, task in self?.save(text: text, task: task) }
        panel.onSaveImages = { [weak self] images, _ in self?.save(images: images) }

        screenshot = ScreenshotCoordinator()
        screenshot.onWillBegin = { [weak self] in
            // 已打开的输入与记录窗口先收起；截图结束后也不自动弹回。
            self?.panel.orderOut(nil)
            self?.itemsWindow?.orderOut(nil)
        }
        screenshot.onSaved = { [weak self] data, ocrText in
            self?.captureError = nil
            self?.save(images: data, method: "screenshot", ocrText: ocrText)
        }
        screenshot.onError = { [weak self] message in
            self?.captureError = message
            self?.statusItem.button?.toolTip = message
            self?.rebuildMenu()
        }

        notifier = ReminderNotifier(actions: .init(
            complete: { [weak self] due in
                try? self?.store.complete(due.item.id)
                self?.triggerSync()
            },
            snooze: { [weak self] due, seconds in
                try? self?.store.setReminder(due.item.id, at: Date().addingTimeInterval(seconds),
                                             reminderId: due.reminder.id,
                                             expectedSchedule: due.reminder.scheduleRevision)
                self?.triggerSync()
            },
            dismiss: { [weak self] due in
                try? self?.scheduler.markFired(item: due.item, reminder: due.reminder)
            }
        ))

        installStatusItem()
        installHotKeys()
        startSyncLoop()
        startReminderLoop()

        if token == nil {
            showServerSetup(firstRun: true)
        }
    }

    private func defaultServerURL() -> URL { URL(string: "http://127.0.0.1:8787")! }

    private func fatalAlert(_ message: String) {
        let alert = NSAlert()
        alert.messageText = "AIMemorandum 无法启动"
        alert.informativeText = message
        alert.runModal()
        NSApp.terminate(nil)
    }

    // MARK: - 菜单栏

    private func installStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.image = NSImage(systemSymbolName: "note.text", accessibilityDescription: "AIMemorandum")
        statusItem.button?.imagePosition = .imageOnly
        rebuildMenu()
    }

    /// 动态重建状态栏菜单：顶部快捷操作 + 待办列表（点击完成）+ 同步/设置/退出。
    private func rebuildMenu() {
        let menu = NSMenu()
        menu.autoenablesItems = false
        func item(_ title: String, _ symbol: String, _ action: Selector?, _ keyEquivalent: String = "") -> NSMenuItem {
            let menuItem = NSMenuItem(title: title, action: action, keyEquivalent: keyEquivalent)
            menuItem.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
            if action != nil { menuItem.target = self }
            return menuItem
        }
        menu.addItem(item("快速记录  ⌥⌘M", "square.and.pencil", #selector(openPanel)))
        menu.addItem(item("截图记录  ⌥⌘S", "camera.viewfinder", #selector(takeScreenshot)))
        menu.addItem(item("速存剪贴板  ⌥⌘V", "doc.on.clipboard", #selector(saveClipboard)))
        menu.addItem(item("查看我的记录  ⌥⌘L", "list.bullet.rectangle", #selector(openItems)))
        if let captureError {
            let warning = item(captureError, "exclamationmark.triangle", nil)
            warning.isEnabled = false
            menu.addItem(warning)
        }

        // 待办区：未完成待办按创建时间正序（等得最久的在前），点击标记完成。
        let todos = ((try? store.items()) ?? []).filter { !$0.ended && $0.kind == "task" && $0.deletedAt == nil }
            .sorted { $0.createdAt < $1.createdAt }
        if !todos.isEmpty {
            menu.addItem(.separator())
            let header = NSMenuItem(title: "待办（\(todos.count)）", action: nil, keyEquivalent: "")
            header.isEnabled = false
            menu.addItem(header)
            for todo in todos.prefix(12) {
                let title = todo.title.count > 36 ? String(todo.title.prefix(36)) + "…" : todo.title
                let menuItem = item(title, "circle", #selector(completeFromMenu(_:)))
                menuItem.representedObject = todo.id
                menuItem.toolTip = "点击标记完成"
                menu.addItem(menuItem)
            }
        }

        menu.addItem(.separator())
        menu.addItem(item("立即同步", "arrow.triangle.2.circlepath", #selector(syncNow)))
        menu.addItem(item("服务端设置…", "gearshape", #selector(openServerSetup)))
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "退出 AIMemorandum", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        quit.image = NSImage(systemSymbolName: "power", accessibilityDescription: nil)
        menu.addItem(quit)
        statusItem.menu = menu
        statusItem.button?.toolTip = captureError ?? "AIMemorandum：\(engine.summary())"
    }

    @objc private func completeFromMenu(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String else { return }
        try? store.complete(id)
        triggerSync()
        rebuildMenu()
        itemsWindow?.refresh()
    }

    @objc private func openPanel() { panel.toggle() }

    @objc private func openItems() {
        if itemsWindow == nil {
            let window = ItemsWindow(store: store)
            window.onSync = { [weak self] in self?.triggerSync() }
            itemsWindow = window
        }
        itemsWindow?.openAndRefresh()
    }
    @objc private func takeScreenshot() { screenshot.begin() }

    @objc private func saveClipboard() {
        let pasteboard = NSPasteboard.general
        var imageData: Data?
        if let png = pasteboard.data(forType: .png) {
            imageData = png
        } else if let tiff = pasteboard.data(forType: .tiff),
                  let rep = NSBitmapImageRep(data: tiff),
                  let png = rep.representation(using: .png, properties: [:]) {
            imageData = png
        }
        if let imageData {
            // OCR 在后台完成后再保存，识别失败则原样保存图片。
            Task { [weak self] in
                let text = await ScreenshotCoordinator.ocrText(from: imageData)
                await MainActor.run {
                    guard let self else { return }
                    if (try? self.store.createImages([imageData], method: "clipboard", body: text)) != nil {
                        self.panel.showToast(text.map { _ in "已保存并提取文字" } ?? "剪贴板图片已保存")
                        self.itemsWindow?.refresh()
                        self.triggerSync()
                    } else {
                        self.panel.showError("剪贴板图片保存失败")
                    }
                }
            }
            return
        }
        if let text = pasteboard.string(forType: .string), !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            save(text: text, task: true, presentToast: false)
            panel.showToast("剪贴板文字已存为待办 ✓")
            return
        }
        panel.showError("剪贴板里没有可保存的文字或图片")
        panel.toggle()
    }

    // MARK: - 保存

    private func save(text: String, task: Bool, presentToast: Bool = true) {
        do {
            _ = try store.createText(text, task: task)
            if presentToast { panel.showToast(task ? "已存为待办 ✓" : "已保存 ✓") }
            itemsWindow?.refresh()
            triggerSync()
        } catch {
            panel.showError("保存失败：\(error.localizedDescription)")
        }
    }

    private func save(images: [Data], method: String = "drop", ocrText: String? = nil) {
        do {
            _ = try store.createImages(images, method: method, body: ocrText)
            panel.showToast(ocrText.map { _ in "已存为待办并提取文字 ✓" } ?? "已存为待办 ✓")
            itemsWindow?.refresh()
            triggerSync()
        } catch {
            panel.showError("图片保存失败：\(error.localizedDescription)")
        }
    }

    // MARK: - 热键

    private func installHotKeys() {
        let hotkeys = HotKeyManager()
        hotkeys.register(id: HotKeyManager.panel, keyCode: 46, modifiers: HotKeyManager.cmdOption) { [weak self] in self?.panel.toggle() }
        hotkeys.register(id: HotKeyManager.screenshot, keyCode: 1, modifiers: HotKeyManager.cmdOption) { [weak self] in self?.screenshot.begin() }
        hotkeys.register(id: HotKeyManager.clipboard, keyCode: 9, modifiers: HotKeyManager.cmdOption) { [weak self] in self?.saveClipboard() }
        hotkeys.register(id: 4, keyCode: 37, modifiers: HotKeyManager.cmdOption) { [weak self] in self?.openItems() }
    }

    // MARK: - 同步循环

    private func startSyncLoop() {
        syncTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.runSyncOnce()
                try? await Task.sleep(nanoseconds: 5_000_000_000)
            }
        }
    }

    private func runSyncOnce() async {
        guard let url = serverURL, let token else { return }
        let live = SyncEngine(store: store, api: APIClient(baseURL: url, token: token))
        do {
            _ = try await live.runOnce()
            await MainActor.run {
                updateSyncStatus()
                self.itemsWindow?.refresh()
            }
        } catch {
            await MainActor.run {
                statusItem.button?.toolTip = "同步失败：\(error.localizedDescription)"
            }
        }
    }

    func triggerSync() {
        Task { await runSyncOnce() }
    }

    @objc private func syncNow() { triggerSync() }

    private func updateSyncStatus() {
        let summary = engine.summary()
        statusItem.button?.toolTip = captureError ?? "AIMemorandum：\(summary)"
    }

    // MARK: - 提醒循环

    private func startReminderLoop() {
        Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            self?.checkReminders()
        }
    }

    private func checkReminders() {
        guard !screenshot.isCapturing else { return }
        rebuildMenu()
        guard let due = try? scheduler.due(), !due.isEmpty else { return }
        for entry in due {
            notifier.present(entry)
            try? scheduler.markFired(item: entry.item, reminder: entry.reminder)
        }
    }

    // MARK: - 服务端设置

    @objc private func openServerSetup() { showServerSetup(firstRun: false) }

    private func showServerSetup(firstRun: Bool) {
        let alert = NSAlert()
        alert.messageText = firstRun ? "连接同步服务" : "服务端设置"
        alert.informativeText = "填入服务地址与注册码（服务端启动时会打印注册码）。也可以暂时跳过，先离线使用。"
        alert.addButton(withTitle: "连接")
        alert.addButton(withTitle: "暂时离线")

        let container = NSView(frame: NSRect(x: 0, y: 0, width: 320, height: 76))
        let urlLabel = NSTextField(labelWithString: "服务地址")
        urlLabel.frame = NSRect(x: 0, y: 56, width: 320, height: 16)
        let urlField = NSTextField(string: (try? store.meta("server_url")) ?? "http://127.0.0.1:8787")
        urlField.frame = NSRect(x: 0, y: 34, width: 320, height: 22)
        let codeLabel = NSTextField(labelWithString: "注册码")
        codeLabel.frame = NSRect(x: 0, y: 16, width: 320, height: 14)
        let codeField = NSTextField(string: "")
        codeField.frame = NSRect(x: 0, y: -8, width: 320, height: 22)
        container.subviews = [urlLabel, urlField, codeLabel, codeField]
        alert.accessoryView = container

        let response = alert.runModal()
        guard response == .alertFirstButtonReturn else { return }
        guard let url = URL(string: urlField.stringValue.trimmingCharacters(in: .whitespaces)), url.scheme != nil else {
            panel.showError("服务地址无效")
            return
        }
        Task { [weak self] in
            guard let self else { return }
            do {
                let token = try await APIClient.enroll(baseURL: url, code: codeField.stringValue.trimmingCharacters(in: .whitespaces), deviceName: Host.current().localizedName ?? "mac")
                try? store.setMeta("server_url", value: url.absoluteString)
                try? store.setMeta("device_token", value: token)
                await MainActor.run { self.panel.showToast("已连接同步服务 ✓") }
                self.triggerSync()
            } catch {
                await MainActor.run { self.panel.showError("连接失败：\(error.localizedDescription)") }
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        syncTask?.cancel()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !panel.isVisible && !screenshot.isCapturing { panel.toggle() }
        return true
    }
}
