# 独立 Mac 应用打包

日期：2026-10-15。

## 原因

试用版此前直接运行 SwiftPM 输出的 Mach-O 文件，没有 `.app` 身份；系统隐私授权可能归到终端或启动它的开发工具。使用者需要独立启动，并对 AIMemorandum 本身授权。

## 本次修改

- `apps/macos/Bundle/Info.plist`：固定 `com.aimemorandum.app`，定义 APPL、可执行文件、图标、版本、macOS 最低版本、菜单栏应用模式与截图用途说明。
- `scripts/build-macos-app.sh`：Release 构建，将二进制、Info.plist、图标放入标准 `.app/Contents/` 并签名。
- `scripts/install-macos-app.sh`：验证、安装到 `~/Applications/AIMemorandum.app`，注册 LaunchServices。升级只替换同 Bundle ID 的应用包。
- `scripts/verify-macos-app.py`：检查应用包结构、可执行文件、资源、签名、真实运行时 Bundle ID 与动态依赖，拒绝依赖构建目录的产物。
- `AppLauncher`：正常功能只在 `.app` 内运行；裸二进制启动会转交 LaunchServices 打开已安装应用。诊断环境与身份检查保留开发入口。
- 显式再次打开已运行的应用时，显示快速输入面板；后台启动和截图不会自动打开记录窗口。

生成的分发包放在 `dist/macos.noindex/`，减少构建目录副本被 Spotlight 当作日常启动入口。正式使用位置固定为用户 Applications。

## 权限和签名边界

本机 `security find-identity -v -p codesigning` 返回 0 个可用身份，因此本轮使用 ad-hoc 签名，不创建或导入新证书，不更改信任设置，不修改 TCC 数据库。应用具有独立 Bundle ID；首次录屏仍需用户按 macOS 系统提示授权。以前终端的录屏权限不自动转移。

固定 Bundle ID 和路径解决日常入口与权限归属问题；不能将本地签名描述成 Developer ID 公证，也不能承诺每次替换本地构建后 macOS 永远不再确认权限。可通过 `MEMO_SIGNING_IDENTITY` 提供未来可用的 Developer ID 身份，Bundle ID 保持不变。

原有数据路径和数据库结构均不变，继续使用 `~/Library/Application Support/AIMemorandum/`。

## 验证方式

```sh
bash scripts/build-macos-app.sh
bash scripts/install-macos-app.sh
python3 scripts/verify-macos-app.py "$HOME/Applications/AIMemorandum.app"
open "$HOME/Applications/AIMemorandum.app"
```

权限只能在系统 UI 由用户确认。独立应用安装和启动完成后，应验证 LaunchServices 与运行进程均指向用户 Applications 内的应用，而非 SwiftPM 的 Debug/Release 裸二进制。

## 本次实际结果

- Release 构建、图标生成、签名校验、动态依赖检查和运行时 Bundle 身份检查均通过。
- 已安装并通过 LaunchServices 启动 `~/Applications/AIMemorandum.app`。
- 实际进程路径为该应用内的 `Contents/MacOS/AIMemorandum`，父进程 PID 为 1；原 Debug 裸进程已退出。
- 原生 UI 工具可以通过 `com.aimemorandum.app` 找到应用，并看到快速输入面板；不再需要将裸可执行文件当作应用操作。
- 原有 15 项 Swift 回归测试全部通过。首次屏幕录制权限仍等待用户按系统提示确认，本轮没有代替用户授权。
