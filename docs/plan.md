# zcode 架构重构计划

## 0. 功能实现状态

### 0.1 文件系统模块

| 功能 | 状态 | 说明 |
|------|------|------|
| 文件树构建 | ✅ 已实现 | SlotMap 存储，自动过滤 target/node_modules/.git |
| 展开/折叠目录 | ✅ 已实现 | toggle_expand，支持键盘和鼠标双击 |
| 文件重命名 | ✅ 已实现 | 带重名检查 |
| 文件移动 | ✅ 已实现 | 带循环检测 |
| 文件删除 | ✅ 已实现 | 递归删除子树 |
| 路径缓存 | ✅ 已实现 | 带失效机制 |
| 扁平化渲染 | ✅ 已实现 | 目录优先排序 |
| 创建文件/目录 | ❌ 未实现 | |
| 文件监控 | ❌ 未实现 | watch API，需要异步 |
| 懒加载 | ❌ 未实现 | 展开时才读取 |
| Provider 抽象 | ✅ 已实现 | FileProvider trait + LocalFileProvider |

### 0.2 编辑器模块 - 文本编辑

### 0.2 编辑器模块 - 文本编辑

### 0.2 编辑器模块 - 文本编辑

### 0.2 编辑器模块 - 文本编辑

### 0.2 编辑器模块 - 文本编辑

| 功能 | 状态 | 说明 |
|------|------|------|
| 字符插入 | ✅ 已实现 | insert_char |
| 换行插入 | ✅ 已实现 | InsertNewline |
| Tab 插入 | ✅ 已实现 | InsertTab |
| 退格删除 | ✅ 已实现 | DeleteBackward |
| Delete 键删除 | ✅ 已实现 | DeleteForward |
| 删除整行 | ⚠️ 部分实现 | Ctrl+D 命令已定义，逻辑未实现 |
| 删除到行尾 | ⚠️ 部分实现 | Ctrl+K 命令已定义，逻辑未实现 |
| 复制 | ✅ 已实现 | Ctrl+C，arboard 剪贴板 |
| 粘贴 | ✅ 已实现 | Ctrl+V，支持 bracketed paste |
| 剪切 | ✅ 已实现 | Ctrl+X |
| Undo | ✅ 已实现 | Git 模型 EditHistory，Ctrl+Z |
| Redo | ✅ 已实现 | Git 模型 EditHistory，Shift+Ctrl+Z / Ctrl+Y |

### 0.3 编辑器模块 - 光标移动

| 功能 | 状态 | 说明 |
|------|------|------|
| 上下左右移动 | ✅ 已实现 | 方向键 |
| 行首/行尾 | ✅ 已实现 | Home/End |
| 文件首/尾 | ✅ 已实现 | Ctrl+Home/End |
| 跨行移动 | ✅ 已实现 | 行尾到下行行首 |
| 按词移动 | ⚠️ 部分实现 | Ctrl+Left/Right 命令已定义，逻辑未实现 |

### 0.4 编辑器模块 - 选区

| 功能 | 状态 | 说明 |
|------|------|------|
| 单击定位 | ✅ 已实现 | 字符级别 |
| 双击选词 | ✅ 已实现 | Unicode 单词边界 |
| 三击选行 | ✅ 已实现 | 整行选择 |
| 拖拽选择 | ✅ 已实现 | |
| 选区高亮 | ✅ 已实现 | 蓝底白字 |
| 选区删除 | ✅ 已实现 | delete_selection |
| 全选 | ⚠️ 部分实现 | Ctrl+A 命令已定义，逻辑未实现 |
| 键盘扩展选区 | ❌ 未实现 | Shift+方向键 |
| 多光标 | ❌ 未实现 | |

### 0.5 编辑器模块 - 视图与滚动

| 功能 | 状态 | 说明 |
|------|------|------|
| 垂直滚动 | ✅ 已实现 | |
| 水平滚动 | ✅ 已实现 | |
| 视口管理 | ✅ 已实现 | 支持光标跟随开关 |
| PageUp/Down | ✅ 已实现 | |
| 鼠标滚轮 | ✅ 已实现 | 支持滚出光标，事件合并优化 |
| 行号显示 | ✅ 已实现 | 右对齐 |
| Tab 展开 | ✅ 已实现 | 对齐到 tab_size |
| Unicode 宽度 | ✅ 已实现 | 支持中文 |
| 布局缓存 | ✅ 已实现 | LayoutEngine |

