import AppKit
import MemorandumCore

// 快速记录面板：唯一的日常入口。文字 Enter 即存、⌘Enter 转待办、
// 图片拖进来直接保存、Esc 关闭。没有标题、分类、日期等前置步骤。
// 浅色主题 + 固定配色：在任何系统外观下文字始终清晰。
final class CapturePanel: NSPanel, NSTextViewDelegate, NSDraggingDestination {
    private let textView = NSTextView()
    private let hintLabel = NSTextField(labelWithString: "")
    private let toastPill = NSView()
    private let toastLabel = NSTextField(labelWithString: "")
    private var toastTimer: Timer?
    private var inputColor: NSColor = .black
    var onSaveText: ((String, Bool) -> Void)?
    var onSaveImages: (([Data], String) -> Void)?
    var onClose: (() -> Void)?

    // 浅色主题固定色板（与 PanelContentView 一致）
    static let surfaceColor = NSColor(calibratedRed: 0.965, green: 0.965, blue: 0.976, alpha: 1)
    static let borderColor = NSColor(calibratedRed: 0.86, green: 0.86, blue: 0.88, alpha: 1)
    private enum Theme {
        static let divider = NSColor(calibratedRed: 0.9, green: 0.9, blue: 0.92, alpha: 1)
        static let prompt = NSColor(calibratedRed: 0.36, green: 0.36, blue: 0.4, alpha: 1)
        static let hint = NSColor(calibratedRed: 0.6, green: 0.6, blue: 0.63, alpha: 1)
        static let inputText = NSColor(calibratedRed: 0.1, green: 0.1, blue: 0.12, alpha: 1)
        static let inputBg = NSColor.white
        static let toastBg = NSColor(calibratedRed: 0.88, green: 0.97, blue: 0.92, alpha: 1)
        static let toastText = NSColor(calibratedRed: 0.12, green: 0.48, blue: 0.27, alpha: 1)
        static let toastErrorBg = NSColor(calibratedRed: 0.99, green: 0.92, blue: 0.92, alpha: 1)
        static let toastErrorText = NSColor(calibratedRed: 0.72, green: 0.15, blue: 0.15, alpha: 1)
    }

    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 480, height: 276),
                   styleMask: [.titled, .fullSizeContentView],
                   backing: .buffered, defer: false)
        isReleasedWhenClosed = false
        isOpaque = false
        backgroundColor = .clear
        level = .floating
        isMovableByWindowBackground = true
        hidesOnDeactivate = true
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        // 强制浅色外观：面板用固定浅色配色，避免深色模式翻转控件颜色
        appearance = NSAppearance(named: .aqua)
        registerForDraggedTypes([.png, .tiff, .fileURL, .string])

        let content = PanelContentView(frame: NSRect(x: 0, y: 0, width: 480, height: 276))
        contentView = content

        let prompt = NSTextField(labelWithString: "记点什么")
        prompt.font = NSFont.systemFont(ofSize: 13, weight: .semibold)
        prompt.textColor = Theme.prompt

        let shortcuts = NSTextField(labelWithString: "⏎ 存为待办")
        shortcuts.font = NSFont.systemFont(ofSize: 11)
        shortcuts.textColor = Theme.hint
        shortcuts.alignment = .right

        let divider = NSBox()
        divider.boxType = .separator
        divider.translatesAutoresizingMaskIntoConstraints = false

        let scroll = NSScrollView(frame: .zero)
        scroll.hasVerticalScroller = true
        scroll.borderType = .noBorder
        scroll.drawsBackground = false
        let inputFont = NSFont.systemFont(ofSize: 15)
        let inputColor = Theme.inputText
        self.inputColor = inputColor
        textView.isRichText = false
        textView.font = inputFont
        textView.textColor = inputColor
        // 打字颜色由两处决定，缺一不可：
        // typingAttributes → 直接输入/英文；markedTextAttributes → 中文输入法组字（IME）。
        textView.typingAttributes = [.font: inputFont, .foregroundColor: inputColor]
        textView.markedTextAttributes = [
            .font: inputFont,
            .foregroundColor: inputColor,
            .underlineStyle: NSUnderlineStyle.single.rawValue,
        ]
        textView.insertionPointColor = NSColor.systemBlue
        textView.drawsBackground = true
        textView.backgroundColor = Theme.inputBg
        textView.delegate = self
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.textContainer?.widthTracksTextView = true
        // 关键：documentView 必须跟随 scroll 宽度拉伸，否则宽度为 0，文字全部挤进缝隙不可见。
        textView.autoresizingMask = [.width]
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        scroll.documentView = textView

        hintLabel.font = NSFont.systemFont(ofSize: 11)
        hintLabel.textColor = Theme.hint
        hintLabel.stringValue = "拖入图片直接存 · Esc 关闭"

        // 收条：底部居中的胶囊提示
        toastPill.wantsLayer = true
        toastPill.layer?.cornerRadius = 13
        toastPill.layer?.backgroundColor = Theme.toastBg.cgColor
        toastPill.translatesAutoresizingMaskIntoConstraints = false
        toastLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        toastLabel.textColor = Theme.toastText
        toastLabel.translatesAutoresizingMaskIntoConstraints = false
        toastPill.addSubview(toastLabel)
        toastPill.isHidden = true
        NSLayoutConstraint.activate([
            toastLabel.leadingAnchor.constraint(equalTo: toastPill.leadingAnchor, constant: 14),
            toastLabel.trailingAnchor.constraint(equalTo: toastPill.trailingAnchor, constant: -14),
            toastLabel.centerYAnchor.constraint(equalTo: toastPill.centerYAnchor),
            toastPill.heightAnchor.constraint(equalToConstant: 26),
        ])

        for view in [prompt, shortcuts, scroll, hintLabel] {
            view.translatesAutoresizingMaskIntoConstraints = false
        }
        content.addSubview(prompt)
        content.addSubview(shortcuts)
        content.addSubview(divider)
        content.addSubview(scroll)
        content.addSubview(hintLabel)
        content.addSubview(toastPill)
        NSLayoutConstraint.activate([
            prompt.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            prompt.topAnchor.constraint(equalTo: content.topAnchor, constant: 15),
            shortcuts.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            shortcuts.centerYAnchor.constraint(equalTo: prompt.centerYAnchor),
            divider.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            divider.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            divider.topAnchor.constraint(equalTo: prompt.bottomAnchor, constant: 10),
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            scroll.topAnchor.constraint(equalTo: divider.bottomAnchor, constant: 10),
            scroll.heightAnchor.constraint(equalToConstant: 156),
            hintLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            hintLabel.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -12),
            toastPill.centerXAnchor.constraint(equalTo: content.centerXAnchor),
            toastPill.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -40),
        ])
    }

    required init?(coder: NSCoder) { fatalError("不支持") }

    override var canBecomeKey: Bool { true }

    func toggle(centeredOn screen: NSScreen? = nil) {
        if isVisible { orderOut(nil); return }
        let target = screen ?? NSScreen.main
        let frame = target?.visibleFrame ?? NSRect(x: 0, y: 0, width: 480, height: 276)
        setFrameOrigin(NSPoint(x: frame.midX - 240, y: frame.maxY - 356))
        makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
        textView.window?.makeFirstResponder(textView)
        toastPill.isHidden = true
    }

    // MARK: - 输入提交

    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(insertNewline(_:)) {
            submit()
            return true
        }
        return false
    }

    private func submit() {
        // 统一存为待办：产品决策不再区分"记录"与"待办"，存进来的一切都是要处理的事。
        let isTask = true
        let text = textView.string
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || pendingDropImages.isEmpty == false else { return }
        if !pendingDropImages.isEmpty {
            onSaveImages?(pendingDropImages, isTask ? "drop_task" : "drop")
            pendingDropImages.removeAll()
        } else {
            onSaveText?(text, isTask)
        }
        resetInput()
        showToast("已存为待办 ✓")
    }

    // MARK: - 拖入图片

    private var pendingDropImages: [Data] = []

    func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        return readImages(from: sender) != nil ? .copy : []
    }

    func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard let images = readImages(from: sender) else { return false }
        onSaveImages?(images, "drop")
        resetInput()
        showToast("已保存 \(images.count) 张图片")
        return true
    }

    private func readImages(from sender: NSDraggingInfo) -> [Data]? {
        let pasteboard = sender.draggingPasteboard
        var result: [Data] = []
        if let files = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] {
            for url in files where ["png", "jpg", "jpeg"].contains(url.pathExtension.lowercased()) {
                if let data = try? Data(contentsOf: url) { result.append(data) }
            }
        }
        if result.isEmpty, let png = pasteboard.data(forType: .png) { result.append(png) }
        if result.isEmpty, let tiff = pasteboard.data(forType: .tiff),
           let rep = NSBitmapImageRep(data: tiff),
           let png = rep.representation(using: .png, properties: [:]) { result.append(png) }
        return result.isEmpty ? nil : result
    }

    // MARK: - 收条

    func showToast(_ message: String) {
        showPill(message, background: Theme.toastBg, text: Theme.toastText)
    }

    func showError(_ message: String) {
        showPill(message, background: Theme.toastErrorBg, text: Theme.toastErrorText)
    }

    private func showPill(_ message: String, background: NSColor, text: NSColor) {
        toastPill.layer?.backgroundColor = background.cgColor
        toastLabel.textColor = text
        toastLabel.stringValue = message
        toastPill.isHidden = false
        toastTimer?.invalidate()
        toastTimer = Timer.scheduledTimer(withTimeInterval: 2.2, repeats: false) { [weak self] _ in
            self?.toastPill.isHidden = true
        }
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // Esc
            orderOut(nil)
            onClose?()
            return
        }
        super.keyDown(with: event)
    }

    override func resignMain() {
        super.resignMain()
        // 面板失焦自动收起，保持"一闪即走"的节奏。
        orderOut(nil)
        onClose?()
    }

    func resetInput() {
        textView.string = ""
        textView.typingAttributes = [
            .font: NSFont.systemFont(ofSize: 15),
            .foregroundColor: inputColor,
        ]
    }

    /// 兜底：任何路径漏进来的白色前景统一改回深色（不影响组字下划线）。
    func textDidChange(_ notification: Notification) {
        guard let storage = textView.textStorage, storage.length > 0 else { return }
        storage.addAttribute(.foregroundColor, value: inputColor, range: NSRange(location: 0, length: storage.length))
    }

    /// 诊断探针访问入口。
    var debugTextView: NSTextView { textView }
}

/// 面板背景：固定浅色圆角卡片 + 细描边，窗口自带阴影。
final class PanelContentView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        let path = NSBezierPath(roundedRect: bounds, xRadius: 14, yRadius: 14)
        CapturePanel.surfaceColor.setFill()
        path.fill()
        CapturePanel.borderColor.setStroke()
        path.lineWidth = 1
        path.stroke()
    }
    override var acceptsFirstResponder: Bool { false }
}
