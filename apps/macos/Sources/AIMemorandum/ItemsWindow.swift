import AppKit
import MemorandumCore

// 可换行标签：AppKit 的多行 NSTextField 必须设置 preferredMaxLayoutWidth 才能正确折行。
final class WrappingLabel: NSTextField {
    override func layout() {
        preferredMaxLayoutWidth = max(bounds.width, 1)
        super.layout()
    }
}

// 记录详情面板：纯 frame 布局（不使用 Auto Layout），
// 标题/徽标/元信息/缩略图固定顶部，正文用可滚动的 NSTextView 填充剩余空间。
final class DetailView: NSView {
    private let titleLabel = WrappingLabel()
    private let badgeLabel = NSTextField(labelWithString: "")
    private let metaLabel = NSTextField(labelWithString: "")
    private let reminderLabel = NSTextField(labelWithString: "")
    private let thumbsStrip = NSView()
    private let bodyScroll = NSScrollView()
    private let bodyText = NSTextView()
    private let emptyHint = NSTextField(labelWithString: "")
    private var attachmentByView: [NSImageView: Attachment] = [:]
    private var imageCache: [String: NSImage] = [:]
    var onOpenAttachment: ((Attachment) -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        titleLabel.font = NSFont.systemFont(ofSize: 17, weight: .semibold)
        titleLabel.textColor = ItemsWindow.Theme.title
        // 标题可编辑：单行输入框样式（无边框、白底、无聚焦环）
        titleLabel.isEditable = true
        titleLabel.isBezeled = false
        titleLabel.drawsBackground = true
        titleLabel.backgroundColor = NSColor.white
        titleLabel.focusRingType = .none
        titleLabel.usesSingleLineMode = true
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1

        badgeLabel.font = NSFont.systemFont(ofSize: 11)
        badgeLabel.lineBreakMode = .byTruncatingTail
        badgeLabel.drawsBackground = false

        metaLabel.font = NSFont.systemFont(ofSize: 11)
        metaLabel.textColor = ItemsWindow.Theme.secondary
        metaLabel.lineBreakMode = .byTruncatingTail

        reminderLabel.font = NSFont.systemFont(ofSize: 12)
        reminderLabel.textColor = ItemsWindow.Theme.orangeText
        reminderLabel.lineBreakMode = .byTruncatingTail

        emptyHint.stringValue = "在左侧选择一条记录，这里会显示完整内容"
        emptyHint.font = NSFont.systemFont(ofSize: 13)
        emptyHint.textColor = ItemsWindow.Theme.secondary
        emptyHint.alignment = .center

        bodyText.isEditable = true
        bodyText.isSelectable = true
        bodyText.font = NSFont.systemFont(ofSize: 13)
        bodyText.textColor = ItemsWindow.Theme.body
        bodyText.backgroundColor = NSColor.white
        bodyText.drawsBackground = true
        bodyText.insertionPointColor = ItemsWindow.Theme.body
        bodyText.textContainerInset = NSSize(width: 6, height: 6)
        bodyText.isVerticallyResizable = true
        bodyText.isHorizontallyResizable = false
        bodyText.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        bodyText.textContainer?.widthTracksTextView = true
        bodyText.textContainer?.containerSize = NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        bodyScroll.documentView = bodyText
        bodyScroll.hasVerticalScroller = true
        bodyScroll.autohidesScrollers = true
        bodyScroll.drawsBackground = false
        bodyScroll.borderType = .noBorder

        for view in [titleLabel, badgeLabel, metaLabel, reminderLabel, thumbsStrip, bodyScroll, emptyHint] {
            addSubview(view)
        }
    }

    required init?(coder: NSCoder) { fatalError("不支持") }

    // MARK: 内容填充

