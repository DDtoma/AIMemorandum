import Foundation
import AppKit
import MemorandumCore

if CommandLine.arguments.contains("--app-identity-check") {
    let identity: [String: Any] = [
        "bundle_identifier": Bundle.main.bundleIdentifier ?? "",
        "bundle_url": Bundle.main.bundleURL.path,
        "is_app_bundle": Bundle.main.bundleURL.pathExtension == "app"
    ]
    print(String(decoding: try! JSONSerialization.data(withJSONObject: identity, options: [.sortedKeys]), as: UTF8.self))
    exit(0)
}

if CaptureRegressionProbe.runIfRequested() { exit(0) }

// 诊断模式：MEMO_PROBE=2 验证截图 Esc 取消路径不再卡死。
if ProcessInfo.processInfo.environment["MEMO_PROBE"] == "2" {
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let coordinator = ScreenshotCoordinator()
    coordinator.onError = { message in print("PROBE onError: \(message)") }
    coordinator.begin()
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
        if coordinator.debugHasOverlay {
            coordinator.debugCancel() // 模拟 Esc
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
            print("PROBE overlay-cleared=\(!coordinator.debugHasOverlay)")
            print("PROBE main-thread-responsive=true") // 能走到这里说明主线程未卡死
            exit(0)
        }
    }
    app.run()
    exit(0)
}

// MEMO_PROBE=3：验证输入文字的实际渲染属性（前景色/背景）。
if ProcessInfo.processInfo.environment["MEMO_PROBE"] == "3" {
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let panel = CapturePanel()
    panel.toggle()
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        let tv = panel.debugTextView
        // 模拟中文输入法完整路径：组字（marked text）→ 提交（unmarkText）
        tv.setMarkedText("ceshi", selectedRange: NSRange(location: 0, length: 0),
                         replacementRange: NSRange(location: NSNotFound, length: 0))
        let markedForeground = (tv.markedTextAttributes ?? [:])[.foregroundColor] as? NSColor
        print("PROBE marked-foreground=\(markedForeground?.usingColorSpace(.sRGB) ?? nil)")
        tv.unmarkText()
        let attr = tv.attributedString().attributes(at: 0, effectiveRange: nil)
        let foreground = attr[.foregroundColor] as? NSColor
        print("PROBE committed-foreground=\(foreground?.usingColorSpace(.sRGB) ?? nil)")
        print("PROBE background=\(tv.backgroundColor.usingColorSpace(.sRGB) ?? nil)")
        print("PROBE textView.frame=\(tv.frame) scroll.frame=\(tv.enclosingScrollView?.frame ?? .zero)")
        print("PROBE caret-rect=\(tv.firstRect(forCharacterRange: tv.selectedRange(), actualRange: nil) ?? .zero)")
        tv.insertText(" 明天上午确认预算数字，记得带合同")
        tv.insertText("\n第二行 12345 English mixed")
        // 自检：把面板当前显示内容截成 PNG，人工核对文字可见性
        if let rep = panel.contentView?.bitmapImageRepForCachingDisplay(in: panel.contentView!.bounds) {
            panel.contentView!.cacheDisplay(in: panel.contentView!.bounds, to: rep)
            if let png = rep.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: "/tmp/memo-panel-check.png"))
                print("PROBE screenshot=/tmp/memo-panel-check.png")
            }
        }
        exit(0)
    }
    app.run()
    exit(0)
}

// MEMO_PROBE=4：记录窗口（列表+详情双栏）自检：造样例数据、选中首行、截图。
if ProcessInfo.processInfo.environment["MEMO_PROBE"] == "4" {
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)
    app.activate(ignoringOtherApps: true)
    let root = URL(fileURLWithPath: "/tmp/memo-probe-items")
    try? FileManager.default.removeItem(at: root)
    let store = try! LocalStore(root: root)
    _ = try! store.createText("明天上午九点和李总确认合同金额，记得把报价单和盖章版协议一起带过去", task: true)
    _ = try! store.createText("看到一个有意思的开源项目，本地优先的同步协议设计，周末抽空研究一下它的冲突处理方案", task: false)
    let window = ItemsWindow(store: store)
    window.openAndRefresh()
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) {
        window.tableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            print("PROBE selected-row=\(window.debugSelectedRow)")
            print("PROBE buttons=\(window.debugButtonInfo)")
            print("PROBE key=\(window.isKeyWindow) main=\(window.isMainWindow) app-active=\(NSApplication.shared.isActive)")
            NSApplication.shared.activate(ignoringOtherApps: true)
            window.debugUpdateDetail()
            if let content = window.contentView {
            print("PROBE window-frame=\(window.frame)")
            print("PROBE content-frame=\(window.debugContentFrame)")
            print("PROBE list-frame=\(window.debugListFrame)")
            print("PROBE detailview-frame=\(window.debugDetailViewFrame)")
            print("PROBE actions-frame=\(window.debugActionsFrame)")
            print("PROBE top-frame=\(window.debugTopFrame)")
            print("PROBE title-frame=\(window.debugDetailTitleFrame)")
            print("PROBE body-frame=\(window.debugBodyFrame)")
                if let rep = content.bitmapImageRepForCachingDisplay(in: content.bounds) {
                    content.cacheDisplay(in: content.bounds, to: rep)
                    if let png = rep.representation(using: .png, properties: [:]) {
                        try? png.write(to: URL(fileURLWithPath: "/tmp/memo-items-check.png"))
                        print("PROBE screenshot=/tmp/memo-items-check.png")
                    }
                }
                // 单独截取按钮区域放大检查标题渲染
                let buttonRect = NSRect(x: 0, y: 0, width: 560, height: 48)
                if let rep2 = content.bitmapImageRepForCachingDisplay(in: buttonRect) {
                    content.cacheDisplay(in: buttonRect, to: rep2)
                    if let png2 = rep2.representation(using: .png, properties: [:]) {
                        try? png2.write(to: URL(fileURLWithPath: "/tmp/memo-buttons-check.png"))
                        print("PROBE buttons-shot=/tmp/memo-buttons-check.png")
                    }
                }
                // 未选中（按钮禁用）状态下再截一张
                window.tableView.deselectAll(nil)
                window.debugUpdateDetail()
                if let rep3 = content.bitmapImageRepForCachingDisplay(in: buttonRect) {
                    content.cacheDisplay(in: buttonRect, to: rep3)
                    if let png3 = rep3.representation(using: .png, properties: [:]) {
                        try? png3.write(to: URL(fileURLWithPath: "/tmp/memo-buttons-disabled.png"))
                        print("PROBE buttons-disabled-shot=/tmp/memo-buttons-disabled.png")
                    }
                }
            }
            exit(0)
        }
    }
    app.run()
    exit(0)
}

guard AppLauncher.ensureBundledLaunch() else { exit(1) }
let delegate = AppDelegate()
NSApplication.shared.delegate = delegate
NSApplication.shared.run()
