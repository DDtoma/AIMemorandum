# Phase 0 探针程序（desktop-spike）

配合 `docs/proposals/AM-002-desktop-port/phase0-checklist.md` 使用。
这不是产品代码，验证完成后不进入 apps/desktop 正式结构。

## 构建

需要 Rust 工具链（stable）。Linux 上 portal_probe 需要系统 D-Bus 会话（桌面会话默认有）。

```sh
# 全部探针（Linux）
cargo build --release --features linux-portals
# Windows / macOS 上 portal_probe 自动排除，直接：
cargo build --release
```

## 运行

| 程序 | 用途 | 平台 |
|---|---|---|
| `ime_probe` | 清单第 1 条：中文 IME 组字不误提交 | 全部 |
| `memory_probe` / `memory_probe --window` | 清单第 2 条：内存基线（配合 scripts/measure-memory.*） | 全部 |
| `portal_probe` | 清单第 3、4 条：ScreenCast / GlobalShortcuts portal 能力 | 仅 Linux |

框选层验证（清单第 5 条）用 `memory_probe --window` 的全屏无边框模式手工验证：
设置环境变量 `PROBE_FULLSCREEN=1` 启动即为全屏无边框窗口。
