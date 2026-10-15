import AppKit

/// 正常使用必须通过 .app/LaunchServices 启动，避免权限归到终端或开发工具。
enum AppLauncher {
    static let bundleIdentifier = "com.aimemorandum.app"
    static var installedURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Applications/AIMemorandum.app", isDirectory: true)
    }

    static func ensureBundledLaunch() -> Bool {
        if Bundle.main.bundleIdentifier == bundleIdentifier, Bundle.main.bundleURL.pathExtension == "app" {
            return true
        }
        guard FileManager.default.fileExists(atPath: installedURL.path) else {
            fputs("请先运行 scripts/build-macos-app.sh，并将生成的 AIMemorandum.app 放入 ~/Applications 后打开。\n", stderr)
            return false
        }
        // 老的 swift run / 裸二进制命令只负责转交给已安装应用，自己不记录或抓屏。
        NSWorkspace.shared.openApplication(at: installedURL, configuration: NSWorkspace.OpenConfiguration()) { _, error in
            if let error {
                fputs("无法打开 AIMemorandum.app：\(error.localizedDescription)\n", stderr)
                exit(1)
            }
            exit(0)
        }
        RunLoop.main.run()
        return false
    }
}
