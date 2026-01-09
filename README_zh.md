<div align="center">

# zcode

[![Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![English](https://img.shields.io/badge/README-English-green.svg)](README.md)
[![Chinese](https://img.shields.io/badge/README-中文-red.svg)](README_zh.md)

</div>

`zcode` 是一个基于 Rust 开发的高性能、现代化 TUI（终端用户界面）文本编辑器。它采用了类 Flux/Redux 的状态管理架构，并结合 Tokio 异步运行时，实现了高效的 I/O 处理和插件扩展能力，旨在提供流畅的终端编码体验。

## 目录

- [快速开始](#快速开始)
- [操作指南](#操作指南)
- [核心功能](#核心功能)
- [项目架构](#项目架构)
- [模块详解](#模块详解)
- [插件系统](#插件系统)

## 快速开始

### 环境依赖

*   [Rust Toolchain](https://rustup.rs/) (1.70+)

### 启动编辑器

在项目根目录下运行：

```bash
# 1. 打开当前目录
cargo run -- .

# 2. 打开指定文件
cargo run -- src/main.rs
```

### 推荐：Release 模式运行

TUI 编辑器在 Debug 模式下可能会因为大量绘制计算显得略有卡顿，推荐使用 Release 模式体验流畅度：

```bash
cargo run --release -- .
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

*   **高性能异步架构**: 利用 `tokio` 运行时处理文件读写、全局搜索和外部进程通信，确保主界面渲染线程永远流畅，不会因为大文件加载或搜索而卡顿。
*   **现代化 UI**: 基于 `ratatui` 和 `crossterm` 构建，支持侧边栏 (Sidebar)、活动栏 (Activity Bar)、底部面板 (Panel)、多标签页 (Tabs) 和灵活的分屏编辑 (Split Panes)。
*   **强大的搜索**: 内置基于 `ripgrep` 的高性能全局搜索和文件内实时搜索。
*   **插件扩展**: 支持通过 stdio (JSON-RPC) 协议加载外部插件，实现语言服务或其他功能扩展（架构上支持多语言编写插件）。
*   **键位映射**: 支持灵活的键位绑定配置 (Keybindings)，用户可自定义快捷键。
*   **剪贴板集成**: 与系统剪贴板无缝互通。

## 项目架构

本项目采用了清晰的分层架构，主要遵循 **Model-View-Intent (MVI)** 或类 **Flux/Redux** 的单向数据流模式。

### 核心数据流

1.  **Event**: 用户输入（键盘、鼠标）或系统消息。
2.  **Dispatcher**: 将 Event 转换为业务含义的 `Action`。
3.  **Store**: 接收 `Action`，更新全局 `State`。
4.  **View**: 根据最新的 `State` 渲染 UI。

## 模块详解

### 1. 核心层 (Kernel) - `src/kernel/`

这是编辑器的"大脑"，纯逻辑层，不依赖具体的 UI 渲染库。

*   **State (`state.rs`)**:
    *   这是单一事实来源 (Single Source of Truth)。包含 UI 布局状态（哪个 Sidebar 可见、哪个 Tab 激活）、编辑器数据（打开的文件、光标位置）、数据缓存等。
*   **Store (`store.rs`)**:
    *   状态容器。它不仅持有 State，还负责接收 Actions 并执行状态转换（Reducers）。
    *   它也管理副作用 (Side Effects) 的触发。
*   **Actions (`action.rs`)**:
    *   定义了所有可能改变状态的操作枚举，例如 `EditorAction::InsertText`, `WorkbenchAction::ToggleSidebar` 等。

### 2. 应用层 (App) - `src/app/`

这层负责将 Kernel 与 TUI 渲染连接起来，其中最核心的是 **Workbench**。

*   **Workbench (`src/app/workbench/`)**:
    *   **生命周期管理**: 初始化服务、加载配置、启动并维持主事件循环 (Event Loop)。
    *   **事件分发**: 监听 `crossterm` 的底层事件，转换为 Kernel能理解的 `InputEvent` 并分发。
    *   **整体布局**: 负责计算屏幕上各个区域（侧边栏、编辑器、状态栏）的大小和位置 (Rect)，并将绘制任务分发给子组件。

### 3. 视图层 (Views) - `src/views/`

负责具体的 UI 组件绘制逻辑。

*   **Editor**: 复杂的文本渲染逻辑，处理滚动、行号绘制、语法高亮（基础）、光标跟随等。
*   **Explorer**: 文件资源管理器树状视图，处理文件夹的展开/折叠逻辑。
*   **Search**: 搜索面板视图，展示全局搜索结果列表。

### 4. 服务与适配器 (Services & Adapters) - `src/kernel/services/`

负责与操作系统和外部世界交互，通常涉及 IO 操作和异步任务。

*   **AsyncRuntime**: 封装 `tokio` 运行时，允许在同步的 TUI 绘制循环之外执行耗时任务。
*   **PluginHost**: 核心服务之一，管理外部插件子进程的启动、保活和通信。
*   **GlobalSearchService**: 封装底层搜索工具（如 `ignore` 和 `grep` crate），提供异步搜索能力。
*   **Clipboard**: 封装 `arboard`，提供跨平台的剪贴板读写。

### 5. 数据模型 (Models) - `src/models/`

*   **TextBuffer**: 基于 `ropey` (Rope 数据结构) 实现。这对于文本编辑器至关重要，它能高效地处理大文本文件的插入和删除操作（O(log N) 复杂度），避免了普通 String 的大量内存拷贝。
*   **EditHistory**: 管理撤销栈 (Undo Stack) 和重做栈 (Redo Stack)。
*   **FileTree**: 递归的文件树结构，用于资源管理器的数据展示。

## 插件系统设计

`zcode` 的插件系统设计灵感来源于 LSP (Language Server Protocol)。

*   **独立进程**: 每个插件运行在独立的子进程中，保证了编辑器的稳定性（插件崩溃不会导致编辑器崩溃）。
*   **通信协议**:
    *   使用标准输入输出 (Stdio) 管道。
    *   消息格式采用 LSP 风格的 Framing: `Content-Length: <len>\r\n\r\n<JSON-RPC Body>`。
*   **双通道机制**:
    *   为了防止大量日志或低优先级消息阻塞 UI 响应，内部实现了 `High` (用于指令调用、UI 更新) 和 `Low` (用于日志、后台任务) 两个优先级的消息通道。
