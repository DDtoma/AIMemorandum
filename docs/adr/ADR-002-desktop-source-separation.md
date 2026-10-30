# 共享桌面核心，平台源码按 crate 分离

状态：已接受（2026-10-30）。

Windows 与 Linux 客户端继续使用同一 Rust + Slint 技术栈，但不复制公共逻辑。`apps/desktop/core` 与 `apps/desktop/ui` 提供共享实现，`apps/desktop/windows` 和 `apps/desktop/linux` 分别承载独立入口、平台集成与平台测试；共享 JSON fixtures 固定跨端契约。

## 考虑过的方案

- 两个完全独立客户端：源码分离最彻底，但存储、同步和提醒逻辑重复，容易发生跨平台语义发散，维护者明确否决。
- 单一 crate 内使用 `cfg`：复用最好，但平台差异源码和测试混杂，不满足维护者的交付结构要求。

## 后果

公共行为只能在 core/ui 实现一次；平台差异不得反向渗入公共 crate。平台构建产物同样分目录，真实桌面验收按平台分别记录。