### 0.6 文件操作

| 功能 | 状态 | 说明 |
|------|------|------|
| 打开文件 | ✅ 已实现 | 双击文件树打开到编辑器 |
| 保存文件 | ✅ 已实现 | Ctrl+S |
| 另存为 | ❌ 未实现 | |
| 多 Tab | ✅ 已实现 | EditorGroup 管理 |
| 关闭 Tab | ✅ 已实现 | Ctrl+W |
| Tab 切换 | ✅ 已实现 | Ctrl+Tab |
| 未保存提示 | ✅ 已实现 | dirty 标记，状态栏显示 [+] |

### 0.7 高级功能

| 功能 | 状态 | 说明 |
|------|------|------|
| 查找 | ❌ 未实现 | Ctrl+F |
| 替换 | ❌ 未实现 | Ctrl+H |
| 命令面板 | ❌ 未实现 | Ctrl+Shift+P |
| 语法高亮 | ❌ 未实现 | 需要 tree-sitter |
| 代码折叠 | ❌ 未实现 | |
| 括号匹配 | ❌ 未实现 | |
| 自动缩进 | ❌ 未实现 | |

### 0.8 系统集成

| 功能 | 状态 | 说明 |
|------|------|------|
| 快捷键系统 | ✅ 已实现 | KeybindingService |
| 配置系统 | ✅ 已实现 | EditorConfig |
| 配置文件加载 | ❌ 未实现 | |
| Git 集成 | ❌ 未实现 | 需要异步 |
| 终端集成 | ❌ 未实现 | 需要异步 |
| AI 辅助 | ❌ 未实现 | 需要异步 |
| SSH 远程 | ❌ 未实现 | 需要异步 |

### 0.9 性能优化

| 功能 | 状态 | 说明 |
|------|------|------|
| 事件队列合并 | ✅ 已实现 | 滚轮事件 coalesce，避免卡顿 |
| 滚动跟随控制 | ✅ 已实现 | follow_cursor 开关 |

### 0.10 功能统计

```
已实现:     37 个
部分实现:    4 个
未实现:     15 个
完成度:     约 66%
```

---

## 1. 重构目标

将 zcode 从当前的简单 TUI 编辑器重构为**可扩展的编辑器框架**，支持：
- 多种文件系统后端（本地、SSH、FTP）
- 多种 AI 服务（Claude、OpenAI、本地模型）
- Git 集成
- 终端集成
- 插件系统（远期）

核心原则：**服务注册 + 依赖注入 + Provider 模式**

---

## 2. 当前架构问题

### 2.1 ~~Editor 职责过重~~ ✅ 已解决
已拆分为 EditorView、TextBuffer、Viewport 等独立模块。

### 2.2 ~~输入处理分散~~ ✅ 已解决
统一通过 View trait 的 handle_input 方法处理。

### 2.3 ~~Workspace 太薄~~ ✅ 已解决
Workbench 实现了完整的视图布局和输入分发。

### 2.4 ~~文件系统耦合~~ ✅ 已解决
FileProvider trait 抽象，LocalFileProvider 实现。

### 2.5 ~~缺少服务层~~ ✅ 已解决
ServiceRegistry + Service trait 已实现。

### 2.6 同步架构限制（新问题）
- 所有 I/O 操作都是阻塞的
- 无法支持后台任务（LSP、文件监控、AI）
- 大文件操作会卡住 UI

---

## 3. 目标架构

### 3.1 目录结构 ✅ 已完成

