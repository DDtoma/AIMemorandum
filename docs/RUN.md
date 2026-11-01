# AIMemorandum 首版开发运行指南

状态：**Phase 1 打通完成**（2026-10-14）。文字/截图快捷记录 → 本地保存 → 同步 → 提醒的完整链路已实现并通过端到端冒烟。GUI 的热键、截图授权、通知弹窗需在真实桌面环境实机验证。

## 一键验证（无界面）

```bash
# 1. 启动服务端（注册码会打印在日志里）
export PATH=/usr/local/go/bin:$PATH
cd services/api
go build -o /tmp/memo-api ./cmd/api
/tmp/memo-api -addr 127.0.0.1:8787 -data /tmp/memo-api-data &

# 2. 跑客户端端到端冒烟
cd ../../apps/macos
swift build
MEMO_URL=http://127.0.0.1:8787 MEMO_ENROLL=<日志里的注册码> \
  .build/out/Products/Debug/memorandum-smoke

# 3. 单元测试
swift run memorandum-tests        # Swift 9 项
cd ../../services/api && go test ./...   # Go 5 项
```

## 安装与启动独立 Mac 应用

```bash
# 在项目根目录构建和安装（开发或更新时执行）
bash scripts/build-macos-app.sh
bash scripts/install-macos-app.sh
open "$HOME/Applications/AIMemorandum.app"
```

日常直接在“用户 → Applications”双击 **AIMemorandum.app**，或用 Spotlight 搜索 AIMemorandum；应用运行后常驻菜单栏，不需要打开终端。应用标识固定为 `com.aimemorandum.app`。裸二进制/`swift run AIMemorandum` 只会转交 LaunchServices 打开已安装应用，不再直接启动具有录屏功能的记录进程。

首次按 ⌥⌘S 时，按系统提示给 **AIMemorandum** 开启屏幕录制权限。该授权与以前给终端、编辑器或其他开发工具的授权独立，不自动继承。设置位置：系统设置 → 隐私与安全 → 屏幕与系统音频录制。应用只捕获用户选定的静态区域，不录音。

当前为本机 ad-hoc 签名构建；已验证 bundle 签名与运行时应用身份，没有 Developer ID 公证。更新本地构建后 macOS 仍可能再次要求确认权限；后续使用固定 Developer ID 签名发布，不更改 Bundle ID 和安装位置。

数据目录仍是 `~/Library/Application Support/AIMemorandum/`，与开发试用版共用，不搬移、不清空原有记录和同步配置。

首次启动会弹出"连接同步服务"：填 `http://127.0.0.1:8787` 和注册码。点"暂时离线"也可以先记，之后从菜单栏 📎 → 服务端设置 再连。

## Windows / Linux 本地打包（不做分发）

本轮只验证**本地打包**：没有分发平台，不做新 CI/CD、上传、发布、签名或公证，不改 `.github/workflows/linux-desktop.yml`。尚未做干净系统验收，也未用安装包完成「注册 → 记录 → 同步 → 提醒」端到端验收。命令均从仓库根目录执行，产物写入已被 Git 忽略的 `apps/desktop/dist`，不要提交大型产物。

### 许可与第三方通知

根 `LICENSE` 为 GPLv3 全文。`THIRD_PARTY_NOTICES.md` 由 `apps/desktop/Cargo.lock` 与 `cargo metadata --locked` 生成，含传递依赖、许可证表达式、实际上游文本和缺失边界。需要 Python 3.11+ 与 Cargo：

```bash
python -B scripts/generate-third-party-notices.py
python -B scripts/generate-third-party-notices.py --check
# 全部 crate 源码已缓存时可离线复现：
python -B scripts/generate-third-party-notices.py --offline --check
```

生成器只采用上游 `license_file` 和包根目录许可/通知文件，不编造逐包版权全文。操作系统库、编译器运行时与打包工具不在 Cargo.lock 范围内。

### Windows NSIS

依赖 Rust/Cargo、PowerShell、NSIS 3。脚本通过 `MAKENSIS` 环境变量或 PATH 查找 makensis，不含本机编译器路径。默认构建 release；已有二进制时用 `-SkipBuild`，也可传 `-Binary`、`-OutputDir`、`-Architecture`：

```powershell
# 可选：$env:MAKENSIS = "<你的 NSIS 目录>\Bin\makensis.exe"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-installer.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows-installer.ps1 -SkipBuild
```

产物为 `apps/desktop/dist/AIMemorandum-<version>-x86_64-setup.exe`。安装器始终为目录-only：当前用户权限，不写注册表、不建快捷方式、不启动应用。卸载器仅删除已知载荷及为空的安装目录。隔离验收（`/D=` 必须最后）：

```powershell
$version = "0.1.0"
$installer = "apps/desktop/dist/AIMemorandum-$version-x86_64-setup.exe"
$isolated = Join-Path $PWD ".local/package-verify/windows/AIMemorandum"
& $installer /S "/D=$isolated"
& (Join-Path $isolated "Uninstall.exe") /S
```

