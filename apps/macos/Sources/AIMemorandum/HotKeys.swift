import AppKit
import MemorandumCore

// 全局快捷键：Carbon RegisterEventHotKey 对未打包的可执行文件同样生效，
// 且不依赖辅助功能权限（与 CGEvent tap 不同）。
import Carbon.HIToolbox

final class HotKeyManager {
    static var current: HotKeyManager?
    private var handlers: [UInt32: () -> Void] = [:]
    private var refs: [EventHotKeyRef?] = []
    private var installed = false

    static let cmdOption: UInt32 = UInt32(cmdKey | optionKey)
    static let panel: UInt32 = 1      // ⌥⌘M
    static let screenshot: UInt32 = 2 // ⌥⌘S
    static let clipboard: UInt32 = 3  // ⌥⌘V

    func register(id: UInt32, keyCode: UInt32, modifiers: UInt32, handler: @escaping () -> Void) {
        installIfNeeded()
        var ref: EventHotKeyRef?
        let result = RegisterEventHotKey(keyCode, modifiers, EventHotKeyID(signature: OSType(0x41494D4D) /* AIMM */, id: id), GetApplicationEventTarget(), 0, &ref)
        guard result == noErr else {
            NSLog("快捷键注册失败 id=\(id) osstatus=\(result)")
            return
        }
        refs.append(ref)
        handlers[id] = handler
    }

    private func installIfNeeded() {
        guard !installed else { return }
        installed = true
        HotKeyManager.current = self
        var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let callback: EventHandlerUPP = { _, event, userData in
            guard let event else { return noErr }
            var hotKeyID = EventHotKeyID()
            let status = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil, MemoryLayout<EventHotKeyID>.size, nil, &hotKeyID)
            guard status == noErr, let manager = HotKeyManager.current, let handler = manager.handlers[hotKeyID.id] else { return noErr }
            DispatchQueue.main.async(execute: handler)
            return noErr
        }
        let selfPointer = Unmanaged.passRetained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), callback, 1, &eventType, selfPointer, nil)
    }
}
