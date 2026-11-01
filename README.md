# AIMemorandum

本机优先的桌面备忘录，用于快速记下文字、截图和剪贴板内容，并通过待办和提醒继续处理未完成事项。macOS 使用原生客户端；Windows 与 Linux 使用共享 Rust + Slint 客户端，代码位于 `apps/desktop/`。

## 使用

macOS 默认快捷键：

| 操作 | 快捷方式 |
|---|---|
| 快速记录 | ⌥⌘M → 输入 → Enter |
| 截图记录 | ⌥⌘S → 框选松手，直接保存 |
| 剪贴板速存 | 复制内容 → ⌥⌘V |
| 查看记录 | ⌥⌘L |

Windows / Linux 默认快捷键为 `Ctrl+Alt+M / S / V / L`。记录先保存到本机，联网后同步。macOS 截图可使用本机 Vision 提取文字；Windows OCR 使用系统已安装的 OCR 语言包，Linux 截图与运行依赖见 [运行指南](docs/RUN.md)。应用以独立安装包或可执行文件运行，屏幕录制权限授予 AIMemorandum 本身。

## 桌面客户端（Windows / Linux）

共享界面与控制器在 `apps/desktop/ui/`，Windows 与 Linux 平台适配分别位于 `apps/desktop/windows/` 和 `apps/desktop/linux/`。本地打包只生成 NSIS、deb 与 AppImage，不签名、不公证、不发布，也不新增发布 CI：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-installer.ps1
```

```bash
bash scripts/build-linux-packages.sh --deb --appimage
```

完整命令、隔离安装/解包步骤与边界见[运行指南](docs/RUN.md)。Windows 与 XFCE/X11 路径已有实机验收记录，见 [apps/desktop/VALIDATION.md](apps/desktop/VALIDATION.md)；GNOME/KDE Wayland、Linux 多屏与非 1 缩放、干净系统完整链路仍待人工验收。

## 构建 Mac 应用

需要 macOS、Swift 6 工具链和系统 SDK；应用目标为 macOS 14 及以上。最低版本与多屏实机验证尚未完成，当前本机构建为 Apple Silicon。

```sh
bash scripts/build-macos-app.sh
bash scripts/install-macos-app.sh
open "$HOME/Applications/AIMemorandum.app"
```

生成的应用位于 `dist/macos.noindex/AIMemorandum.app`，安装到 `~/Applications/AIMemorandum.app`。本机默认 ad-hoc 签名；Developer ID 签名、公证和正式分发尚未完成。

## 服务端与测试

当前开发服务端使用 Go 1.26 和 SQLite，默认只监听本机地址；PostgreSQL 是后续迁移目标。服务端启动、设备绑定与端到端验证步骤见[运行指南](docs/RUN.md)。

```sh
cd services/api
go test ./...
go run ./cmd/api -addr 127.0.0.1:8787 -data ../../.dev/api
```

Mac 核心回归测试：

```sh
cd apps/macos
swift run --disable-sandbox memorandum-tests
```

本机数据库、截图原图、设备凭据、运行日志和构建产物不纳入 Git。用户记录保存在 `~/Library/Application Support/AIMemorandum/`。

## 项目结构

- `apps/macos/`：macOS 菜单栏客户端、本地存储、同步、截图和提醒。
- `apps/desktop/`：Windows / Linux 共享 Rust + Slint 客户端、平台适配、打包与验收记录。
- `services/api/`：Go API、事项操作、附件与同步存储。
- `scripts/`：应用打包、安装和签名检查。
- `docs/RUN.md`：当前开发运行方式与已知边界。
- `docs/implementation/`：实现与修复记录。
- `docs/proposals/AM-001-personal-memorandum/`：产品和架构设计、接口草案、开发验收计划。

设计文档包含尚未实现的功能与目标，以运行指南和实际代码为准。