    func setItem(_ item: Item?, store: LocalStore) {
        for view in thumbsStrip.subviews { view.removeFromSuperview() }
        attachmentByView = [:]
        guard let item else {
            emptyHint.isHidden = false
            titleLabel.isHidden = true
            badgeLabel.isHidden = true
            metaLabel.isHidden = true
            reminderLabel.isHidden = true
            thumbsStrip.isHidden = true
            bodyScroll.isHidden = true
            return
        }
        emptyHint.isHidden = true
        titleLabel.isHidden = false
        badgeLabel.isHidden = false
        metaLabel.isHidden = false
        thumbsStrip.isHidden = item.attachments.isEmpty
        titleLabel.isEditable = item.deletedAt == nil
        bodyText.isEditable = item.deletedAt == nil

        titleLabel.stringValue = item.title.isEmpty ? "（无标题）" : item.title

        // 徽标行：富文本背景色模拟胶囊（不再区分"记录/待办"，一律待办）
        let (stateText, stateColor, stateSoft) = ItemsWindow.stateBadge(item)
        var parts: [NSAttributedString] = [Self.badge(stateText, fg: stateColor, bg: stateSoft)]
        let ready = ((try? store.synchronized(item)) ?? false)
        parts.append(Self.badge(ready ? "已同步" : "待同步",
                                fg: ready ? ItemsWindow.Theme.secondary : ItemsWindow.Theme.orangeText,
                                bg: ready ? ItemsWindow.Theme.graySoft : ItemsWindow.Theme.orangeSoft))
        if !item.attachments.isEmpty {
            parts.append(Self.badge("附件 \(item.attachments.count)",
                                    fg: ItemsWindow.Theme.secondary, bg: ItemsWindow.Theme.graySoft))
        }
        let joined = NSMutableAttributedString(string: "")
        for (index, part) in parts.enumerated() {
            if index > 0 { joined.append(NSAttributedString(string: "  ")) }
            joined.append(part)
        }
        badgeLabel.attributedStringValue = joined

        // 元信息
        var meta = ["创建于 \(ItemsWindow.fullTime(item.createdAt))"]
        if let completedAt = item.completedAt, !completedAt.isEmpty {
            meta.append("完成于 \(ItemsWindow.fullTime(completedAt))")
        } else if item.updatedAt != item.createdAt {
            meta.append("更新于 \(ItemsWindow.fullTime(item.updatedAt))")
        }
        if item.titleOrigin == "generated" { meta.append("标题自动生成") }
        metaLabel.stringValue = meta.joined(separator: " · ")

        // 提醒
        let reminders = item.activeReminders
        reminderLabel.isHidden = reminders.isEmpty
        if !reminders.isEmpty {
            let times = reminders.compactMap { Wire.date($0.remindAt) }
                .sorted()
                .map { ItemsWindow.fullTime(Wire.timestamp($0)) }
            reminderLabel.stringValue = "⏰ 提醒：" + times.joined(separator: "、")
        }

        // 截图缩略图（最多 3 张，单击打开原图）
        for attachment in item.attachments.prefix(3) {
            let imageView = NSImageView(frame: NSRect(x: 0, y: 0, width: 128, height: 96))
            imageView.wantsLayer = true
            imageView.layer?.cornerRadius = 6
            imageView.layer?.borderWidth = 1
            imageView.layer?.borderColor = ItemsWindow.Theme.divider.cgColor
            imageView.imageScaling = .scaleProportionallyDown
            imageView.imageAlignment = .alignCenter
            let click = NSClickGestureRecognizer(target: self, action: #selector(openAttachment(_:)))
            imageView.addGestureRecognizer(click)
            attachmentByView[imageView] = attachment
            thumbsStrip.addSubview(imageView)
            loadThumbnail(attachment, into: imageView)
        }

        // 正文（OCR 文字 / 原始文字）
        let body = item.bodyMd.trimmingCharacters(in: .whitespacesAndNewlines)
        bodyScroll.isHidden = body.isEmpty
        bodyText.string = body
        resizeBodyToFit()
        needsLayout = true
    }

    private func loadThumbnail(_ attachment: Attachment, into imageView: NSImageView) {
        if let cached = imageCache[attachment.id] {
            imageView.image = cached
            return
        }
        let url = storeURLFor(attachment)
        Task {
            let image = NSImage(contentsOf: url)
            await MainActor.run { [weak self] in
                guard let image else { return }
                self?.imageCache[attachment.id] = image
                imageView.image = image
            }
        }
    }

    // 由 ItemsWindow 注入，避免 DetailView 直接持有 store
    var attachmentURLProvider: ((Attachment) -> URL)?
    private func storeURLFor(_ attachment: Attachment) -> URL {
        attachmentURLProvider?(attachment) ?? URL(fileURLWithPath: "/tmp/\(attachment.id)")
    }

    @objc private func openAttachment(_ sender: NSClickGestureRecognizer) {
        guard let view = sender.view as? NSImageView,
              let attachment = attachmentByView[view] else { return }
        onOpenAttachment?(attachment)
    }

    private func resizeBodyToFit() {
        let width = max(bodyScroll.contentSize.width, 50)
        let used = (bodyText.layoutManager?.usedRect(for: bodyText.textContainer!).height ?? 0)
            + bodyText.textContainerInset.height * 2
        let height = max(bodyScroll.contentSize.height, used + 10)
        bodyText.frame = NSRect(x: 0, y: 0, width: width, height: height)
    }

    override func layout() {
        super.layout()
        let pad: CGFloat = 18
        let width = bounds.width
        let textWidth = max(width - pad * 2, 50)
        var y = bounds.height - 20

        emptyHint.frame = NSRect(x: pad, y: (bounds.height - 40) / 2, width: textWidth, height: 40)

        if !titleLabel.isHidden {
            titleLabel.frame = NSRect(x: pad, y: y - 26, width: textWidth, height: 26)
            y -= 26 + 8
        }
        if !badgeLabel.isHidden {
            badgeLabel.frame = NSRect(x: pad, y: y - 16, width: textWidth, height: 16)
            y -= 16 + 8
        }
        if !metaLabel.isHidden {
            metaLabel.frame = NSRect(x: pad, y: y - 14, width: textWidth, height: 14)
            y -= 14 + 4
        }
        if !reminderLabel.isHidden {
            reminderLabel.frame = NSRect(x: pad, y: y - 16, width: textWidth, height: 16)
            y -= 16 + 6
        }
        if !thumbsStrip.isHidden {
            let count = min(attachmentByView.count, 3)
            thumbsStrip.frame = NSRect(x: pad, y: y - 96, width: CGFloat(count) * 136 - 8, height: 96)
            var x: CGFloat = 0
            for view in thumbsStrip.subviews {
                view.frame = NSRect(x: x, y: 0, width: 128, height: 96)
                x += 136
            }
            y -= 96 + 10
        }
        if !bodyScroll.isHidden {
            bodyScroll.frame = NSRect(x: pad, y: 10, width: textWidth, height: max(y - 12, 60))
            bodyScroll.contentView.postsBoundsChangedNotifications = true
            resizeBodyToFit()
        }
    }

    static func badge(_ text: String, fg: NSColor, bg: NSColor) -> NSAttributedString {
        NSAttributedString(string: " \(text) ",
                           attributes: [.font: NSFont.systemFont(ofSize: 11, weight: .medium),
                                        .foregroundColor: fg,
                                        .backgroundColor: bg])
    }

    // 探针
    var debugTitleFrame: CGRect { titleLabel.frame }
    var debugBodyFrame: CGRect { bodyText.frame }
    // 编辑态读取（保存按钮用）
    var editedTitle: String { titleLabel.stringValue }
    var editedBody: String { bodyText.string }
}

// 窗口内容容器：纯 frame 布局。
// 顶栏高 20 贴顶部（留 12 边距），按钮行高 28 贴底部（留 12 边距），
// 左侧列表固定 380 宽，分隔线 1pt，右侧详情占满剩余空间。
final class ContentView: NSView {
    private weak var top: NSStackView?
    private weak var list: NSScrollView?
    private weak var divider: NSBox?
    private weak var detail: DetailView?
    private weak var actions: NSStackView?

