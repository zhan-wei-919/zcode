# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

zcode 是一个用 Rust 编写的 TUI（Terminal User Interface）文本编辑器，使用 ratatui 作为 UI 框架。项目目标是构建一个类似 VSCode 的编辑器架构，支持文件树管理、文本编辑、鼠标交互等功能。

## Build & Test Commands

### 构建与运行
```bash
# 编译项目
cargo build

# 运行编辑器（需要提供路径参数）
cargo run -- <path_to_directory>

# 示例：打开当前目录
cargo run -- .
```

### 测试
```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test <test_name>

# 运行集成测试（tests/ 目录下）
cargo test --test <test_file_name>

# 示例：运行文件系统相关测试
cargo test file_system
```

### 代码检查
```bash
# 运行 clippy 进行代码检查
cargo clippy

# 自动格式化代码
cargo fmt
```

## Architecture Overview

### 模块结构

项目采用模块化设计，主要分为三个核心模块：

```
zcode/
├── src/
│   ├── main.rs           # 入口点，初始化 TUI
│   ├── lib.rs            # 库入口，导出公共模块
│   ├── workspace.rs      # 工作区：顶层状态容器
│   ├── file_system/      # 文件树管理
│   │   └── mod.rs
│   └── editor/           # 文本编辑器
│       ├── mod.rs        # 统一导出和渲染逻辑
│       ├── core/         # 核心组件（Editor, TextModel, EditorView）
│       ├── layout/       # 布局引擎（处理 Tab 展开和缓存）
│       ├── input/        # 输入处理（键盘、鼠标、命令系统）
│       └── config/       # 配置管理
├── tests/                # 集成测试
└── docs/                 # 设计文档
```

### 核心架构设计

#### 1. Workspace（工作区）
- **职责**：聚合文件树和编辑器模块，作为顶层状态容器
- **位置**：`src/workspace.rs`
- **原则**：高内聚低耦合，模块间通过 Workspace 协调但互不依赖

#### 2. FileSystem（文件系统）
- **数据结构**：使用 `SlotMap<NodeId, Node>` 平坦存储树节点
- **关键设计**：
  - 节点仅存储 basename，完整路径按需计算并缓存
  - 使用 `BTreeMap` 保持子节点有序（目录优先）
  - 支持 rename、move、delete 等操作，带完整错误处理
- **路径缓存**：带失效机制，在 rename/move 时递归失效子树缓存
- **未来规划**：计划引入 Provider 模式抽象文件系统（见 `docs/file_system_design.md`）

#### 3. Editor（编辑器）
采用 **MVC 分层架构**：

##### Model（数据层）- `TextModel`
- 基于 `ropey::Rope` 实现高效文本操作
- 支持选区管理（`Selection` 结构）
- 提供插入、删除、复制、粘贴等核心操作
- **位置**：`src/editor/core/text_model.rs`

##### View（视图层）- `EditorView`
- 管理视口状态（垂直/水平滚动）
- 缓存布局计算结果（`LayoutEngine`）
- 处理 Tab 展开和 grapheme 索引映射
- **位置**：`src/editor/core/view.rs`

##### Controller（控制层）- `Editor`
- 协调 Model 和 View
- 处理命令（Command）和键绑定（Keybindings）
- 管理光标位置和编辑模式
- **位置**：`src/editor/core/state.rs`

##### Input System（输入系统）
- **Command 模式**：所有编辑操作统一抽象为 Command
- **Keybindings**：可配置的键绑定系统
- **MouseController**：处理鼠标点击和拖拽选区
- **位置**：`src/editor/input/`

##### Layout Engine（布局引擎）
- **核心职责**：将逻辑光标位置转换为屏幕显示坐标
- **关键优化**：
  - 缓存 Tab 展开后的文本和 grapheme 索引
  - 避免重复计算 Unicode 字符宽度
- **位置**：`src/editor/layout/`

### 关键技术细节

#### 1. Unicode 处理
- 使用 `unicode-segmentation` 处理 grapheme clusters（字素簇）
- 使用 `unicode-width` 计算字符显示宽度
- Tab 展开：对齐到 `tab_size` 的倍数（默认 4）

