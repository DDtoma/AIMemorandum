import AppKit
import MemorandumCore

// 提醒弹窗：提醒到期时在屏幕右上角出现，是"事项再次出现"的载体。
// 关闭通知不代表完成；只有点"完成"才结束事项。窗体自绘，避免依赖
// 未打包环境下的 UserNotifications 行为差异。
final class ReminderNotifier {
    struct Actions {
        let complete: (DueReminder) -> Void
        let snooze: (DueReminder, TimeInterval) -> Void
        let dismiss: (DueReminder) -> Void
    }

    private let actions: Actions
    private var windows: [NSPanel] = []
    private var contexts: [ObjectIdentifier: Context] = [:]

    init(actions: Actions) {
        self.actions = actions
    }

    func present(_ due: DueReminder) {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 340, height: 132),
                            styleMask: [.titled, .fullSizeContentView],
                            backing: .buffered, defer: false)
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isReleasedWhenClosed = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.level = NSWindow.Level(rawValue: Int(CGShieldingWindowLevel())) // 高于浮动窗，低于系统遮挡层
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        // 强制浅色外观：深色模式下按钮/标签会画成白字而隐形
        panel.appearance = NSAppearance(named: .aqua)

        let content = PanelContentView(frame: NSRect(x: 0, y: 0, width: 340, height: 132))

        let title = NSTextField(labelWithString: "⏰ 提醒")
        title.font = .boldSystemFont(ofSize: 11)
        title.textColor = NSColor(calibratedWhite: 0.35, alpha: 1)

        let body = NSTextField(wrappingLabelWithString: due.item.title)
        body.font = .systemFont(ofSize: 13, weight: .medium)
        body.maximumNumberOfLines = 2

        let time = NSTextField(labelWithString: DateFormatter.localizedString(from: due.remindAt, dateStyle: .none, timeStyle: .short))
        time.font = .systemFont(ofSize: 10)
        time.textColor = NSColor(calibratedWhite: 0.55, alpha: 1)

        let completeButton = NSButton(title: "完成", target: nil, action: nil)
        completeButton.bezelStyle = .rounded
        completeButton.controlSize = .small
        completeButton.keyEquivalent = "\r"
        completeButton.target = self
        completeButton.action = #selector(didComplete(_:))

        let snoozeButton = NSButton(title: "稍后 10 分钟", target: self, action: #selector(didSnooze(_:)))
        snoozeButton.bezelStyle = .rounded
        snoozeButton.controlSize = .small

        let dismissButton = NSButton(title: "知道了", target: self, action: #selector(didDismiss(_:)))
        dismissButton.bezelStyle = .rounded
        dismissButton.controlSize = .small

        let buttons = NSStackView(views: [completeButton, snoozeButton, dismissButton])
        buttons.orientation = .horizontal
        buttons.spacing = 6
        buttons.translatesAutoresizingMaskIntoConstraints = false

        let stack = NSStackView(views: [title, body, buttons])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 5
        stack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 14),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -14),
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 12),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -12),
        ])
        panel.contentView = content

        // 依次堆叠在屏幕右上角。
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let offset = CGFloat(windows.count) * 144
        panel.setFrameTopLeftPoint(NSPoint(x: screen.maxX - 356, y: screen.maxY - 40 - offset))

        let context = Context(panel: panel, due: due)
        contexts[ObjectIdentifier(completeButton)] = context
        contexts[ObjectIdentifier(snoozeButton)] = context
        contexts[ObjectIdentifier(dismissButton)] = context

        windows.append(panel)
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private struct Context { let panel: NSPanel; let due: DueReminder }

    @objc private func didComplete(_ sender: NSButton) {
        guard let context = contexts[ObjectIdentifier(sender)] else { return }
        finish(context.panel)
        actions.complete(context.due)
    }

    @objc private func didSnooze(_ sender: NSButton) {
        guard let context = contexts[ObjectIdentifier(sender)] else { return }
        finish(context.panel)
        actions.snooze(context.due, 600)
    }

    @objc private func didDismiss(_ sender: NSButton) {
        guard let context = contexts[ObjectIdentifier(sender)] else { return }
        finish(context.panel)
        actions.dismiss(context.due)
    }

    private func finish(_ panel: NSPanel) {
        panel.orderOut(nil)
        windows.removeAll { $0 === panel }
    }
}