    override func draw(_ dirtyRect: NSRect) {
        ItemsWindow.Theme.bg.setFill()
        bounds.fill()
    }

    func install(top: NSStackView, list: NSScrollView, divider: NSBox, detail: DetailView, actions: NSStackView) {
        self.top = top
        self.list = list
        self.divider = divider
        self.detail = detail
        self.actions = actions
        for view in [top, list, divider, detail, actions] { addSubview(view) }
        layoutChildren()
    }

    override func layout() {
        super.layout()
        layoutChildren()
    }

    private func layoutChildren() {
        let width = bounds.width
        let height = bounds.height
        let topBarHeight: CGFloat = 20
        let actionsHeight: CGFloat = 28
        let topInset: CGFloat = 12
        let bottomInset: CGFloat = 12
        let listWidth: CGFloat = 380

        top?.frame = NSRect(x: 16, y: height - topInset - topBarHeight, width: max(width - 32, 100), height: topBarHeight)
        actions?.frame = NSRect(x: 16, y: bottomInset, width: 440, height: actionsHeight)
        let contentTop = height - topInset - topBarHeight - 10
        let contentBottom = bottomInset + actionsHeight + 10
        let contentHeight = max(contentTop - contentBottom, 60)
        list?.frame = NSRect(x: 0, y: contentBottom, width: listWidth, height: contentHeight)
        divider?.frame = NSRect(x: listWidth, y: contentBottom, width: 1, height: contentHeight)
        detail?.frame = NSRect(x: listWidth + 1, y: contentBottom, width: max(width - listWidth - 1, 100), height: contentHeight)
    }
}

// "我的记录"窗口：左侧列表 + 右侧详情，全部纯 frame 布局（不使用 Auto Layout），
// 避免栈视图/滚动视图组合对窗口尺寸的反向拉扯。
final class ItemsWindow: NSWindow, NSTableViewDataSource, NSTableViewDelegate {
    private let store: LocalStore
    private var items: [Item] = []
    private var todoOnly = false

