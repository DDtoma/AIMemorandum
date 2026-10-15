# 第一条完整流程实施记录

本轮范围：原生 Mac 菜单栏文字与区域截图快速记录、本地 SQLite 与原图落盘、Go/PostgreSQL 同步、一次性系统提醒、完成撤销提醒。遵守 AM-001，其他首版功能在后续任务中继续。

工程约定：Swift Package + AppKit/SwiftUI，最低 macOS 14，Swift 5 语言模式运行在 Swift 6 工具链；SQLite C 接口，无第三方 Mac 依赖。Go 1.25 + pgx/v5，PostgreSQL 17；所有日期用 UTC RFC3339。使用独立 `.dev/` 数据目录测试，不读取其他项目数据库。

设计与环境差异：本机只有 Command Line Tools，没有完整 Xcode 与 Developer ID 证书。本轮构建本地 ad-hoc 签名 `.app`，商业签名、公证和最低系统版本测试仍待后续。开发连接仅允许显式 localhost HTTP，部署配置仍采用 HTTPS。

本轮测试先覆盖离线记录、中文输入提交判定、提醒结束、冲突保护，再覆盖真实 PostgreSQL 和跨语言同步。原方案完整任务不因本轮开始而全部标记完成。
