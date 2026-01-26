# zcode TODO

> 目标：在现有 “多面板 + 搜索 + Problems + 基础 LSP” 骨架上，补齐现代 IDE（偏 Rust/RA 工作流）关键能力，并把触发/缓存/编辑应用链路做成熟。

## 现状（已具备）

- 多文件/多 pane、Explorer、全局搜索、Problems 列表、命令面板
- Explorer：右键菜单（新建文件/文件夹、重命名、删除）
- LSP：`diagnostics`、`completion`、`hover`、`go-to-definition`、`signature help`
  - 现有实现集中在 `src/kernel/services/adapters/lsp.rs`，并通过 `KernelEffect`/`Action` 回流到 Store/UI。

## P0（先做：打通“编辑应用”链路，解锁多个特性）

- [x] **WorkspaceEdit / TextEdit 应用引擎**
  - [x] 支持 `TextEdit[]`、`WorkspaceEdit`（跨文件、多段 edit）
  - [x] 处理 UTF-16 位置换算（LSP Position/Range）
  - [x] 对已打开 buffer：应用 edit 并维护 `edit_version` / undo 历史一致性
  - [x] 对未打开文件：安全落盘（或懒加载后应用）
  - [x] 支持 `resourceOperations`（可选）：create/rename/delete（至少为 RA rename/organize-imports 做准备）
- [x] **LSP capability/feature gate**
  - 解析 `initialize` 返回的 `serverCapabilities`，只在支持时启用对应命令/触发逻辑
  - 记录 `positionEncoding`（RA 多为 UTF-16，但需要机制化）
- [x] **手动触发入口补齐**
  - [x] 增加 `Command`：`LspSignatureHelp`（手动触发）
  - [x] 增加 `Command`：`LspFormat`（手动触发）
  - [x] 后续 rename/references/codeAction/symbols 的命令入口与默认 keymap

## P1（核心 IDE 功能：Rust 向）

- [x] **Rename**
  - UI：输入框（复用/扩展现有 `InputDialog`）
  - LSP：`textDocument/rename` -> `WorkspaceEdit` -> 应用
- [x] **Find References**
  - LSP：`textDocument/references`
  - UI：结果列表（建议复用 BottomPanel：类似 SearchResults/Problems 的“Locations”tab）
  - 跳转：选择条目后定位到文件/行列
- [x] **Code Action / Quick Fix**
  - LSP：`textDocument/codeAction`（基于 selection 或 diagnostic range）
  - UI：actions 列表 + 预览（可选）
  - 执行：优先 `edit`（WorkspaceEdit）；其次 `command`（`workspace/executeCommand`）
- [x] **Format**
  - [x] LSP：`textDocument/formatting`（文档格式化）
  - [x] LSP：`textDocument/rangeFormatting`
  - [x] 触发：手动命令
  - [x] 触发：`formatOnSave` 设置（可选）
  - [x] 应用：TextEdit[] -> buffer 更新（注意 undo/selection/cursor）
- [x] **Document/Workspace Symbols**
  - LSP：`textDocument/documentSymbol`、`workspace/symbol`
  - UI：命令面板/侧边 Outline（先做命令面板筛选更快落地）

## P2（语义能力：提升“像 IDE”的感觉）

- [x] **Semantic Tokens**
  - LSP：`textDocument/semanticTokens/*`
  - 渲染：把 tokens 融入现有高亮/主题；做增量刷新与缓存
  - [x] 输入时更稳定：失败/空响应不清空语义高亮 + 更长 debounce（避免“闪白”）
  - [x] 编辑时只失效受影响 token 的语义 spans，并对单行 edit 做 span 位移补偿（避免“半高亮/闪烁”）
- [x] **Inlay Hints**
  - LSP：`textDocument/inlayHint`
  - 渲染：行内“虚拟文本”布局（与换行/滚动/选择/光标对齐）
- [x] **Folding**
  - LSP：`textDocument/foldingRange`
  - 数据结构：折叠区间维护 + 编辑后失效/重算策略
  - UI：折叠标记、展开/折叠快捷键、foldtext（可选）

## P1/P2（补全体验与策略成熟化）

- [x] **更强的 Completion 数据模型**
  - 支持 `kind`、`documentation`、`insertTextFormat`（snippet）、`additionalTextEdits`（auto-import）
  - 支持 `completionItem/resolve`（懒加载 docs/detail）
- [x] **更成熟的触发/缓存策略**
  - 从 `serverCapabilities.completionProvider.triggerCharacters` 动态驱动触发字符
  - 处理 `CompletionList.isIncomplete`：决定是否继续请求/刷新
  - Completion session：输入过程中复用上一次结果（前端过滤）+ 超时/版本失效策略
- [x] **Completion 插入光标/占位符体验**
  - `()` 结尾的纯文本补全：光标自动落在括号内
  - snippet 补全：自动选中第一个占位符文本（如 `${1:value}`）方便直接覆盖输入

## 工程化与稳定性（贯穿）

- [x] **LSP 进程生命周期更优雅**
  - shutdown/exit 的正常关闭流程；崩溃后的重启策略（backoff）
  - stderr 分级与采样（避免刷屏/影响性能）
- [x] **测试体系补齐**
  - 扩展 `src/bin/zcode_lsp_stub.rs`：覆盖 rename/references/codeAction/format/symbols 等响应
  - 针对 WorkspaceEdit/TextEdit 的确定性单元测试（含 UTF-16 边界、CRLF、emoji 等）
- [x] **Completion 弹窗稳定性**
  - 渲染同步（viewport resize / editor search 消息）不应误关 completion
  - 回归测试：`src/kernel/store.rs` 新增 `completion_does_not_close_on_viewport_resize`
- [x] **BottomPanel Logs 可复制**
  - focus 在 Logs 时，`Copy`（默认 `Ctrl+C`）复制全部 logs 到系统剪贴板
- [ ] **多语言/多 workspace 支持（可选）**
  - 每项目/每语言 server 配置；root marker 识别；同 root 复用连接
  - 为后续插件化/外部配置预留接口