    let tableView = NSTableView()
    private let listScroll = NSScrollView()
    private let detailView = DetailView(frame: .zero)
    private let summaryLabel = NSTextField(labelWithString: "")
    private let todoSwitch = NSButton(checkboxWithTitle: "只看待办", target: nil, action: nil)
    private let top = NSStackView()
    private let actionButton = NSButton(title: "完成", target: nil, action: nil)
    private let saveButton = NSButton(title: "保存修改", target: nil, action: nil)
    private let snoozeButton = NSButton(title: "+2小时提醒", target: nil, action: nil)
    private let deleteButton = NSButton(title: "删除", target: nil, action: nil)
    private let actionsRow = NSStackView()
    private var lastSelectedRow = -1
    private var saveTimer: Timer?

    var onSync: (() -> Void)?

    private enum Columns {
        static let state = NSUserInterfaceItemIdentifier("state")
        static let content = NSUserInterfaceItemIdentifier("content")
        static let sync = NSUserInterfaceItemIdentifier("sync")
        static let time = NSUserInterfaceItemIdentifier("time")
    }

    // 固定浅色色板：不跟随系统外观，避免"字色=背景色"问题。
    enum Theme {
        static let bg = NSColor(srgbRed: 0.985, green: 0.976, blue: 0.965, alpha: 1)
        static let panel = NSColor.white
        static let title = NSColor(srgbRed: 0.10, green: 0.10, blue: 0.13, alpha: 1)
        static let body = NSColor(srgbRed: 0.22, green: 0.22, blue: 0.26, alpha: 1)
        static let secondary = NSColor(srgbRed: 0.45, green: 0.45, blue: 0.50, alpha: 1)
        static let divider = NSColor(srgbRed: 0.88, green: 0.86, blue: 0.83, alpha: 1)
        static let accent = NSColor(srgbRed: 0.20, green: 0.45, blue: 0.95, alpha: 1)
        static let accentSoft = NSColor(srgbRed: 0.90, green: 0.93, blue: 1.0, alpha: 1)
        static let orangeSoft = NSColor(srgbRed: 1.0, green: 0.94, blue: 0.86, alpha: 1)
        static let orangeText = NSColor(srgbRed: 0.75, green: 0.42, blue: 0.05, alpha: 1)
        static let graySoft = NSColor(srgbRed: 0.93, green: 0.93, blue: 0.94, alpha: 1)
        static let redText = NSColor(srgbRed: 0.80, green: 0.22, blue: 0.20, alpha: 1)
        static let greenSoft = NSColor(srgbRed: 0.88, green: 0.97, blue: 0.91, alpha: 1)
        static let greenText = NSColor(srgbRed: 0.13, green: 0.50, blue: 0.28, alpha: 1)
    }