只有显式执行 `$isolated/AIMemorandum.exe` 才运行应用；验收脚本不要省略 `/D=` 隔离目录。

### Linux deb 与 AppImage

Rust release 的桌面开发依赖见 `apps/desktop/linux/README.md`。打包只直接需要 bash/ar/tar，不需要 `dpkg-deb` 或 `mksquashfs`。AppImage 使用 AppRun 和外部 appimagetool；工具通过 `APPIMAGETOOL` 环境变量或 PATH 提供：

```bash
# 可选：export APPIMAGETOOL="<你的 appimagetool 路径>"
bash scripts/build-linux-packages.sh --deb --appimage
# 已有 target/release/aimemorandum 时：
bash scripts/build-linux-packages.sh --deb --appimage --skip-build
```

也可传 `--binary`、`--output-dir`、`--architecture`。默认产物为 `apps/desktop/dist/aimemorandum_<version>_amd64.deb` 与 `apps/desktop/dist/AIMemorandum-<version>-x86_64.AppImage`，均带桌面元数据、SVG、许可与通知。AppImage 还在 AppDir 顶层携带已检入的 256×256 `apps/desktop/linux/packaging/aimemorandum.png`，满足 appimagetool 的图标检查；构建时只复制 PNG，不需要转换器。

deb 隔离「安装/卸载」通过解包和删除临时目录完成，不调用系统包管理器或 maintainer scripts：

```bash
version=0.1.0
isolated="$PWD/.local/package-verify/linux/deb"
mkdir -p "$isolated"
ar p "apps/desktop/dist/aimemorandum_${version}_amd64.deb" data.tar.gz | tar -xzf - -C "$isolated"
# 可选人工运行："$isolated/usr/bin/aimemorandum"
rm -rf "$isolated"
```

AppImage 无需安装；可在隔离目录用 `--appimage-extract` 检查，显式运行解包后的 `AppRun`，再删除目录。构建脚本不会自动运行应用。

### 本票回归测试

```bash
python -B scripts/test-desktop-packaging.py
```

测试用临时二进制与工具桩检查包结构、版本/架构、桌面元数据、许可随包、输出名、NSIS 无注册表/快捷方式/自动启动、通知生成和本机路径合同；不安装应用、不修改用户系统，也不替代真实桌面验收。

## 日常操作（步骤最少）

| 动作 | 操作 |
|---|---|
| 文字 | ⌥⌘M → 输入 → Enter（一律存为待办） |
| 查看全部/确认待办 | ⌥⌘L（或菜单栏 📎 → 查看我的记录）：详情可直接编辑标题与正文，保存修改 / 完成 / 重开 / +2小时提醒 / 删除 |
| 状态栏直接看待办 | 点菜单栏图标：待办列表（最多12条，点击即完成） |
| 截图识别文字 | ⌥⌘S / ⌥⌘V：本机 Vision OCR，文字写入正文供检索，收条提示"已存为待办并提取文字" |
| 截图 | ⌥⌘S → 框选松手，自动存为待办（首次需授权"屏幕录制"） |
| 剪贴板速存 | 复制内容 → ⌥⌘V（文字与图片都存为待办） |
| 图片拖入 | ⌥⌘M 打开面板 → 拖入图片 |
| 提醒 | 到期后屏幕右上角弹出：完成 / 稍后 10 分钟 / 知道了 |

Esc 关闭面板；面板失焦自动收起；离线时记录先落本地，恢复后 5 秒内自动同步；同步冲突会保留本地副本等待裁决。

## 代码结构

| 路径 | 内容 |
|---|---|
| `apps/macos/Sources/MemorandumCore/` | 模型、SQLite 本地存储（Outbox/影子版本/冲突表）、同步引擎、提醒调度、API 客户端 |
| `apps/macos/Sources/AIMemorandum/` | 菜单栏 App：热键(Carbon)、快捷面板、截图(ScreenCaptureKit)、提醒弹窗 |
| `apps/macos/Sources/Smoke/` | 端到端冒烟程序 |
| `services/api/internal/domain/` | 领域模型与动作（版本校验、提醒规则） |
| `services/api/internal/store/` | SQLite 存储（事件游标、操作幂等、附件哈希校验落盘） |
| `services/api/internal/api/` | HTTP 路由（创建/动作/附件/同步/注册） |
| `services/api/cmd/api/` | 服务端入口 |

## 已知边界（下一步）

- 服务端暂用 SQLite 打通开发流程，PostgreSQL 迁移按原设计待做（存储层已隔离）。
- 同步冲突目前只记录副本，冲突裁决 UI、SSE 提示流、搜索、历史/回收站未做。
- 截图权限、热键冲突、中文输入法误提交需实机验证（方案风险清单中的 HIGH 项）。
- 独立应用首次录屏授权由 macOS 控制；无需再给启动它的终端或编辑器授权。
- 构建：本机 SwiftPM 的 sandbox-exec 被系统拒绝时，使用 `swift build --disable-sandbox`。
- 记录列表数据实时读本地库（含未同步项）；删除为进回收站语义（deletedAt），撤销功能后续版本提供。