```
src/
├── main.rs                     # ✅ 入口：事件循环 + 事件合并
├── lib.rs                      # ✅ 库导出
│
├── core/                       # ✅ 核心框架
│   ├── mod.rs
│   ├── service.rs              # ✅ Service trait + ServiceRegistry
│   ├── view.rs                 # ✅ View trait + EventResult
│   ├── command.rs              # ✅ Command 枚举
│   ├── event.rs                # ✅ InputEvent 定义
│   └── context.rs              # ✅ AppContext
│
├── services/                   # ✅ 服务实现
│   ├── mod.rs
│   ├── file/                   # ✅ 文件服务
│   │   ├── mod.rs
│   │   ├── provider.rs         # ✅ FileProvider trait
│   │   ├── local.rs            # ✅ LocalFileProvider
│   │   └── service.rs          # ✅ FileService
│   ├── keybinding.rs           # ✅ 快捷键服务
│   └── config.rs               # ✅ 配置服务
│
├── models/                     # ✅ 数据模型
│   ├── mod.rs
│   ├── file_tree.rs            # ✅ 文件树结构
│   ├── text_buffer.rs          # ✅ 文本缓冲区
│   └── selection.rs            # ✅ 选区模型
│
├── views/                      # ✅ 视图层
│   ├── mod.rs
│   ├── explorer/               # ✅ 文件浏览器
│   │   ├── mod.rs
│   │   └── explorer_view.rs    # ✅ 双击展开/打开
│   └── editor/                 # ✅ 编辑器
│       ├── mod.rs
│       ├── editor_group.rs     # ✅ 多 Tab 管理
│       ├── editor_view.rs      # ✅ 单个编辑器
│       └── viewport.rs         # ✅ 视口管理
│
└── app/                        # ✅ 应用组装层
    ├── mod.rs
    └── workbench.rs            # ✅ 工作台
```

### 3.2 核心 Trait 定义 ✅ 已完成

```rust
// core/service.rs ✅
pub trait Service: Any {
    fn name(&self) -> &'static str;
}

// core/view.rs ✅
pub trait View {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult;
    fn render(&mut self, frame: &mut Frame, area: Rect);
    fn cursor_position(&self) -> Option<(u16, u16)>;
}

// services/file/provider.rs ✅
pub trait FileProvider: Send + Sync {
    fn scheme(&self) -> &'static str;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    fn read_file(&self, path: &Path) -> Result<String>;
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;
    // ... 更多方法
}
```

---

## 4. 重构阶段

### Phase 1: 核心框架搭建 ✅ 已完成

- [x] 创建 `core/service.rs` - Service trait + ServiceRegistry
- [x] 创建 `core/view.rs` - View trait
- [x] 创建 `core/event.rs` - InputEvent 定义
- [x] 创建 `core/context.rs` - AppContext
- [x] 创建 `core/command.rs` - Command 系统基础

### Phase 2: 服务层重构 ✅ 已完成

- [x] 创建 `services/file/provider.rs` - FileProvider trait
- [x] 创建 `services/file/local.rs` - LocalFileProvider
- [x] 创建 `services/file/service.rs` - FileService
- [x] 创建 `services/keybinding.rs` - KeybindingService
- [x] 创建 `services/config.rs` - ConfigService

### Phase 3: 模型层重构 ✅ 已完成

- [x] 创建 `models/file_tree.rs` - 文件树数据结构
- [x] 创建 `models/text_buffer.rs` - 文本缓冲区
- [x] 创建 `models/selection.rs` - 选区模型

### Phase 4: 视图层重构 ✅ 已完成

- [x] 创建 `views/explorer/explorer_view.rs` - 实现 View trait
- [x] 创建 `views/editor/editor_view.rs` - 实现 View trait
- [x] 创建 `views/editor/editor_group.rs` - 多 Tab 管理
- [x] 创建 `views/editor/viewport.rs` - 视口管理

### Phase 5: 应用层重构 ✅ 已完成

- [x] 创建 `app/workbench.rs` - 视图布局 + 输入分发
- [x] 重写 `main.rs` - 事件循环 + 事件合并优化
- [x] 实现鼠标点击切换 active_area
- [x] 实现全局快捷键处理
- [x] 实现双击展开目录/打开文件

### Phase 6: 功能增强（进行中）

- [x] 多 Tab 支持（切换、关闭）
- [x] 文件树键盘导航（上下选择、Enter 打开）
- [x] 文件树鼠标双击展开/打开
- [x] Undo/Redo（Git 模型 EditHistory）
- [x] 复制/粘贴/剪切（arboard + bracketed paste）
- [ ] 查找/替换
- [ ] 命令面板（Ctrl+Shift+P）

### Phase 7: 异步架构重构（待开始）

见下方 **异步架构方案**。

