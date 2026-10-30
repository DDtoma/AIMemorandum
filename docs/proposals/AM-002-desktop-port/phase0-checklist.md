# AM-002 Phase 0：风险验证实测清单

日期：2026-10-26。本文档是 AM-002 README 第 8 节 Phase 0 的执行手册。
**每条结论必须带回实测证据（数字/截图/日志），不允许"应该可以"式结论。**
六项全部有结论后，把结果摘要回填到 AM-002 README 附录，并做 go / no-go 决策。

## 0. 环境矩阵（先填实）

| 机器 | 系统与版本 | 会话类型 | 桌面环境 | IME | 实测人 |
|---|---|---|---|---|---|
| W1 | Windows 11（记录具体版本） | — | — | 微软拼音 | |
| L1 | Ubuntu 24.04+ | Wayland | GNOME 46/47 | IBus 中文 | |
| L2 | Fedora / KDE neon | Wayland | KDE Plasma 6 | Fcitx5 | |
| L3 | Xubuntu / Manjaro XFCE | X11 | XFCE 4.18/4.20 | Fcitx5 | |

GNOME 老版本（<47，无 GlobalShortcuts portal）如有条件加一行 L4。

## 1. Slint 中文 IME 组字不误提交（生死测试）

- 程序：`apps/desktop/spike` 的 `ime_probe`。
- 步骤：切到中文输入法 → 输入拼音 → **组字未上屏时按 Enter** → 观察；再上屏后按 Enter 提交 → 观察。
- 通过标准：组字期间的 Enter **不触发** submitted 日志；结束组字后的 Enter 才触发。四台环境各测一次。
- 不通过处理：按 README 退路顺序摸底 egui（半天）→ iced（半天），只验证同一场景。
- 记录：| 环境 | 组字 Enter 是否误提交 | 结论 |

## 2. 内存基线

- 程序：`memory_probe`（空闲模式 = 事件循环空转；`--window` = 模拟记录窗口）。
- 脚本：Linux `scripts/measure-memory.sh <pid> 30`；Windows `scripts\measure-memory.ps1 -Pid <pid> -Seconds 30`。
- 步骤：空闲模式采样 30s → --window 模式采样 30s，四台环境各一轮。
- 通过标准：空闲 ≤ 60 MB RSS / Working Set；窗口模式 ≤ 80 MB。探针不含 core/db，真实应用预计上浮 10–20 MB，判定时预留。
- 不通过处理：同第 1 条退路；core 层不受影响。
- 记录：| 环境 | 空闲峰值 | 窗口峰值 |

## 3. Wayland ScreenCast portal + PipeWire 取流（L1/L2）

- 程序：`portal_probe`（仅 Linux 构建）。会真实弹出系统授权框，这正是观察对象。
- 步骤：每会话首次运行 → 记录弹窗内容与次数；再运行 → 记录 restore token 是否免弹窗；确认拿到 PipeWire node 与 fd。
- 通过标准：GNOME 与 KDE 都能取到流；授权行为（每会话一次 / 可持久）记录为已知体验差异。
- 注意：ashpd API 随版本变动，编译修正不算失败；取流成功以拿到 stream 为准，帧解码细节在 Phase 3 完成。
- 记录：| DE | 首次弹窗 | 二次运行行为 | 取流结果 |

## 4. GlobalShortcuts portal 支持矩阵（L1/L2/L4）

- 程序：`portal_probe`。
- 通过标准：输出明确 AVAILABLE / UNSUPPORTED；UNSUPPORTED 的环境验证兜底路径（`--toggle-panel` 子命令 + DE 快捷键设置绑定）可用。
- 记录：| DE 与版本 | portal 可用性 | 兜底绑定验证 |

## 5. 框选层窗口形态（L1/L2）

- 验证 Slint 全屏无边框窗口在 GNOME/KDE Wayland 上能盖在静态帧图上做框选（坐标转换与 Mac 的 ScreenshotGeometry 同规则：记录逻辑坐标→像素坐标的取整）。
- KDE 附加题：验证 wlr-layer-shell 真覆盖层与 Slint（winit 后端）的结合可行性；**不可行则 KDE 与 GNOME 统一用全屏普通窗口**，不算失败，回填 README 第 4/5 节。
- 记录：| DE | 全屏窗口框选 | layer-shell 可行性 |

## 6. 默认热键冲突检查（四台环境）

- 候选：`Ctrl+Alt+M / S / V / L`。
- 步骤：系统与常见应用（浏览器、输入法、截图工具）已占用组合键逐一核对；有冲突则在 W1/L1/L2/L3 上各选一组合适替代。
- 产出：冻结的默认热键表，回填 README 5.2 节。

## 决策门

- 第 1、2 条任一不达标 → 换 UI 框架（egui → iced），core 层计划不变，重测第 1、2 条。
- 第 3 条 GNOME 或 KDE 取不到流 → 该 DE 降级为 Screenshot portal 路线并回填文档（需用户确认）。
- 其余条目只影响体验矩阵，不阻塞。
