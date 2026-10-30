# AM-002：Windows 与 Linux 桌面客户端

| 属性 | 内容 |
|---|---|
| 日期 | 2026-10-26 |
| 状态 | 方向已确认，待 Phase 0 风险验证；不是已实现的代码 |
| 范围 | 新增 Windows / Linux 桌面客户端；macOS 端保持 AppKit 原生不动；服务端协议零改动 |
| 上游约束 | AM-001 的领域模型、同步语义、冲突规则全部继承，不改协议 |

## 1. 已确认的硬约束

| 约束 | 来源 | 落实位置 |
|---|---|---|
| 开源项目（GPL 兼容） | 用户确认 2026-10-26 | 允许使用 Slint（GPLv3 免费档） |
| Windows 与 Linux 体验一致 | 用户确认 | 单一代码库 Rust + Slint；提醒弹窗、面板、记录窗口全部自绘 |
| Wayland 截图走 PipeWire | 用户确认 | ScreenCast portal 取流 + 自绘框选层，不做"系统弹框截图"降级版 |
| 支持 KDE / GNOME / XFCE | 用户确认 | 平台能力矩阵见第 4 节，每个 DE 有明确路线 |
| 常驻内存 < 100 MB | 用户确认 | 验收口径与度量方法见第 6 节 |

## 2. 技术选型

**最终决定：Rust 单一技术栈 + Slint UI。**

| 维度 | 结论 |
|---|---|
| UI 框架 | Slint（声明式，自带 Fluent/Material 样式；软件/GPU 渲染；内存量级 30–60 MB） |
| 语言 | 全部 Rust：核心逻辑、UI、平台集成一个 cargo workspace，无桥接层 |
| 许可 | Slint GPLv3 档，项目开源兼容；二进制再分发遵守 GPLv3 |
| 排除项 | Tauri/Electron/Flutter（webview 或运行时内存超预算）；GTK4（Windows 观感非原生）；Qt（引入 C++ 与 LGPL 动态链接约束）；egui/iced（Slint 许可可用时的次选，作为 Phase 0 失败后的退路） |

桌面集成 crate 选型（独立于 UI 框架，可替换）：

| 能力 | crate / 通道 | 说明 |
|---|---|---|
| 托盘菜单 | `tray-icon` | Win 原生；Linux 走 StatusNotifierItem（KDE/XFCE 原生支持，GNOME 需 AppIndicator 扩展，文档说明） |
| 全局热键 | `global-hotkey`（Win/X11）；`ashpd` GlobalShortcuts portal（Wayland） | 兜底：CLI 子命令 + DE 自带快捷键绑定，见第 5 节 |
| 截图 X11/Win | `xcap` | 框选即存，与 Mac 体验一致 |
| 截图 Wayland | `ashpd` ScreenCast portal + `pipewire` 取流 | 见第 5 节 |
| OCR | Windows：`windows` crate 调 WinRT OCR | Linux 无系统级 OCR，首版跳过（AM-001 本来就列为可后置增强） |
| 存储 | `rusqlite`（bundled） | schema 与 Mac 端逐表一致 |
| HTTP | `ureq`（阻塞式） | 不引 tokio/hyper 全家桶，控内存与二进制体积 |
| 异步 | 仅 portal/PipeWire 所需的最小运行时，独立工作线程 | UI 线程不跑 reactor |
| 杂项 | `serde`、`sha2`、`directories`、`image`（仅 PNG/JPEG） | 与 Mac 端字段、哈希、目录规则对齐 |

## 3. 架构

```
apps/desktop/
├── Cargo.toml               # workspace
├── core/                    # 纯 Rust、无 UI 依赖，直译 MemorandumCore
│   ├── store.rs             #   items/shadow/outbox/meta/conflicts 五表 + staging 崩溃恢复
│   ├── sync.rs              #   push → upload → pull；outbox 依赖链与 base_revision 重写
│   ├── reminder.rs          #   due/fired/prune，key 格式与 Mac 一致
│   ├── api.rs               #   契约类型（serde，snake_case）、错误信封、幂等 operation_id
│   └── ocr.rs               #   平台 OCR 抽象 trait
├── ui/                      # Slint 界面
│   ├── capture_panel.slint  #   快速记录面板（≈ Mac 480×276，拖入图片、Esc 收起、组字不提交）
│   ├── items_window.slint   #   记录窗口双栏（列表 + 可编辑详情 + 底部操作行）
│   ├── reminder_popup.slint #   自绘提醒弹窗（完成 / 稍后 10 分钟 / 知道了）
│   └── settings.slint       #   服务端地址 + 注册码
└── platform/                # cfg 分平台
    ├── hotkey.rs  tray.rs  capture.rs  overlay.rs  autostart.rs  single_instance.rs
```

必须逐条复刻的 Mac 端不变量（违反任何一条都会导致多设备同步发散）：