### Phase 8: 扩展服务（远期）

- [ ] `services/git/` - Git 集成
- [ ] `services/ssh/` - SSH 远程文件
- [ ] `services/ai/` - AI 补全和对话
- [ ] `views/terminal/` - 内置终端
- [ ] `services/lsp/` - LSP 客户端

---

## 5. 异步架构方案

### 5.1 为什么需要异步

当前架构是完全同步的，存在以下限制：

| 问题 | 影响 |
|------|------|
| 阻塞 I/O | 大文件读取会卡住 UI |
| 无后台任务 | 无法实现文件监控、LSP、AI |
| 单线程 | 无法并行处理多个任务 |

### 5.2 异步需求分类

#### 必须异步（阻塞会导致 UI 卡死）

| 功能 | 原因 |
|------|------|
| LSP 通信 | 独立进程，持续双向通信 |
| AI 补全/对话 | 网络请求，延迟不可控 |
| SSH 远程文件 | 网络 I/O，延迟高 |
| 文件监控 | 持续监听，不能阻塞主循环 |
| 终端集成 | 子进程 I/O，实时读取输出 |
| 大文件加载 | 100MB+ 文件会卡住 UI |

#### 建议异步（提升体验）

| 功能 | 原因 |
|------|------|
| 文件保存 | 大文件或网络盘可能慢 |
| 文件树构建 | 大项目可能需要几秒 |
| 全局搜索 | 搜索大量文件需要后台执行 |
| 语法高亮 | tree-sitter 解析大文件耗时 |
| 自动保存 | 定时任务 |
| Git 状态 | 调用 git 命令可能慢 |

#### 保持同步

| 功能 | 原因 |
|------|------|
| 事件处理 | 必须在主线程，保证顺序 |
| UI 渲染 | ratatui 不支持多线程渲染 |
| 小文件读写 | 本地 SSD 几乎瞬时完成 |
| 光标移动/编辑 | 纯内存操作 |

### 5.3 目标架构

```
┌─────────────────────────────────────────────────────────────┐
│                      Main Thread (sync)                      │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    Event Loop                        │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐            │   │
│  │  │ Terminal│  │ Channel │  │  Timer  │            │   │
│  │  │ Events  │  │ Recv    │  │ (tick)  │            │   │
│  │  └────┬────┘  └────┬────┘  └────┬────┘            │   │
│  │       └───────────┬┴───────────┘                   │   │
│  │                   ▼                                 │   │
│  │            ┌─────────────┐                         │   │
│  │            │   Dispatch  │                         │   │
│  │            └──────┬──────┘                         │   │
│  │                   ▼                                 │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐            │   │
│  │  │ Update  │─▶│  State  │─▶│ Render  │            │   │
│  │  └─────────┘  └─────────┘  └─────────┘            │   │
│  └─────────────────────────────────────────────────────┘   │
│                          ▲                                  │
│                          │ mpsc channel                     │
│                          ▼                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Async Runtime (tokio)                   │   │
│  │  ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐          │   │
│  │  │  LSP  │ │  AI   │ │ Watch │ │Search │  ...     │   │
│  │  │Client │ │Service│ │Service│ │Service│          │   │
│  │  └───────┘ └───────┘ └───────┘ └───────┘          │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 5.4 核心组件

#### AppMessage - 跨线程消息

```rust
pub enum AppMessage {
    // 文件操作结果
    FileLoaded { path: PathBuf, content: String },
    FileSaved { path: PathBuf },
    FileError { path: PathBuf, error: String },

    // 文件监控
    FileChanged { path: PathBuf },
    FileCreated { path: PathBuf },
    FileDeleted { path: PathBuf },

    // LSP 消息
    LspDiagnostics { uri: String, diagnostics: Vec<Diagnostic> },
    LspCompletion { items: Vec<CompletionItem> },
    LspHover { content: String },

    // AI 消息
    AiResponse { request_id: u64, content: String },
    AiStreamChunk { request_id: u64, chunk: String },

    // 搜索结果
    SearchResult { query: String, matches: Vec<SearchMatch> },
    SearchProgress { query: String, files_searched: usize },

    // 定时任务
    Tick,
    AutoSave,
}
```

#### AsyncRuntime - 异步任务管理

```rust
pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
    tx: mpsc::Sender<AppMessage>,
}

