import AppKit
import ScreenCaptureKit
import Vision
import MemorandumCore

// 截图记录：热键 → 全屏遮罩框选 → 松手自动保存。
// 需要"屏幕录制"权限；首次使用时引导到系统设置，未授权期间保留
// "系统截图（⌘⇧4）后粘贴/拖入"的可用路径。
final class ScreenshotCoordinator {
    /// 回调携带识别出的文字（本机 Vision OCR，可能为 nil）。
    var onSaved: (([Data], String?) -> Void)?
    var onError: ((String) -> Void)?
    var onWillBegin: (() -> Void)?

    private var overlay: OverlayWindow?
    private(set) var isCapturing = false
    private let hasScreenCaptureAccess: () -> Bool

    init(hasScreenCaptureAccess: @escaping () -> Bool = { CGPreflightScreenCaptureAccess() }) {
        self.hasScreenCaptureAccess = hasScreenCaptureAccess
    }

    func begin() {
        guard !isCapturing, overlay == nil else { return }
        guard hasScreenCaptureAccess() else {
            CGRequestScreenCaptureAccess()
            openScreenRecordingSettings()
            onError?("截图需要\"屏幕录制\"权限：已打开系统设置，授权后重试；也可以 ⌘⇧4 系统截图后拖入面板保存")
            return
        }
        // 在包含鼠标指针的屏幕上启动框选。
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { $0.frame.contains(mouse) } ?? NSScreen.main
        guard let screen else { return }
        isCapturing = true
        onWillBegin?()
        let overlayWindow = OverlayWindow(screen: screen)
        overlayWindow.selectionHandler = { [weak self] rect in
            // 不在事件分发中同步释放窗口，挪到下一轮 RunLoop。
            DispatchQueue.main.async { self?.overlay = nil }
            guard rect.width >= 8, rect.height >= 8 else {
                self?.isCapturing = false
                return
            } // 点一下视为取消
            self?.capture(screen: screen, rect: rect)
        }
        overlayWindow.cancelHandler = { [weak self] in
            self?.isCapturing = false
            DispatchQueue.main.async { self?.overlay = nil }
        }
        overlay = overlayWindow
        overlayWindow.orderFrontRegardless()
        overlayWindow.makeKey()
        overlayWindow.makeFirstResponder(overlayWindow.contentView)
    }

    // 供诊断探针模拟 Esc 取消。
    func debugCancel() { overlay?.cancelSelection() }
    var debugHasOverlay: Bool { overlay != nil }

    private func openScreenRecordingSettings() {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")!
        NSWorkspace.shared.open(url)
    }

    private func capture(screen: NSScreen, rect: NSRect) {
        Task { [weak self] in
            do {
                let data = try await Self.capturePNG(screen: screen, rect: rect)
                // OCR 在后台执行，失败不阻塞保存（识别是增强，不是保存的前置条件）。
                let text = await Self.ocrText(from: data)
                await MainActor.run {
                    self?.isCapturing = false
                    self?.onSaved?([data], text)
                }
            } catch {
                await MainActor.run {
                    self?.isCapturing = false
                    self?.onError?("截图失败：\(error.localizedDescription)")
                }
            }
        }
    }

    /// 本机文字识别：中文简体 + 英文，按版面从上到下、从左到右拼接成行。
    static func ocrText(from data: Data) async -> String? {
        guard data.count <= 20971520,
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let cgImage = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        return await withCheckedContinuation { continuation in
            let request = VNRecognizeTextRequest { request, _ in
                guard let observations = request.results as? [VNRecognizedTextObservation], !observations.isEmpty else {
                    continuation.resume(returning: nil)
                    return
                }
                // 同一行的多个块按 x 拼接；不同行按 y 排序。
                let sorted = observations.sorted { a, b in
                    if abs(a.boundingBox.midY - b.boundingBox.midY) > 0.02 {
                        return a.boundingBox.midY > b.boundingBox.midY
                    }
                    return a.boundingBox.minX < b.boundingBox.minX
                }
                var lines: [(y: CGFloat, text: String)] = []
                for observation in sorted {
                    guard let candidate = observation.topCandidates(1).first else { continue }
                    if var last = lines.last, abs(last.y - observation.boundingBox.midY) <= 0.02 {
                        lines[lines.count - 1].text += " " + candidate.string
                    } else {
                        lines.append((y: observation.boundingBox.midY, text: candidate.string))
                    }
                }
                let text = lines.map(\.text).joined(separator: "\n")
                continuation.resume(returning: text.isEmpty ? nil : text)
            }
            request.recognitionLanguages = ["zh-Hans", "en-US"]
            request.recognitionLevel = .accurate
            request.usesLanguageCorrection = true
            let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
            do {
                try handler.perform([request])
            } catch {
                continuation.resume(returning: nil)
            }
        }
    }

