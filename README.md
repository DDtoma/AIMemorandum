# AIMemorandum

Mac 原生菜单栏备忘录，用于快速记下文字、截图和剪贴板内容，并通过待办和提醒继续处理未完成事项。

## 使用

| 操作 | 快捷方式 |
|---|---|
| 记录文字 | ⌥⌘M → 输入 → Enter |
| 区域截图 | ⌥⌘S → 框选松手，直接保存 |
| 剪贴板速存 | 复制内容 → ⌥⌘V |
| 查看记录 | ⌥⌘L |

记录先保存到本机，联网后同步。截图可使用本机 Vision 提取文字。应用以独立 `.app` 安装，屏幕录制权限授予 AIMemorandum 本身。

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

- `apps/macos/`：菜单栏客户端、本地存储、同步、截图和提醒。
- `services/api/`：Go API、事项操作、附件与同步存储。
- `scripts/`：应用打包、安装和签名检查。
- `docs/RUN.md`：当前开发运行方式与已知边界。
- `docs/implementation/`：实现与修复记录。
- `docs/proposals/AM-001-personal-memorandum/`：产品和架构设计、接口草案、开发验收计划。

设计文档包含尚未实现的功能与目标，以运行指南和实际代码为准。