impl AsyncRuntime {
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    pub fn load_file_async(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.spawn(async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let _ = tx.send(AppMessage::FileLoaded { path, content });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FileError {
                        path,
                        error: e.to_string()
                    });
                }
            }
        });
    }
}
```

#### 新的主循环

```rust
fn run_app(terminal: &mut Terminal<...>) -> io::Result<()> {
    let (tx, rx) = mpsc::channel::<AppMessage>();
    let async_runtime = AsyncRuntime::new(tx);
    let mut workbench = Workbench::new(async_runtime)?;

    loop {
        // 1. 渲染
        terminal.draw(|f| workbench.render(f, f.area()))?;

        // 2. 等待事件（带超时，用于定时任务）
        if event::poll(Duration::from_millis(16))? {
            let ev = event::read()?;
            let events = drain_pending_events(ev);
            for ev in events {
                if workbench.handle_input(&ev.into()) == EventResult::Quit {
                    return Ok(());
                }
            }
        }

        // 3. 处理异步消息
        while let Ok(msg) = rx.try_recv() {
            workbench.handle_message(msg);
        }
    }
}
```

### 5.5 实施步骤

#### Step 1: 添加依赖

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "fs"] }
```

#### Step 2: 创建 runtime 模块

```
src/
└── runtime/
    ├── mod.rs          # 模块导出
    ├── message.rs      # AppMessage 定义
    └── runtime.rs      # AsyncRuntime 实现
```

**message.rs** - 先定义基础消息类型，后续按需扩展：

```rust
use std::path::PathBuf;

pub enum AppMessage {
    // 文件树懒加载
    DirLoaded {
        path: PathBuf,
        entries: Vec<DirEntryInfo>,
    },
    DirLoadError {
        path: PathBuf,
        error: String,
    },

    // 文件内容加载
    FileLoaded {
        path: PathBuf,
        content: String,
    },
    FileError {
        path: PathBuf,
        error: String,
    },
}

pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
}
```

**runtime.rs** - 封装 tokio runtime：

```rust
use std::sync::mpsc::Sender;
use std::path::PathBuf;
use super::message::{AppMessage, DirEntryInfo};

pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
    tx: Sender<AppMessage>,
}

impl AsyncRuntime {
    pub fn new(tx: Sender<AppMessage>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");
        Self { runtime, tx }
    }

    pub fn load_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_dir(&path).await {
                Ok(mut entries) => {
                    let mut result = Vec::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if let Ok(file_type) = entry.file_type().await {
                            if let Some(name) = entry.file_name().to_str() {
                                result.push(DirEntryInfo {
                                    name: name.to_string(),
                                    is_dir: file_type.is_dir(),
                                });
                            }
                        }
                    }
                    let _ = tx.send(AppMessage::DirLoaded { path, entries: result });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::DirLoadError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn load_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let _ = tx.send(AppMessage::FileLoaded { path, content });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FileError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }
}
```

#### Step 3: 修改主循环

**关键改动**：

| 现在 | 重构后 |
|------|--------|
| `event::read()` 阻塞等待 | `event::poll(16ms)` + 超时 |
| 无异步消息处理 | `rx.try_recv()` 检查消息 |
| `Workbench::new(path)` | `Workbench::new(path, runtime)` |

```rust
fn run_app(terminal: &mut Terminal<...>, path: &Path) -> io::Result<()> {
    // 1. 创建 channel 和 runtime
    let (tx, rx) = std::sync::mpsc::channel();
    let async_runtime = AsyncRuntime::new(tx);

    // 2. Workbench 持有 runtime 引用
    let mut workbench = Workbench::new(path, async_runtime)?;

    loop {
        // 3. 渲染
        terminal.draw(|frame| {
            workbench.render(frame, frame.area());
        })?;

        // 4. 用 poll 替代 read，设置 16ms 超时（约 60fps）
        if event::poll(Duration::from_millis(16))? {
            let event = event::read()?;
            let events = drain_pending_events(event);
            for ev in events {
                if matches!(workbench.handle_input(&ev.into()), EventResult::Quit) {
                    return Ok(());
                }
            }
        }

        // 5. 处理异步消息
        while let Ok(msg) = rx.try_recv() {
            workbench.handle_message(msg);
        }
    }
}
```

