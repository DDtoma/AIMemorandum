# AM-002 真实桌面验收汇总

日期：2026-11-01（Asia/Shanghai）  
对应规格：[#1](https://github.com/DDtoma/AIMemorandum/issues/1)；验收票：[#10](https://github.com/DDtoma/AIMemorandum/issues/10)。

本文只汇总实际执行的桌面验收、可追溯证据和仍需人工执行的项目。自动测试、打包结构检查与真实桌面验收分开记录；未执行的项目不标为通过。

## 1. 证据边界

- Windows 实机证据来自 #7 平台提交 `839c810f95a3d70d292d9fd08b92c9e885e526b8`，原始记录见 [windows/VALIDATION.md](windows/VALIDATION.md)。
- Linux XFCE/X11 实机证据来自 #8 平台提交 `47d6fe71cffe241c6933afe3abff8b3aa17de190`，原始记录见 [#8](https://github.com/DDtoma/AIMemorandum/issues/8) 及该票列出的远端证据文件。
- 上述实机结果先于最终合并，不冒充最终集成提交上的重新实测。合并后另跑自动回归；代码评审修复提交为 `78a5b876923b023c5e5630a733f45a5487bdfbed`，其后收口提交补充 Linux portal 快捷键文案与本文/包哈希，不把自动回归当作新的桌面证据。
- Windows 与 XFCE 的隔离数据目录均用于验收，未使用用户真实数据库。具体目录见各原始记录。

## 2. 实测环境

| 环境 | 实际版本与会话 | 显示器与缩放 | IME |
| --- | --- | --- | --- |
| Windows | Windows 10.0.22631.2861；optimized GNU Rust 1.98.1 | 主屏 `0,0 2560×1600`、144 DPI；副屏 `-1920,271 1920×1080`、96 DPI | Microsoft Pinyin |
| Linux L3 | EndeavourOS，kernel `7.2.11-arch1-1`；XFCE 4.20.4/X11；Rust 1.99.1 | 单个 DP-5 `1920×1080`、scale 1 | Fcitx5/Rime |
| Linux L1 | 没有可用 GNOME/Wayland 活动环境 | 未测 | 未测 |
| Linux L2 | 没有可用 KDE/Wayland 活动环境 | 未测 | 未测 |

## 3. Windows 实机结果（#7）

| 项目 | 实际证据 | 结论 |
| --- | --- | --- |
| 中文 IME | 输入 `nihao` 显示 `ni'hao`；组字 Enter 未建记录，提交“你好”后的 Enter 才保存，记录 ID `56e236b4-c07f-4b8e-b873-fdc15315d8c1`。 | 通过 |
| 焦点、草稿、热键动作 | Ctrl+Alt+M 隐藏后返回原前台，再次呼出恢复到 Edit 并保留 `draftcheck`，记录数未变化。 | 通过 |
| PNG/JPEG 原生拖放 | 从真实 Explorer 拖入；source=drop、MIME 正确，持久化 original.png/original.jpg 与源文件逐字节一致。 | 通过 |
| 文本/图像剪贴板 | 文本保存为 `clipboard-check-0731`；真实 640×160 位图保存后 PNG 解码仍为 640×160。 | 通过 |
| 单屏截图与 OCR | 真实 Ctrl+Alt+S 和指针框选保存记录 `d37a2bae-36ba-4db0-93ff-bf8d93efd20d`；150% 下 PNG 为 1095×555，非空且有 OCR 正文。 | 通过 |
| WinRT OCR | 已安装识别器输出包含 `AIMemorandum OCR 123`；产品控制器路径同时验证正文、附件与原始字节。 | 通过 |
| 混合 DPI 与 overlay | Win32 实测两块物理屏；overlay 物理边界 `-1920,0 4480×1600`，覆盖整个虚拟桌面。 | 通过 |
| 跨屏框选 | 从副屏 `(-1700,400)` 拖到主屏 `(700,1000)`，保存 PNG `2400×600`；双屏标记位置正确，OCR 同时包含 `LEFT NEGATIVE 96 DPI` 与 `PRIMARY 144 DPI`。 | 通过 |
| 热键注册与冲突 | 原生绑定注册成功；第二管理器对 Ctrl+Alt+M 返回 `HotKey already registered`。 | 通过 |
| 托盘内容与点击身份 | 真实托盘更新 12 项，长标题为 36 个 Unicode scalar；鼠标与键盘操作均只完成目标记录，其他记录不变。 | 通过 |
| 单实例 | 第二个 `--toggle-panel` 进程 exit 0，第一进程保持运行；真实 pipe 帧与所有权另有集成测试。 | 通过 |
| 内存：托盘 | WorkingSet64 **21,856,256 bytes**（20.8 MiB）；PrivateMemorySize64 **4,325,376 bytes**（4.1 MiB）。 | 低于 60/100 MiB 限额 |
| 内存：记录窗口 | WorkingSet64 **24,915,968 bytes**（23.8 MiB）；PrivateMemorySize64 **6,078,464 bytes**（5.8 MiB）。 | 低于 80/100 MiB 限额 |
| 内存：双屏 overlay | 默认 `winit-software`：WorkingSet64 **89,509,888 bytes**（85.4 MiB）；PrivateMemorySize64 **70,651,904 bytes**（67.4 MiB）。 | 低于 100 MiB |

默认 renderer 是 `winit-software`。femtovg/OpenGL 实测超过内存预算，不是默认配置。完整命令、原始数字及 opt-in probes 见 [windows/VALIDATION.md](windows/VALIDATION.md)。

## 4. Linux XFCE/X11 实机结果（#8）

原始证据目录：`/home/llight/.cache/aimemorandum-xfce-tray-20261101` 与 `/home/llight/.cache/aimemorandum-xfce-acceptance`。

| 项目 | 实际证据 | 结论 |
| --- | --- | --- |
| 中文 IME | Fcitx5/Rime 实际输入并保存“你好”；组词中的第一次 Enter 不保存，组词完成后的 Enter 保存。证据：`ime-enter-red-result.txt`（历史文件名保留，结果为通过）。 | 通过 |
| 焦点与草稿 | 无需点击即可英文输入并保存；隐藏、重开后草稿保留；tray-only 时进程不退出。证据：`focus-draft-final-result.txt`。 | 通过 |
| PNG/JPEG 原生拖放 | GTK3 XDND 拖入真实 PNG/JPEG，均生成 32×24 附件。证据：`drop-png-green-result.txt`、`drop-jpeg-green-result.txt`。 | 通过 |
| 托盘 | 真实 com.canonical.dbusmenu 读取到 12 项与 36 Unicode 字符标题；点击只完成 UUID `00000000-0000-0000-0000-0000000003e8`。证据：`tray-click-identity.txt`。 | 通过 |
| 热键与 CLI | Ctrl+Alt+M/V/S 通过；XFCE 的 Ctrl+Alt+L 冲突被单独报告，其余热键保留；`--items`、`--save-clipboard` 与无参数回退可用。 | 通过；冲突隔离符合契约 |
| 连续截图 | 两次 overlay 均为 `1920×1080@(0,0)`；两个不同逻辑选区均输出 300×200 PNG。证据：`final-clean-result.txt`。 | 仅单屏 scale 1 通过 |
| 内存：托盘 | 初始 **41.28 MiB**；连续四轮回到托盘约 **49.91–50.09 MiB**。 | 低于 60 MiB |
| 内存：overlay | **81.69→89.42 MiB**。证据：`screenshot-release-order-memory.txt`。 | 低于 100 MiB |
| 内存：记录窗口 | 记录窗口 **53.02 MiB**；记录窗口列表 **57.55 MiB**。 | 分别低于 80/100 MiB |

Linux OCR 不在 #8 范围。以上结果不证明 GNOME/KDE Wayland、多显示器或非 1 缩放。

## 5. 本地安装包验证（#9，非发布验收）

按维护者限定的范围，只验证本地 Windows NSIS、Linux deb/AppImage；不做 CI、分发、签名或公证。命令与边界见 [../../docs/RUN.md](../../docs/RUN.md)。Windows 包由代码提交 `78a5b876923b023c5e5630a733f45a5487bdfbed` 构建；Linux 包在该提交上追加 portal 快捷键文案修正后构建。哈希为最终产物实测值：

| 包 | 大小（bytes） | SHA-256 |
| --- | ---: | --- |
| `AIMemorandum-0.1.0-x86_64-setup.exe` | 10,920,386 | `16d1df0537ba0bc546f75c280194054909951683600edbb752ca53bf95503e2e` |
| `aimemorandum_0.1.0_amd64.deb` | 15,968,692 | `a4a9398d3a115d411b19ac3c41939f5b35321eea42021ea237ada6d91601a501` |
| `AIMemorandum-0.1.0-x86_64.AppImage` | 15,608,312 | `18236fe70c6479d68e6e221df5371e3dc2750d6092cc60b80f2d2bc63a8e7a09` |

- Windows release 二进制 41,210,861 bytes，SHA-256 `44d5646dbfbf2848c7a6067664bca700387ab9d18d628403e22e8b4ec98e840c`；Linux release 二进制 44,413,920 bytes，SHA-256 `11d1f1bdee1f4c163aacbb20aa130dd3ef63a860a0adc8768ffe56c8d4f5efc2`。
- Windows：NSIS 3.11 实际构建成功；最终安装包在隔离目录安装/卸载均 exit 0。安装后的 exe、LICENSE、THIRD_PARTY_NOTICES.md 与源哈希一致，卸载目录删除，Start Menu shortcut、HKCU Uninstall、HKCU Run 均未改变。未启动应用。
- deb：实际检查 control 与 payload，包内 `usr/bin/aimemorandum` 与 release 二进制逐字节一致；通过 ar/tar 解包检查，不声称已在干净系统用包管理器安装。
- AppImage：官方 appimagetool continuous 9bab981 build 340 实际构建成功；实际 `--appimage-extract` 后，二进制、LICENSE、THIRD_PARTY_NOTICES.md、PNG 与 AppRun 均验证，两个入口可执行。未据此声称完成业务链路。
- 最终自动验证：Windows 上 fmt、workspace tests、clippy `-D warnings`、release 构建、11 项打包合同测试、第三方清单 `--offline --check`、`services/api` 的 `go test ./...` 全部通过；Linux 从 `78a5b87` 的干净 `git archive` 执行同一组检查，打包测试 9 通过、2 项仅 Windows 的检查跳过。自动验证不等同于下节的实机验收。
- 三包均未完成「干净系统安装 → 注册 → 记录 → 同步 → 提醒」全链路，不能记为发布验收通过。

## 6. 仍需人工执行的验收矩阵

以下项目缺对应真实环境，保持**未验证**，不是失败，也不是通过。执行后应把系统/DE 版本、会话类型、显示器拓扑、日志、数字和必要截图追加到本文。

### 6.1 GNOME/Wayland 与 KDE/Wayland

在 GNOME 46/47+（IBus 中文）和 KDE Plasma 6（Fcitx5）分别执行：

1. 记录 `XDG_SESSION_TYPE`、DE/ compositor 版本与 IME；验证组字 Enter 不保存、上屏后 Enter 才保存，以及隐藏/重开后的焦点和草稿。
2. 真实运行 ScreenCast portal：记录首次授权弹窗、第二次运行的 restore-token 行为，确认取得 PipeWire node/fd 和静态帧；拒绝授权时必须失败关闭。
3. 检查 GlobalShortcuts portal 的 AVAILABLE/UNSUPPORTED；若不支持，用桌面环境快捷键绑定 `--toggle-panel` 等 CLI，并验证回退。
4. 验证全屏普通窗口能在静态帧上框选。KDE layer-shell 未集成，不作为通过条件；不得把普通窗口结果写成 layer-shell 结果。
5. 验证 Ctrl+Alt+M/S/V/L、真实冲突隔离与 CLI 回退；GNOME 需启用 AppIndicator/KStatusNotifier 兼容扩展后验证托盘。
6. 验证托盘 12 项、36 Unicode 字符标题和点击完成身份；核对其他记录不变。
7. 用 `scripts/measure-memory.sh <pid> 30` 分别采样托盘、记录窗口、列表与 overlay：托盘 ≤60 MiB、记录窗口 ≤80 MiB、所有稳定状态 <100 MiB。

### 6.2 Linux 多显示器与非 1 缩放

在 X11 和上述 Wayland 环境至少覆盖一个负原点、混合/分数缩放拓扑：

1. 记录逻辑/物理显示器边界、scale 与 portal 输出元数据；overlay 必须落在 captured output，不得在匹配缺失/歧义时任意选屏。
2. 跨屏拖动并在每屏放置可识别标记；比较保存 PNG 的物理尺寸、两屏内容和 OCR/像素区域。
3. 连续更换输出截图，确认路由不被合并 UI 更新卡住；隐藏后再次打开仍使用正确输出与尺寸。

### 6.3 干净系统安装包全链路（不在本轮本地打包范围）

对每种目标包各准备一台干净 VM/机器：安装或解包 → 启动 → 注册 → 创建文字/截图记录 → 同步 → 到期提醒 → 卸载/清理。记录安装依赖、portal/权限提示、服务端版本与每一步结果；本轮未执行，不标通过。

## 7. 收口结论

#10 的“证据汇总与缺口记录”可以收口：Windows 与 XFCE/X11 已有真实证据，缺失环境及人工步骤已逐项命名。本文不是全平台发布认证；GNOME/KDE Wayland、Linux 多屏/非 1 缩放和干净系统业务全链路仍是明确的未验证边界。
