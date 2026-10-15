# 截图选区错位与记录窗口遮挡修复

日期：2026-10-15。

## 问题与原因

用户框选区域与实际保存区域不同；截图时“我的记录”自动前置，可能遮挡目标内容。

原实现把 SelectionView 的本地坐标当成全局坐标，并且没有把 AppKit 左下原点转换成 ScreenCaptureKit 的左上原点。显示器还通过 `SCDisplay.frame == NSScreen.frame` 匹配，这两套坐标在副屏布局中不能直接比较。框选窗口调用 `NSApp.activate`，会将本应用的其他窗口带到前台。

## 修改

- `ScreenshotGeometry` 接收屏幕内选区，转换为左上原点，裁剪边界并按捕获比例对齐像素。
- 使用 `NSScreenNumber` 对应 `SCDisplay.displayID`，不再通过全局 frame 判断显示器。
- 使用 SCK 的 `pointPixelScale` 同时计算 sourceRect 和输出尺寸。
- 框选窗口改为带 nonactivatingPanel 样式的 NSPanel，移除显式激活应用的调用。
- 框选前收起输入面板和“我的记录”，保存/取消后不自动恢复；已有草稿没有被清空。
- 抓屏过滤器排除本进程窗口；框选和捕获期间暂停弹出新的提醒窗口。
- 选区松手时更新最后坐标并先关闭遮罩；Esc 与取消走同一条清理路径。
- 截图出错时在菜单栏显示原因，不自动打开输入面板。

## 实际验证

1. `swift run --disable-sandbox --scratch-path .build memorandum-tests`：15/15 通过。新增用例覆盖上/下区域、像素对齐、屏幕边界、位于左侧/上方的副屏坐标。
2. `swift build --disable-sandbox --scratch-path .build --product AIMemorandum`：编译成功；工具链和既有代码仍有 warning。
3. 原生窗口回归：显示测试记录窗口，执行 begin，确认记录窗口隐藏；执行取消后确认遮罩已清理、记录窗口未弹回。两项通过。
4. 原生真实像素回归：macOS 未授予当前诊断进程屏幕录制权限，明确 SKIP，未声称通过。探针只会对另一测试进程绘制的色块抓屏，并比较上/下区域颜色、尺寸和本地保存结果。
5. 已退出旧试用进程并启动同路径修复版；没有迁移或修改用户事项数据库结构。

诊断命令（在项目根目录执行；目录只用于测试资料）：

```sh
MEMO_PROBE=capture-regression \
MEMO_CAPTURE_PROBE_ROOT="$PWD/.dev/capture-regression-20261015" \
apps/macos/.build/out/Products/Debug/AIMemorandum
```

退出码 0 表示全部检查通过；1 表示失败；2 表示窗口检查已通过、实际抓屏因权限不足而跳过。诊断模式不会申请、修改或绕过系统录屏权限。CLI 诊断进程首次取得 key window 时实测 isActive 从 false 变为 true；本次验收断言的是记录窗口不显示或遮挡，没有宣称所有系统下进程激活状态完全不变。

仍需用户环境确认：已有授权下的实际框选内容、不同外接显示器和缩放布局。本次不把模拟副屏坐标测试当成多屏实机验证。
