# MyQuickPasteSlint

一个基于 Rust + Slint 的轻量级 Windows 快捷粘贴工具，用来管理本地常用文本并一键复制。

## 功能概览

- 全局快捷键呼出，默认 `Alt+V`
- Tab 分组管理，最多 `4` 个分组
- 每个分组最多 `8` 条内容
- 单击内容或复制图标即可复制
- 支持拖拽重排 Tab 和内容
- 独立 `Settings` 窗口
- 支持开机启动、快捷键录制、配置导入导出
- 本地存储，不依赖云服务

## 当前状态

- 主页和 `Settings` 已统一为同一套视觉风格
- 已补充跨 PC / 高 DPI 兼容性修复
- 当前重点兼容场景包括：
  - `125%` / `150%` Windows 缩放
  - 首次启动与托盘打开的窗口定位
  - 小屏或可用高度较紧张时的窗口收缩显示

## 运行与构建

开发检查：

```powershell
cargo check
cargo test
```

生成 release：

```powershell
cargo build --release
```

如果默认 `target/release/MyQuickPaste.exe` 被系统占用，可使用兼容构建目录：

```powershell
cargo build --release --target-dir target-release-compat
```

## Release 包

当前仓库内同步保留的可执行文件：

- `artifacts/MyQuickPaste.exe`

这是便于直接验证和分发的 release 包路径。

## 主要目录

- `src/`：Rust 业务逻辑、配置与平台适配
- `ui/`：Slint 界面定义
- `assets/`：图标、Windows manifest 等资源
- `docs/`：需求与专项方案文档
- `artifacts/`：可直接使用的 exe 输出

## 相关文档

- `docs/product-requirements.md`
- `docs/ui-refactor-plan.md`
- `docs/display-compatibility-fix-plan.md`