    init(store: LocalStore) {
        self.store = store
        super.init(contentRect: NSRect(x: 0, y: 0, width: 860, height: 500),
                   styleMask: [.titled, .closable, .resizable, .miniaturizable],
                   backing: .buffered, defer: false)
        isReleasedWhenClosed = false
        title = "我的记录"
        backgroundColor = Theme.bg
        // 强制浅色外观：内容区使用固定浅色色板，若跟随系统深色模式，
        // 按钮/复选框等标准控件会画成白字而隐形（用户深色模式下已复现）。
        appearance = NSAppearance(named: .aqua)
        minSize = NSSize(width: 720, height: 440)

        let content = ContentView(frame: NSRect(x: 0, y: 0, width: 860, height: 500))
        contentView = content

        // MARK: 顶栏
        todoSwitch.target = self
        todoSwitch.action = #selector(toggleFilter)
        todoSwitch.font = NSFont.systemFont(ofSize: 12)
        summaryLabel.textColor = Theme.secondary
        summaryLabel.font = NSFont.systemFont(ofSize: 12)
        summaryLabel.lineBreakMode = .byTruncatingTail
        top.addArrangedSubview(todoSwitch)
        top.addArrangedSubview(summaryLabel)
        top.orientation = .horizontal
        top.spacing = 12

        // MARK: 左侧列表
        for (id, width) in [(Columns.state, 84.0), (Columns.content, nil), (Columns.sync, 72.0), (Columns.time, 118.0)] {
            let column = NSTableColumn(identifier: id)
            if let width { column.width = width } else { column.resizingMask = .autoresizingMask }
            column.title = id == Columns.state ? "状态" : id == Columns.content ? "内容" : id == Columns.sync ? "同步" : "时间"
            tableView.addTableColumn(column)
        }
        tableView.dataSource = self
        tableView.delegate = self
        tableView.usesAlternatingRowBackgroundColors = false
        tableView.rowHeight = 34
        tableView.gridStyleMask = []
        tableView.allowsColumnReordering = false
        tableView.allowsMultipleSelection = false
        tableView.backgroundColor = Theme.panel
        listScroll.documentView = tableView
        listScroll.hasVerticalScroller = true
        listScroll.autohidesScrollers = true
        listScroll.drawsBackground = false

        // 分隔线
        let divider = NSBox()
        divider.boxType = .separator

        // MARK: 右侧详情
        detailView.attachmentURLProvider = { [weak self] attachment in
            self?.store.attachmentURL(attachment) ?? URL(fileURLWithPath: "/tmp/\(attachment.id)")
        }
        detailView.onOpenAttachment = { [weak self] attachment in
            guard let self else { return }
            NSWorkspace.shared.open(self.store.attachmentURL(attachment))
        }

        // MARK: 操作按钮行
        actionButton.bezelStyle = .rounded
        actionButton.controlSize = .regular
        actionButton.attributedTitle = Self.buttonTitle("完成", color: Theme.accent)
        actionButton.target = self
        actionButton.action = #selector(toggleDone)
        saveButton.bezelStyle = .rounded
        saveButton.controlSize = .regular
        saveButton.attributedTitle = Self.buttonTitle("保存修改", color: Theme.title)
        saveButton.target = self
        saveButton.action = #selector(saveEdits)
        snoozeButton.bezelStyle = .rounded
        snoozeButton.controlSize = .regular
        snoozeButton.attributedTitle = Self.buttonTitle("+2小时提醒", color: Theme.title)
        snoozeButton.target = self
        snoozeButton.action = #selector(snoozeSelected)
        deleteButton.bezelStyle = .rounded
        deleteButton.controlSize = .regular
        deleteButton.attributedTitle = Self.buttonTitle("删除", color: Theme.redText)
        deleteButton.target = self
        deleteButton.action = #selector(deleteSelected)
        actionsRow.addArrangedSubview(actionButton)
        actionsRow.addArrangedSubview(saveButton)
        actionsRow.addArrangedSubview(snoozeButton)
        actionsRow.addArrangedSubview(deleteButton)
        actionsRow.orientation = .horizontal
        actionsRow.spacing = 8

        // MARK: 组装（全部 frame 布局）
        content.install(top: top, list: listScroll, divider: divider, detail: detailView, actions: actionsRow)

        detailView.setItem(nil, store: store)
        // 双保险：通知 + 委托方法都可能漏（程序化选中时），同时监听通知中心。
        NotificationCenter.default.addObserver(self, selector: #selector(selectionChanged(_:)),
                                               name: NSTableView.selectionDidChangeNotification, object: tableView)

        // 视图可见时定期刷新（同步循环可能改变状态）
        Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            if self?.isVisible == true { self?.refresh() }
        }
        refresh()
    }