1. 写操作先改 `items` 再入 `outbox`，UI 不等网络。
2. outbox 依赖链：`depends_on` = 该 item 上一条 pending op；prepare 时用依赖的 `result_revision` 重写 `base_revision`；**冻结后的 body 与实际发送字节一致**。
3. 附件崩溃一致性顺序：staging/manifest.json + 原图（atomic + fsync + 0600）→ 事务入库 → 删 staging；启动时按 manifest 校验 SHA256 恢复。
4. `applyRemote` 在本地有未确认 op 时只写 shadow 不动投影；409 冲突保留本地副本进 `conflicts` 表。
5. 附件限制一致：≤10 张、单张 20 MiB、40 MP、总量 50 MiB，仅 PNG/JPEG。
6. 提醒 key 格式 `<item>:<reminder>:<scheduleRevision>:<remindAt>`，`fired_reminders` 差集与 prune 规则一致。

数据目录：Windows `%APPDATA%\AIMemorandum\`，Linux `~/.local/share/aimemorandum/`（XDG）。各设备独立库，不互拷。

## 4. 平台能力矩阵

| 能力 | Windows | Xorg（XFCE 及一切 X11 会话） | KDE Wayland | GNOME Wayland |
|---|---|---|---|---|
| 全局热键 | RegisterHotKey（global-hotkey） | X11 抓取（global-hotkey） | GlobalShortcuts portal（Plasma 6 原生） | GlobalShortcuts portal（GNOME 47+；更老版本走兜底） |
| 区域截图 | xcap，无权限概念，体验最佳 | xcap + override-redirect 框选层，体验与 Mac 一致 | PipeWire 取流 + **layer-shell 框选层**（KWin 支持） | PipeWire 取流 + **全屏无边框窗口**做框选层（Mutter 不支持 layer-shell） |
| 截图授权 | 无 | 无 | 首次/每会话一次系统对话框；支持 restore token 持久化 | 同左（持久化支持弱于 KDE，Phase 0 实测记录） |
| 托盘 | 原生 | 原生 / SNI | SNI 原生 | 需 AppIndicator 扩展（发行版常见预装；文档写明） |
| OCR | WinRT 内置中文 | 无 | 无 | 无 |
| 开机自启 | 注册表 Run 项 | XDG autostart .desktop | 同左 | 同左 |

XFCE 备注：4.18/4.20 主流是 X11 会话，走 Xorg 列；实验性 Wayland 会话（labwc/wayfire 系 wlroots）支持 layer-shell，可走 KDE 列同款框选层。Phase 0 只验收 X11 会话。

## 5. 两条关键平台路线

### 5.1 Wayland 截图（PipeWire，用户指定路线）

流程：ScreenCast portal `CreateSession → SelectSources → Start`（persist_mode + restore_token 尽量持久化授权）→ 拿 PipeWire node → `pipewire` crate 接流取最新帧（dmabuf 优先，避免整帧拷贝；4K RGBA 单帧约 33 MB，截完即释放，不计入常驻内存口径）→ Slint 全屏框选层盖在静态帧上 → 用户框选 → 裁剪编码 PNG → 走与 Mac 一致的 staging → 入库 → OCR（仅 Windows）。

KDE 用 layer-shell 真覆盖层；GNOME 用全屏无边框普通窗口代替（不能穿透置顶层级，视觉等价）。**已知体验差异：每次会话首次截图会出一次系统授权框，这与 Mac 的首次屏幕录制授权同级，可接受；做不到 Mac 的"次次无感"，文档对用户写明。**

### 5.2 全局热键的通用兜底

单实例 + 本地 IPC（`single_instance.rs`，Unix socket / Windows named pipe）：`aimemorandum --toggle-panel` / `--screenshot` / `--save-clipboard` 转发给常驻进程。任何 DE（包括 portal 不可用的老 GNOME、平铺 WM）都可以用桌面环境自己的快捷键设置绑定这三个命令。**这是覆盖率 100% 的保底路径，portal 和 X11 抓取只是更省事的默认。**

默认热键与 Mac 对齐语义：Win/Linux 用 `Ctrl+Alt+M / S / V / L`（避开 Win 键组合被系统占用的问题，Phase 0 确认无冲突后冻结写进文档）。

## 6. 内存预算与验收口径

| 场景 | 目标 | 硬上限 |
|---|---|---|
| 托盘常驻（无窗口） | ≤ 60 MB RSS | — |
| 打开记录窗口 | ≤ 80 MB | **任何稳态 < 100 MB（用户约束）** |
| 截图进行中 | 瞬时尖峰允许（单帧缓冲），结束后回落 | 不进入稳态口径 |

度量方法：Linux 读 `/proc/<pid>/status` VmRSS，Windows 读 Working Set（CI 用 PowerShell `Get-Process`）；Phase 0 出基线数据，之后每次 Phase 验收复测并记录进文档。若 Slint 基线超预算，退路顺序：egui → iced，核心层（core/）不受影响。

## 7. 与现有仓库的一致性

- 服务端（services/api）**协议零改动**，仅补 Windows/Linux CI 构建与 `go test`。
- 契约对齐手段：按 AM-001 `00_shared/06_test_design.md` 规划的"共享规则样本"落地为 JSON fixtures（outbox 序列、冲突响应、reminder 序列），Mac（Swift）与新客户端（Rust）跑同一份样本对同一 Go 服务端验证。
- 现有 15 条 Swift 回归中平台无关的 12 条逐条移植为 Rust 核心测试（AC 编号保留，Mac 专属 AC-MAC-001 系列替换为 AC-DESK 截图几何用例）。
- 冒烟程序等价物：`memorandum-smoke` 的 Rust 版，步骤不变（注册→离线创建→同步→回读附件→提醒→完成）。

## 8. 分阶段计划与验收

| 阶段 | 内容 | 验收 |
|---|---|---|
| **Phase 0：风险验证（3–5 人日）** | ① Slint 中文 IME 组字期间 Enter 不提交实测（Win + 两种 Linux IME）；② Slint 内存基线（对照第 6 节口径）；③ KDE/GNOME Wayland 上 ScreenCast portal + PipeWire 取流走通，记录授权框行为与 restore token 效果；④ GlobalShortcuts portal 在 KDE/GNOME 的可用性矩阵；⑤ layer-shell 框选层在 KDE 可行、GNOME 全屏窗口替代可行；⑥ 默认热键无系统冲突 | 六项结论写入本文档附录；内存基线达标才进 Phase 1，不达标按退路换 UI |
| **Phase 1：核心直译（8–10 人日）** | core/ 四模块 + schema + staging 恢复 + 契约样本测试 | 服务端零改动通过全部契约测试；12 条移植测试全绿 |
| **Phase 2：UI 与日常链路（8–12 人日）** | 四个 Slint 界面 + 托盘（含最近 12 条待办点击完成）+ 单实例 IPC + 自启 | Windows 上完整走通 docs/RUN.md 日常操作表；内存复测达标 |
| **Phase 3：截图与热键（8–12 人日）** | Win/X11 xcap 框选即存；Wayland PipeWire 路线（KDE layer-shell / GNOME 全屏窗）；Windows WinRT OCR；热键 portal + 兜底绑定 | 三平台框选内容与像素尺寸回归（对齐 AC-CAP-002）；GNOME 全屏框选层可用 |
| **Phase 4：打包发布（5–8 人日）** | Windows NSIS 安装包（无额外运行时依赖）；Linux deb + AppImage（无 webkitgtk 依赖，体积小）；CI 三平台构建与测试；GPLv3 声明与第三方许可清单 | 干净系统安装 → 注册 → 记录 → 同步 → 提醒全链路；安装/卸载干净 |

总计约 24–35 人日，一人全职约 5–7 周。

## 附录

- Phase 0 执行手册：[phase0-checklist.md](phase0-checklist.md)（六项验证的步骤、通过标准与记录表）
- 探针代码：`apps/desktop/spike/`；内存采样脚本：`scripts/measure-memory.sh`、`scripts/measure-memory.ps1`

## 9. 主要风险

| 风险 | 等级 | 处理 |
|---|---|---|
| Slint 中文 IME 行为不达标 | HIGH | Phase 0 第一项就测；退路 egui/iced 同样要在 Phase 0 各花半天摸底 |
| GNOME Wayland 框选层只能用全屏窗，视觉/焦点细节与 KDE 不同 | MEDIUM | 接受为平台差异并文档化；不允许影响"框选即存"主路径 |
| GNOME 托盘依赖扩展 | MEDIUM | 文档说明 + 无托盘时降级为普通窗口应用可运行 |
| 老版本 GNOME（<47）无 GlobalShortcuts portal | MEDIUM | CLI 子命令 + DE 绑定兜底，Phase 0 出支持矩阵 |
| Rust/Swift 双核心长期演化发散 | HIGH | 契约样本是唯一事实来源；任何同步语义改动先改 AM-001 `02_sync_and_reminders.md` 再改两端代码 |
| restore token 在 GNOME 持久化效果差 | LOW | 每会话一次系统框可接受；跟踪上游 portal 演进 |

## 10. 明确不做（本阶段）

- 不迁移 macOS 端（已验证的权限归属与 15 条回归不动；未来若维护压力过大再评估 Mac 迁入本客户端）。
- 不做 Linux OCR（无系统能力；tesseract 列为后续可选增强）。
- 不改服务端协议、不引入 SSE（轮询保持，SSE 是 AM-001 的既有欠账，另行立项）。
- 不做 macOS/Windows/Linux 之外的形态（无移动端、无 Web 端）。