#### Step 4: 修改 Workbench

```rust
pub struct Workbench {
    // ... 现有字段 ...
    runtime: AsyncRuntime,
}

impl Workbench {
    pub fn new(path: &Path, runtime: AsyncRuntime) -> io::Result<Self> {
        // ...
    }

    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::DirLoaded { path, entries } => {
                // 更新文件树
            }
            AppMessage::DirLoadError { path, error } => {
                // 显示错误
            }
            AppMessage::FileLoaded { path, content } => {
                // 打开编辑器 tab
            }
            AppMessage::FileError { path, error } => {
                // 显示错误
            }
        }
    }

    pub fn runtime(&self) -> &AsyncRuntime {
        &self.runtime
    }
}
```

#### Step 5: 迁移文件树为懒加载

1. FileTree 节点增加 `LoadState` 枚举
2. 初始化只加载根目录一层
3. 展开目录时调用 `runtime.load_dir(path)`
4. `handle_message` 收到 `DirLoaded` 后更新树

#### Step 6: 后续扩展

按需添加更多异步服务：
- FileWatchService（文件监控）
- LspService（语言服务）
- AiService（AI 补全）
- SearchService（全局搜索）

### 5.6 风险与对策

| 风险 | 对策 |
|------|------|
| 异步复杂度增加 | 封装 AsyncRuntime，隐藏细节 |
| 状态同步问题 | 所有状态修改在主线程，通过消息通信 |
| 调试困难 | 添加日志，使用 tokio-console |
| 性能开销 | 只对需要的操作使用异步 |

---

## 6. 验收标准

### Phase 1-5 ✅ 已完成
- [x] 所有现有功能正常工作
- [x] 代码结构符合目标架构
- [x] 通过所有现有测试（70 个）

### Phase 6 完成标准
- [x] Undo/Redo 工作
- [ ] 查找/替换可用
- [x] 复制/粘贴可用

### Phase 7 完成标准
- [ ] 异步架构就绪
- [ ] 大文件加载不卡顿
- [ ] 文件监控可用

### Phase 8 完成标准
- [ ] LSP 集成可用
- [ ] AI 补全可用
- [ ] SSH 远程编辑可用

---

## 7. 更新日志

### 2024-12-27
- 实现复制/粘贴/剪切功能
- 新增 `services/clipboard.rs` - ClipboardService（arboard 封装）
- 支持 Ctrl+C/X/V 快捷键
- 支持 crossterm bracketed paste 事件
- 10MB 粘贴大小限制（TODO: 后续优化大文本处理）
- 通过所有测试（87 个）

### 2024-12-24
- 实现 Undo/Redo 功能（Git 模型）
- 新增 `models/edit_op.rs` - EditOp 原子操作定义
- 新增 `models/edit_history.rs` - Git 风格历史管理
- 集成到 EditorView，支持 Ctrl+Z / Shift+Ctrl+Z / Ctrl+Y
- 通过所有测试（83 个）

### 2024-XX-XX
- 完成 Phase 1-5 架构重构
- 实现双击展开目录/打开文件
- 修复鼠标点击位置偏移问题
- 实现滚轮事件合并优化
- 实现视口滚动跟随控制
- 添加异步架构方案

---

## 8. Undo/Redo 设计（Git 模型）

### 8.1 设计目标

| 目标 | 说明 |
|------|------|
| 原子性 | 文本变更和光标位置同时记录，Undo 后光标正确恢复 |
| 历史不丢失 | Undo 后新编辑创建分支，不覆盖 redo 历史 |
| 高效 | 利用 Rope O(1) clone，checkpoint 避免重放 |
| 可扩展 | 预留 Git 风格 API，支持未来 undo tree 可视化 |

### 8.2 核心数据结构

```
models/
├── edit_op.rs      # OpId + OpKind + EditOp
└── edit_history.rs # EditHistory (Git 模型)
```

#### OpId - 操作唯一标识

```rust
pub struct OpId {
    pub timestamp: u64,  // 毫秒时间戳
    pub counter: u16,    // 同一毫秒内的计数器
}
```

- 避免 UUID 依赖
- `OpId::root()` 表示文件初始状态

