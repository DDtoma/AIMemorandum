import AppKit
import MemorandumCore

/// 独立诊断进程：只截取另一个测试进程绘制的色块，不保存用户屏幕内容。
enum CaptureRegressionProbe {
    private static var fixtureWindows: [NSWindow] = []

    static func runIfRequested() -> Bool {
        let env = ProcessInfo.processInfo.environment
        guard let mode = env["MEMO_PROBE"], ["capture-fixture", "capture-regression"].contains(mode),
              let path = env["MEMO_CAPTURE_PROBE_ROOT"] else { return false }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let root = URL(fileURLWithPath: path)
        if mode == "capture-fixture" {
            var fixtures: [[String: Any]] = []
            for screen in NSScreen.screens {
                guard let id = (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value else { continue }
                let area = screen.visibleFrame
                let size = CGSize(width: min(640, area.width * 0.75), height: min(480, area.height * 0.75))
                let frame = CGRect(x: area.midX - size.width / 2, y: area.midY - size.height / 2, width: size.width, height: size.height)
                let window = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
                window.isReleasedWhenClosed = false
                window.hidesOnDeactivate = false
                window.level = .floating
                window.title = "截图回归测试色块"
                window.contentView = ColorBandsView(frame: CGRect(origin: .zero, size: size))
                window.orderFrontRegardless()
                fixtureWindows.append(window)
                fixtures.append(["display_id": id, "frame": NSStringFromRect(frame)])
            }
            try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try? JSONSerialization.data(withJSONObject: fixtures).write(to: root.appendingPathComponent("fixtures.json"), options: .atomic)
            app.run()
            return true
        }

        let helper = Process()
        helper.executableURL = URL(fileURLWithPath: CommandLine.arguments[0])
        var helperEnv = env
        helperEnv["MEMO_PROBE"] = "capture-fixture"
        helper.environment = helperEnv
        do {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try? FileManager.default.removeItem(at: root.appendingPathComponent("fixtures.json"))
            try helper.run()
        } catch {
            print("FAIL fixture start: \(error)")
            exit(1)
        }
        Task { @MainActor in
            defer { if helper.isRunning { helper.terminate() } }
            do {
                for _ in 0..<50 {
                    if FileManager.default.fileExists(atPath: root.appendingPathComponent("fixtures.json").path) { break }
                    try await Task.sleep(nanoseconds: 100_000_000)
                }
                let data = try Data(contentsOf: root.appendingPathComponent("fixtures.json"))
                guard let fixtures = try JSONSerialization.jsonObject(with: data) as? [[String: Any]], !fixtures.isEmpty else { throw StoreError.message("未生成测试显示器") }
                try await Task.sleep(nanoseconds: 250_000_000)

                // 真正显示记录窗口，再调用与热键相同的 begin/取消路径。
                let records = NSWindow(contentRect: CGRect(x: 20, y: 20, width: 160, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
                records.isReleasedWhenClosed = false
                records.title = "我的记录 · 回归测试"
                records.orderFrontRegardless()
                let activeBefore = app.isActive
                // 只测试框选与取消，不调用抓屏 API，因此无需扩大诊断进程的屏幕权限。
                let coordinator = ScreenshotCoordinator(hasScreenCaptureAccess: { true })
                coordinator.onWillBegin = { records.orderOut(nil) }
                coordinator.begin()
                print("PROBE overlay=\(coordinator.debugHasOverlay) capturing=\(coordinator.isCapturing) records-visible=\(records.isVisible) active-before=\(activeBefore) active-after=\(app.isActive)")
                // CLI 诊断进程首次取得 key window 时，AppKit 可改变应用激活状态；
                // 验收的是记录窗口不能弹回或遮挡选区，而不是进程的 isActive 值。
                guard coordinator.debugHasOverlay, coordinator.isCapturing, !records.isVisible else {
                    throw StoreError.message("没有进入框选，或记录窗口没有隐藏")
                }
                print("PASS AC-MAC-001 records hidden throughout selection")
                coordinator.debugCancel()
                try await Task.sleep(nanoseconds: 100_000_000)
                guard !coordinator.debugHasOverlay, !coordinator.isCapturing, !records.isVisible else { throw StoreError.message("取消后框选窗口或记录窗口重新出现") }
                records.close()
                print("PASS AC-MAC-001 cancel closes overlay without restoring records")

                guard CGPreflightScreenCaptureAccess() else {
                    print("SKIP live pixel capture: macOS has not authorized this test process for screen recording")
                    if helper.isRunning { helper.terminate(); helper.waitUntilExit() }
                    exit(2)
                }

                let store = try LocalStore(root: root.appendingPathComponent("store"))
                let initialItems = try store.items().count
                let initialPending = try store.pending().count
                var captures = 0
                for fixture in fixtures {
                    guard let id = (fixture["display_id"] as? NSNumber)?.uint32Value,
                          let screen = NSScreen.screens.first(where: { ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value == id }),
                          let frameString = fixture["frame"] as? String else { throw StoreError.message("显示器映射失败") }
                    let frame = NSRectFromString(frameString)
                    for (name, y, expected) in [("top", CGFloat(0.75), (0.90, 0.10, 0.15)), ("bottom", CGFloat(0), (0.95, 0.80, 0.10))] {
                        let selection = CGRect(x: frame.minX - screen.frame.minX + 10,
                                               y: frame.minY - screen.frame.minY + frame.height * y + 10,
                                               width: frame.width * 0.6, height: frame.height * 0.2)
                        let png = try await ScreenshotCoordinator.capturePNG(screen: screen, rect: selection)
                        guard let image = NSBitmapImageRep(data: png),
                              let color = image.colorAt(x: image.pixelsWide / 2, y: image.pixelsHigh / 2)?.usingColorSpace(.sRGB),
                              abs(Double(color.redComponent) - expected.0) < 0.16,
                              abs(Double(color.greenComponent) - expected.1) < 0.16,
                              abs(Double(color.blueComponent) - expected.2) < 0.16 else { throw StoreError.message("\(id) \(name) 截到了错误色块") }
                        guard let sourceRect = ScreenshotGeometry.sourceRect(localSelection: selection, screenSize: screen.frame.size, scale: screen.backingScaleFactor),
                              image.pixelsWide == Int((sourceRect.width * screen.backingScaleFactor).rounded()),
                              image.pixelsHigh == Int((sourceRect.height * screen.backingScaleFactor).rounded()) else { throw StoreError.message("像素尺寸不匹配") }
                        _ = try store.createImages([png], method: "screenshot")
                        captures += 1
                        print("PASS AC-CAP-002 display=\(id) \(name) selected pixels match, \(image.pixelsWide)x\(image.pixelsHigh), saved to SQLite")
                    }
                }
                guard try store.items().count == initialItems + captures, try store.pending().count == initialPending + captures else { throw StoreError.message("截图本地保存或待同步操作数量不一致") }
                print("PASS native capture regression: \(fixtures.count) display(s), \(captures) captures")
                if helper.isRunning { helper.terminate(); helper.waitUntilExit() }
                exit(0)
            } catch {
                print("FAIL capture regression: \(error.localizedDescription)")
                if helper.isRunning { helper.terminate(); helper.waitUntilExit() }
                exit(1)
            }
        }
        app.run()
        return true
    }
}

private final class ColorBandsView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        let colors = [NSColor(srgbRed: 0.95, green: 0.80, blue: 0.10, alpha: 1),
                      NSColor(srgbRed: 0.12, green: 0.25, blue: 0.90, alpha: 1),
                      NSColor(srgbRed: 0.15, green: 0.80, blue: 0.30, alpha: 1),
                      NSColor(srgbRed: 0.90, green: 0.10, blue: 0.15, alpha: 1)]
        for (index, color) in colors.enumerated() {
            color.setFill()
            CGRect(x: 0, y: bounds.height * CGFloat(index) / 4, width: bounds.width, height: bounds.height / 4).fill()
        }
        NSColor.white.setFill()
        CGRect(x: 40, y: 0, width: 20, height: bounds.height).fill()
    }
}