#### 2. 选区高亮渲染
- 行级别判断选区相交（优化性能）
- 使用 `ratatui::text::Span` 分段渲染高亮
- 支持多行选区和水平滚动

#### 3. 视口管理
- 垂直滚动：根据光标位置自动调整 `viewport_offset`
- 水平滚动：当光标超出屏幕宽度时自动滚动
- 只渲染可见行（性能优化）

#### 4. 文件树渲染
- 使用 `flatten_for_view()` 将树结构拍扁为列表
- 目录优先排序（先显示文件夹）
- 仅渲染展开节点的子节点

### 设计原则

根据 `docs/file_system_design.md`，项目遵循以下原则：

1. **抽象解耦**：文件树逻辑与具体实现分离（未来引入 Provider 模式）
2. **懒写入**：操作立即在内存生效，支持批量提交（规划中）
3. **事务性**：支持 commit/rollback（规划中）
4. **可扩展**：架构设计参考 VSCode，支持远程文件系统等扩展

## Development Guidelines

### 编辑器模块开发

- **添加新命令**：在 `src/editor/input/command.rs` 定义 Command，在 `TextModel` 实现逻辑
- **修改键绑定**：编辑 `src/editor/input/keybindings.rs`
- **调整渲染**：修改 `src/editor/mod.rs` 中的 `render()` 函数
- **性能优化**：优先考虑缓存（如 `LayoutEngine::cached_layout`）

### 文件系统模块开发

- **操作约束**：所有修改操作（rename/move/delete）需要带错误处理
- **路径缓存**：修改树结构后务必调用 `invalidate_path_cache_subtree()`
- **测试覆盖**：新增操作必须添加集成测试（见 `tests/` 目录）

### 测试策略

- **单元测试**：放在模块内（`#[cfg(test)] mod tests`）
- **集成测试**：放在 `tests/` 目录，测试跨模块交互
- **命名约定**：测试文件以 `_test.rs` 结尾（如 `architecture_test.rs`）

## Known Issues & TODOs

根据代码和设计文档，以下是待完成的功能：

### FileSystem Module
- [ ] 实现 `FileSystemProvider` trait（抽象层）
- [ ] 实现懒写入机制（`PendingOperation` + `commit()`）
- [ ] 添加文件监控（`watch()` API）
- [ ] 支持懒加载（展开时才读取子目录）

### Editor Module
- [ ] 完善鼠标交互（目前仅支持基础点击）
- [ ] 实现 undo/redo（需要 Command History）
- [ ] 支持多光标编辑
- [ ] 语法高亮（需要集成 tree-sitter）

### UI/UX
- [ ] 文件树选中状态高亮
- [ ] 状态栏显示更多信息（文件类型、编码等）
- [ ] 命令面板（类似 VSCode 的 Ctrl+Shift+P）

## Performance Considerations

- **文件树构建**：`build_from_path()` 过滤 `target/`、`node_modules/`、`.git/` 提升性能
- **渲染优化**：仅渲染可见行（`visible_start..visible_end`）
- **布局缓存**：`LayoutEngine` 缓存 Tab 展开结果，避免重复计算
- **路径缓存**：`FileTree::full_path()` 使用 `HashMap` 缓存，减少向上遍历

## Dependencies

核心依赖说明：
- **ratatui**: TUI 框架，提供布局和组件
- **crossterm**: 跨平台终端控制（键盘/鼠标事件）
- **ropey**: 高效的文本 rope 数据结构（支持大文件）
- **slotmap**: 带代数的 ID 分配器（防止悬垂引用）
- **unicode-segmentation**: 处理 Unicode grapheme clusters
- **unicode-width**: 计算字符显示宽度（支持中文等宽字符）
- **walkdir**: 递归遍历目录
- **rustc-hash**: 快速哈希表（`FxHashSet/FxHashMap`）

## Code Style

- **模块组织**：使用 `mod.rs` 统一导出子模块
- **文档注释**：公共 API 需要 `///` 文档注释
- **错误处理**：优先使用自定义 Error 类型（如 `FsTreeError`）
- **命名约定**：遵循 Rust 标准（snake_case for functions, PascalCase for types）