#### OpKind - 操作内容

```rust
pub enum OpKind {
    Insert { char_offset: usize, text: String },
    Delete { start: usize, end: usize, deleted: String },
}
```

#### EditOp - 完整操作记录

```rust
pub struct EditOp {
    pub id: OpId,                    // 唯一标识
    pub parent: OpId,                // 父操作（形成 DAG）
    pub kind: OpKind,                // 操作内容
    pub cursor_before: (usize, usize), // 操作前光标
    pub cursor_after: (usize, usize),  // 操作后光标
}
```

#### EditHistory - 历史管理

```rust
pub struct EditHistory {
    base_snapshot: Rope,              // 初始状态
    ops: HashMap<OpId, EditOp>,       // DAG 存储
    head: OpId,                       // 当前位置
    children: HashMap<OpId, Vec<OpId>>, // 子节点索引
    checkpoints: HashMap<OpId, Checkpoint>, // 快照缓存
}
```

### 8.3 数据流

```
编辑操作:
  用户输入 → TextBuffer.insert_char_op() → EditOp
           → EditHistory.push(op) → 更新 DAG + HEAD
           → 写入 .ops 文件（延迟刷盘）

Undo:
  Ctrl+Z → EditHistory.undo()
         → HEAD = parent
         → rebuild_rope_at(parent) → 从 checkpoint 重放
         → TextBuffer.set_rope() + set_cursor()

Redo:
  Shift+Ctrl+Z → EditHistory.redo()
               → HEAD = children.last()
               → rebuild_rope_at(child)
               → TextBuffer.set_rope() + set_cursor()
```

### 8.4 Git 模型 vs 线性模型

```
线性模型（传统编辑器）:
  A → B → C
  Undo 到 A，新编辑 D:
  A → D  (B, C 丢失)

Git 模型（当前实现）:
  A → B → C
  Undo 到 A，新编辑 D:
  A → B → C
   \→ D   (B, C 保留，形成分支)
```

### 8.5 API 设计

#### 基础 API（EditorView 使用）

```rust
impl EditHistory {
    pub fn push(&mut self, op: EditOp, current_rope: &Rope);
    pub fn undo(&mut self) -> Option<(Rope, (usize, usize))>;
    pub fn redo(&mut self) -> Option<(Rope, (usize, usize))>;
    pub fn head(&self) -> OpId;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
    pub fn is_dirty(&self) -> bool;
}
```

#### Git 风格 API（预留）

```rust
impl EditHistory {
    pub fn get_op(&self, id: &OpId) -> Option<&EditOp>;
    pub fn parent(&self, id: &OpId) -> Option<OpId>;
    pub fn children_of(&self, id: &OpId) -> Vec<OpId>;
    pub fn checkout(&mut self, id: OpId) -> Option<(Rope, (usize, usize))>;
    pub fn log(&self) -> Vec<&EditOp>;      // 从 HEAD 回溯
    pub fn reflog(&self) -> impl Iterator<Item = &EditOp>; // 所有操作
    pub fn branch_points(&self) -> Vec<OpId>; // 分叉点
}
```

### 8.6 性能优化

| 优化 | 说明 |
|------|------|
| Checkpoint | 每 100 个操作保存 Rope 快照，避免从头重放 |
| Rope O(1) clone | 快照共享内存，空间开销小 |
| 延迟刷盘 | 批量写入 + 定时刷新，减少 IO |
| 子节点索引 | `children: HashMap<OpId, Vec<OpId>>`，O(1) 查找 |

### 8.7 崩溃恢复

```
.ops 文件格式（JSON Lines）:
  {"id":{"timestamp":123,"counter":0},"parent":...,"kind":...}
  {"id":{"timestamp":124,"counter":0},"parent":...,"kind":...}
  HEAD=123:0001

恢复流程:
  1. 读取原文件 → base_snapshot
  2. 解析 .ops 文件 → 重建 DAG
  3. 找到 HEAD → rebuild_rope_at(HEAD)
  4. 提示用户是否恢复
```

### 8.8 快捷键

| 快捷键 | 功能 |
|--------|------|
| Ctrl+Z | Undo |
| Shift+Ctrl+Z | Redo |
| Ctrl+Y | Redo（备选）|