    required init?(coder: NSCoder) { fatalError("不支持") }

    // 探针只读访问
    var debugDetailTitleFrame: CGRect { detailView.debugTitleFrame }
    var debugBodyFrame: CGRect { detailView.debugBodyFrame }
    var debugListFrame: CGRect { listScroll.frame }
    var debugDetailViewFrame: CGRect { detailView.frame }
    var debugActionsFrame: CGRect { actionsRow.frame }
    var debugTopFrame: CGRect { top.frame }
    var debugContentFrame: CGRect { contentView?.frame ?? .zero }
    var debugSelectedRow: Int { tableView.selectedRow }
    var debugButtonInfo: String { "action=[\(actionButton.title) e=\(actionButton.isEnabled)] snooze=[\(snoozeButton.title) e=\(snoozeButton.isEnabled)] delete=[\(deleteButton.title) e=\(deleteButton.isEnabled)]" }
    func debugUpdateDetail() { updateDetail() }

    func openAndRefresh() {
        refresh()
        makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    func refresh() {
        let selectedID = selectedItem()?.id
        items = (try? store.items()) ?? []
        tableView.reloadData()
        summaryLabel.stringValue = summaryText()
        if let selectedID, let index = displayed.firstIndex(where: { $0.id == selectedID }) {
            tableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        }
        updateDetail()
    }

    private var displayed: [Item] {
        // 已删除（回收站）不再出现在列表中
        let alive = items.filter { $0.deletedAt == nil }
        return todoOnly ? alive.filter { !$0.ended && $0.kind == "task" } : alive
    }

    private func summaryText() -> String {
        let alive = items.filter { $0.deletedAt == nil }
        let todos = alive.filter { !$0.ended && $0.kind == "task" }.count
        let pending = (try? store.pending())?.filter { $0.state != "acknowledged" }.count ?? 0
        let conflicts = (try? store.conflictItems())?.count ?? 0
        var parts = ["共 \(alive.count) 条", "待办 \(todos) 条"]
        if pending > 0 { parts.append("待同步 \(pending)") }
        if conflicts > 0 { parts.append("冲突 \(conflicts)") }
        return parts.joined(separator: " · ")
    }

    private func selectedItem() -> Item? {
        guard tableView.selectedRow >= 0, tableView.selectedRow < displayed.count else { return nil }
        return displayed[tableView.selectedRow]
    }

    private var detailKey = ""
    private func updateDetail() {
        let item = selectedItem()
        // 内容指纹：仅当选中项内容/同步态真正变化时才重填详情，
        // 避免 2 秒定时刷新打断用户正在进行的编辑。
        let ready = item.map { ((try? store.synchronized($0)) ?? false) } ?? false
        let key = item.map { "\($0.id)|\($0.updatedAt)|\(ready)" } ?? "none"
        if key != detailKey {
            detailKey = key
            detailView.setItem(item, store: store)
        }
        actionButton.isEnabled = item.map { $0.deletedAt == nil } ?? false
        actionButton.attributedTitle = Self.buttonTitle(item?.ended == true ? "重新打开" : "完成", color: Theme.accent)
        saveButton.isEnabled = item.map { $0.deletedAt == nil } ?? false
        snoozeButton.isEnabled = item.map { $0.kind == "task" && !$0.ended } ?? false
        deleteButton.isEnabled = item != nil
    }

    static func buttonTitle(_ text: String, color: NSColor) -> NSAttributedString {
        NSAttributedString(string: text,
                           attributes: [.foregroundColor: color,
                                        .font: NSFont.systemFont(ofSize: 13, weight: .medium)])
    }

    // MARK: - 操作

    @objc private func toggleFilter() {
        todoOnly = todoSwitch.state == .on
        tableView.deselectAll(nil)
        refresh()
    }

    @objc private func saveEdits() {
        guard let item = selectedItem(), item.deletedAt == nil else { return }
        do {
            try store.update(item.id, title: detailView.editedTitle, body: detailView.editedBody)
            onSync?()
            refresh()
            flashSaved()
        } catch {
            flashError("保存失败：\(error.localizedDescription)")
        }
    }

    private func flashSaved() {
        saveButton.attributedTitle = Self.buttonTitle("已保存 ✓", color: Theme.greenText)
        saveTimer?.invalidate()
        saveTimer = Timer.scheduledTimer(withTimeInterval: 1.5, repeats: false) { [weak self] _ in
            self?.saveButton.attributedTitle = Self.buttonTitle("保存修改", color: Theme.title)
        }
    }

    private func flashError(_ message: String) {
        summaryLabel.stringValue = message
        summaryLabel.textColor = Theme.redText
        Timer.scheduledTimer(withTimeInterval: 3, repeats: false) { [weak self] _ in
            self?.summaryLabel.textColor = Theme.secondary
            self?.refresh()
        }
    }

    @objc private func toggleDone() {
        guard let item = selectedItem() else { return }
        do {
            // 历史"记录"类型先转为待办，统一按待办处理
            if item.kind != "task" {
                try store.convertToTask(item.id)
            }
            let fresh = try store.item(item.id) ?? item
            if fresh.ended {
                try store.reopen(fresh.id)
            } else {
                try store.complete(fresh.id)
            }
            onSync?()
            refresh()
        } catch {
            flashError(error.localizedDescription)
        }
    }

    @objc private func snoozeSelected() {
        guard let item = selectedItem(), !item.ended else { return }
        try? store.setReminder(item.id, at: Date().addingTimeInterval(2 * 3600))
        onSync?()
        refresh()
    }

    @objc private func deleteSelected() {
        guard let item = selectedItem() else { return }
        try? store.deleteItem(item.id)
        onSync?()
        refresh()
    }

    // MARK: - 徽标与时间

    static func stateBadge(_ item: Item) -> (String, NSColor, NSColor) {
        if item.deletedAt != nil { return ("已删除", Theme.secondary, Theme.graySoft) }
        switch item.taskState {
        case "todo": return ("待办", Theme.accent, Theme.accentSoft)
        case "in_progress": return ("进行中", Theme.accent, Theme.accentSoft)
        case "waiting": return ("等待中", Theme.orangeText, Theme.orangeSoft)
        case "completed": return ("已完成", Theme.greenText, Theme.greenSoft)
        case "canceled": return ("已取消", Theme.secondary, Theme.graySoft)
        default: return (item.kind == "capture" ? "记录" : "备忘", Theme.secondary, Theme.graySoft)
        }
    }

    static func fullTime(_ iso: String) -> String {
        guard let date = Wire.date(iso) else { return iso }
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter.string(from: date)
    }

    static func timeText(_ iso: String) -> String {
        guard let date = Wire.date(iso) else { return iso }
        let formatter = DateFormatter()
        formatter.dateFormat = "MM-dd HH:mm"
        return formatter.string(from: date)
    }

    // MARK: - NSTableView

    func numberOfRows(in tableView: NSTableView) -> Int { displayed.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let item = displayed[row]
        let selected = tableView.isRowSelected(row)
        let text: String
        let color: NSColor
        switch tableColumn?.identifier {
        case Columns.state:
            let badge = Self.stateBadge(item)
            text = badge.0
            color = selected ? NSColor.white : badge.1
        case Columns.sync:
            let ready = ((try? store.synchronized(item)) ?? false)
            text = item.ended ? "—" : (ready ? "已同步" : "待同步")
            color = selected ? NSColor.white : (ready ? Theme.secondary : Theme.orangeText)
        case Columns.time:
            text = Self.timeText(item.createdAt)
            color = selected ? NSColor.white : Theme.secondary
        default:
            text = item.title.isEmpty ? (item.bodyMd.isEmpty ? "[图片记录]" : item.bodyMd.replacingOccurrences(of: "\n", with: " ")) : item.title
            color = selected ? NSColor.white : (item.ended ? Theme.secondary : Theme.body)
        }
        let identifier = NSUserInterfaceItemIdentifier("cell-\(tableColumn?.identifier.rawValue ?? "")")
        let cell = tableView.makeView(withIdentifier: identifier, owner: nil) as? NSTableCellView ?? {
            let c = NSTableCellView()
            c.identifier = identifier
            let field = NSTextField(labelWithString: "")
            field.translatesAutoresizingMaskIntoConstraints = false
            field.lineBreakMode = .byTruncatingTail
            field.font = NSFont.systemFont(ofSize: 12)
            c.addSubview(field)
            NSLayoutConstraint.activate([
                field.leadingAnchor.constraint(equalTo: c.leadingAnchor),
                field.trailingAnchor.constraint(lessThanOrEqualTo: c.trailingAnchor),
                field.centerYAnchor.constraint(equalTo: c.centerYAnchor),
            ])
            c.textField = field
            return c
        }()
        cell.textField?.stringValue = text
        cell.textField?.textColor = color
        return cell
    }

    @objc private func selectionChanged(_ notification: Notification) {
        // 选中行单元格需要重取（文字颜色随选中态变白）
        let current = tableView.selectedRow
        var rows = IndexSet()
        if lastSelectedRow >= 0, lastSelectedRow < displayed.count { rows.insert(lastSelectedRow) }
        if current >= 0, current < displayed.count { rows.insert(current) }
        lastSelectedRow = current
        if !rows.isEmpty {
            tableView.reloadData(forRowIndexes: rows, columnIndexes: IndexSet(integersIn: 0..<tableView.numberOfColumns))
        }
        updateDetail()
    }

    func tableView(_ tableView: NSTableView, selectionDidChange notification: Notification) {
        updateDetail()
    }
}
