# AIMemorandum 共享 Slint UI

本 crate 只包含跨 Windows/Linux 共享的视图与交互逻辑。平台差异不得写入这里；Windows/Linux 适配器通过小接口注入。

## 结构

```
Slint 视图 → UiIntent → UiWorker（专属线程） → UiController → apps/desktop/core
                                      ↓
                           可渲染 ViewModel（无设备凭据）
```

- `ui/capture_panel.slint`：文本、剪贴板、图片落盘与 Enter/Shift+Enter 交互。
- `ui/items_window.slint`：筛选、计数、稳定 ID 选择、编辑、完成、重新打开、删除、附件与 +2 小时提醒。
- `ui/reminder_popup.slint`：完成、知道了、同一提醒身份延后 600 秒。
- `ui/settings.slint`：服务地址、注册码、设备名称、注册与自动启动；设备凭据不进入 Slint 模型。
- `ui/screenshot_overlay.slint`：框选手势；controller 统一规范化坐标并取消任一尺寸小于 8px 的选择。

`UiWorker` 独占 `LocalStore` 和 controller。SQLite、图片 I/O、注册网络请求均不在 Slint UI 线程执行。Slint 定时器每 2 秒刷新记录、每 10 秒检查本地提醒。

## 平台适配点

`UiController` 注入 `Screenshot`、`Clipboard`、`Enrollment`、`DesktopShell`、`Clock`。测试使用真实临时 `LocalStore` 和这些接口的 fake。

`SlintShell` 公开五个窗口句柄，供平台 crate：

- 原生图片拖放转换为路径后调用 `capture.invoke_paths_dropped(...)`；立即保存由共享 controller 完成。
- 原生 IME 组字状态写入 `capture.set_ime_marked(...)`；Enter 是否提交最终由 core 的 `InputPolicy` 决定。
- 截图流程由平台显示 `screenshot` 并提供静态帧/取流；共享层只处理框选和保存。
- 托盘只消费 `ViewModel.tray`：打开任务最旧优先、全部数量、最多 12 行、标题 36 个 Unicode 字符加省略号。共享 UI 不直接操作托盘。

`CoreEnrollment` 是真实 core 注册适配器，必须留在 worker 线程。当前设备凭据按既有桌面数据约定保存在 `LocalStore` meta，不进入视图模型。

## 自动化验证

在 `apps/desktop` 执行：

```bash
cargo test --workspace
cargo check --workspace
```

controller 测试覆盖：IME 策略、保存失败草稿保留、筛选/计数/稳定 ID、dirty 刷新保护、反向框选与 8px 阈值、提醒身份与 600 秒延后、设置顺序、拖图/剪贴板真实落盘、托盘 12/36。worker 测试确认 controller 不在调用线程执行。

## 真机验收记录（不得用编译或单元测试替代）

| 项目 | 当前结果 | 仍需要的证据 |
|---|---|---|
| Windows/Linux 构建 | 当前 Windows GNU 主机通过 `cargo check`；Linux 未测 | Linux 构建日志 |
| 中文 IME 组字 Enter | 未做真机验证 | 组字中 Enter 不提交、上屏后 Enter 提交的日志或录屏 |
| 窗口焦点/隐藏后草稿 | 未做真机验证 | 切出、隐藏、重开后草稿与焦点行为记录 |
| 原生图片拖放 | 未做真机验证；目前只验证平台回调到真实落盘 | Windows/Linux 各一次原生拖图结果 |
| 原生截图/权限/像素坐标 | 未做真机验证；目前只验证共享几何逻辑 | 授权、缩放与多显示器实测 |
| 托盘与全局热键 | 未做真机验证；平台实现属于后续系统票 | 菜单、12/36、热键冲突结果 |
| 内存 | 未测量共享 UI 最终进程 | 托盘 ≤60MB、记录窗 ≤80MB、稳态 <100MB 的采样 |

这些未完成项必须在对应支持平台补测，不能标记为已通过。
