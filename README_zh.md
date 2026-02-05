<div align="center">

# zcode

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![English](https://img.shields.io/badge/README-English-green.svg)](README.md)
[![Chinese](https://img.shields.io/badge/README-中文-red.svg)](README_zh.md)

</div>

`zcode` 是一个基于 Rust 开发的现代化、高性能 TUI（终端用户界面）文本编辑器。

## 目录

- [快速开始](#快速开始)
- [安装](#安装)
- [操作指南](#操作指南)
  - [常用快捷键 (Keybindings)](#常用快捷键-keybindings)
  - [鼠标操作](#鼠标操作)
- [核心功能](#核心功能)
- [配置](#配置)
- [常见问题](#常见问题)
- [开发](#开发)
- [License](#license)

## 快速开始

### 环境依赖

*   [Rust toolchain](https://rustup.rs/)（推荐使用 `rust-toolchain.toml` 固定的版本）
*   Linux: 需要 C 工具链用于链接（例如 `cc`/`gcc`）
*   终端字体建议使用 Nerd Font（用于显示 UI 图标）

### 启动编辑器

在项目根目录下运行：

```bash
# 1. 打开当前目录
cargo run

# 2. 显式打开当前目录
cargo run -- .

# 3. 打开指定文件
cargo run -- src/main.rs
```

### 推荐：Release 模式运行

TUI 编辑器在 Debug 模式下可能会因为大量绘制计算显得略有卡顿，推荐使用 Release 模式体验流畅度：

```bash
cargo run --release -- .
```

## 安装

从源码构建并安装到 Cargo 的 bin 目录（通常是 `~/.cargo/bin`）：

```bash
cargo install --path . --locked
zcode .
```

也可以使用辅助脚本：

```bash
./install.sh --user
# 或者：sudo ./install.sh --system
```

## 操作指南

### 常用快捷键 (Keybindings)

默认键位映射如下：

*   **通用**:
    *   `Ctrl + q`: 退出编辑器
    *   `Ctrl + s`: 保存当前文件
    *   `Ctrl + ,`: 打开设置 (Settings)
    *   `Ctrl + b`: 切换侧边栏显示

*   **编辑器**:
    *   `Ctrl + f`: 打开文件内搜索
    *   `Ctrl + h`: 打开替换
    *   `F3` / `Ctrl + g`: 查找下一个
    *   `Shift + F3` / `Ctrl + Shift + g`: 查找上一个
    *   `Ctrl + \`: 垂直拆分编辑器 (Split Vertical)
    *   `Ctrl + Shift + \`: 关闭拆分的编辑器
    *   `Ctrl + Shift + e`: 聚焦文件资源管理器
    *   `Ctrl + Shift + f`: 聚焦全局搜索面板

*   **标签页**:
    *   `Ctrl + w`: 关闭当前标签页
    *   `Ctrl + Tab`: 切换到下一个标签页
    *   `Ctrl + Shift + Tab`: 切换到上一个标签页

*   **光标与选区**:
    *   `Ctrl + a`: 全选
    *   `Ctrl + c` / `x` / `v`: 复制 / 剪切 / 粘贴
    *   `Ctrl + z`: 撤销
    *   `Ctrl + y`: 重做

### 鼠标操作

`zcode` 对鼠标操作有完善的支持：

*   **文件导航**: 在左侧资源管理器中，点击文件夹可展开/折叠，双击文件可打开。
*   **标签页管理**: 
    *   **左键点击**: 切换标签页。
    *   **中键点击**: 关闭标签页。
    *   **点击 'x'**: 关闭标签页。
*   **分屏调整**: 拖动编辑器之间的分割线可调整分屏大小。
*   **光标定位**: 在编辑器区域点击可直接移动光标。

## 核心功能

*   **高性能异步架构**: 利用 `tokio` 运行时处理文件读写与全局搜索，确保主界面渲染线程永远流畅，不会因为大文件加载或搜索而卡顿。
*   **现代化 UI**: 基于 `ratatui` 和 `crossterm` 构建，支持侧边栏 (Sidebar)、活动栏 (Activity Bar)、底部面板 (Panel)、多标签页 (Tabs) 和灵活的分屏编辑 (Split Panes)。
*   **强大的搜索**: 内置基于 `ripgrep` 的高性能全局搜索和文件内实时搜索。
*   **键位映射**: 支持灵活的键位绑定配置 (Keybindings)，用户可自定义快捷键。
*   **剪贴板集成**: 与系统剪贴板无缝互通。

## 配置

配置文件位置：

*   Linux: `~/.cache/.zcode/setting.json`
*   macOS: `~/Library/Caches/.zcode/setting.json`

## 常见问题

### `error: linker cc not found`

请安装 C 工具链。

*   Ubuntu/Debian: `sudo apt install build-essential`
*   Fedora/RHEL: `sudo dnf install gcc`
*   Arch: `sudo pacman -S base-devel`

### Linux 上剪贴板不可用

请安装以下任意一个：

*   Wayland: `wl-clipboard`（`wl-copy` / `wl-paste`）
*   X11: `xclip` 或 `xsel`

### Rust LSP（`rust-analyzer`）找不到

确保系统存在 `rust-analyzer` 且在 PATH 中。如果你用 `rustup`：

```bash
rustup component add rust-analyzer rust-src
```

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## License

GPL-3.0，详见 `LICENSE`。