    static func capturePNG(screen: NSScreen, rect: NSRect) async throws -> Data {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let displayID = (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value,
              let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw StoreError.message("未找到对应显示器")
        }
        // 即使系统尚未完成窗口隐藏，也不能截入本应用的选框、记录窗口或提示。
        let ownWindows = content.windows.filter { $0.owningApplication?.processID == ProcessInfo.processInfo.processIdentifier }
        let filter = SCContentFilter(display: display, excludingWindows: ownWindows)
        let config = SCStreamConfiguration()
        config.showsCursor = false
        let scale = CGFloat(filter.pointPixelScale)
        guard let sourceRect = ScreenshotGeometry.sourceRect(localSelection: rect, screenSize: screen.frame.size, scale: scale) else {
            throw StoreError.message("截图区域无效，请重新框选")
        }
        config.sourceRect = sourceRect
        config.width = max(1, Int((sourceRect.width * scale).rounded()))
        config.height = max(1, Int((sourceRect.height * scale).rounded()))
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        // 防截屏保护（企业微信等企业应用常见）会让截到的内容变成纯色空白层；
        // 保存前检测，给出明确解释而不是存一张废图。
        if Self.isBlankCapture(image) {
            throw StoreError.message("截取到的内容是一片空白：该窗口可能开启了防截屏保护（企业微信等企业应用常见），系统层面无法绕过。可先用 ⌘⇧4 系统截图验证，或改用文字记录")
        }
        let rep = NSBitmapImageRep(cgImage: image)
        rep.size = NSSize(width: rect.width, height: rect.height)
        guard let png = rep.representation(using: .png, properties: [:]) else {
            throw StoreError.message("无法编码 PNG")
        }
        return png
    }

    /// 采样网格像素：若全部采样点几乎同色，判定为防截屏保护输出的空白层。
    static func isBlankCapture(_ image: CGImage) -> Bool {
        let width = image.width, height = image.height
        guard width > 0, height > 0,
              image.bitsPerPixel >= 24,
              let data = image.dataProvider?.data,
              let ptr = CFDataGetBytePtr(data) else { return false }
        let bytesPerPixel = image.bitsPerPixel / 8
        let bytesPerRow = image.bytesPerRow
        var first: (r: Int, g: Int, b: Int)?
        let stepX = max(1, width / 32), stepY = max(1, height / 32)
        for y in stride(from: 0, to: height, by: stepY) {
            for x in stride(from: 0, to: width, by: stepX) {
                let offset = y * bytesPerRow + x * bytesPerPixel
                let r = Int(ptr[offset]), g = Int(ptr[offset + 1]), b = Int(ptr[offset + 2])
                if let f = first {
                    if abs(r - f.r) > 8 || abs(g - f.g) > 8 || abs(b - f.b) > 8 { return false }
                } else {
                    first = (r, g, b)
                }
            }
        }
        return true
    }
}

/// 全屏半透明遮罩 + 框选视图。
final class OverlayWindow: NSPanel {
    var selectionHandler: ((NSRect) -> Void)?
    var cancelHandler: (() -> Void)?
    private var finished = false

    init(screen: NSScreen) {
        // 非激活面板可以接收框选和 Esc，不会把“我的记录”等其他窗口带到前台。
        super.init(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isReleasedWhenClosed = false // 关键：程序化创建的窗口由 ARC 管理，close() 不能再释放一次
        level = .screenSaver
        isOpaque = false
        backgroundColor = .clear
        ignoresMouseEvents = false
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        let view = SelectionView(frame: NSRect(origin: .zero, size: screen.frame.size))
        view.onComplete = { [weak self] rect in self?.completeSelection(rect) }
        view.onCancel = { [weak self] in self?.cancelSelection() }
        contentView = view
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func completeSelection(_ rect: NSRect) {
        guard !finished else { return }
        finished = true
        orderOut(nil)
        close()
        selectionHandler?(rect)
    }

    func cancelSelection() {
        guard !finished else { return }
        finished = true
        orderOut(nil)
        close()
        cancelHandler?()
    }
}

final class SelectionView: NSView {
    var onComplete: ((NSRect) -> Void)?
    var onCancel: (() -> Void)?
    private var startPoint: NSPoint?
    private var currentRect: NSRect = .zero

    override init(frame: NSRect) {
        super.init(frame: frame)
        let tracking = NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeAlways], owner: self)
        addTrackingArea(tracking)
    }
    required init?(coder: NSCoder) { fatalError("不支持") }
    override var acceptsFirstResponder: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.black.withAlphaComponent(0.25).setFill()
        bounds.fill()
        if !NSIsEmptyRect(currentRect) {
            NSColor.systemBlue.withAlphaComponent(0.15).setFill()
            NSBezierPath(rect: currentRect).fill()
            NSColor.controlAccentColor.setStroke()
            let border = NSBezierPath(rect: currentRect.insetBy(dx: 0.5, dy: 0.5))
            border.lineWidth = 1.5
            border.stroke()
        }
    }

    override func mouseDown(with event: NSEvent) { startPoint = convert(event.locationInWindow, from: nil) }

    override func mouseDragged(with event: NSEvent) {
        updateSelection(with: event)
    }

    private func updateSelection(with event: NSEvent) {
        guard let start = startPoint else { return }
        let now = convert(event.locationInWindow, from: nil)
        currentRect = NSRect(x: min(start.x, now.x), y: min(start.y, now.y),
                             width: abs(start.x - now.x), height: abs(start.y - now.y)).intersection(bounds)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        updateSelection(with: event)
        onComplete?(currentRect)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { onCancel?() } // Esc
    }

    override func resetCursorRects() { addCursorRect(bounds, cursor: .crosshair) }
}
