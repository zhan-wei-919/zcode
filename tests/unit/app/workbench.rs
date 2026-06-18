use super::*;
use crate::core::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::editor::{ReloadCause, ReloadRequest};
use crate::models::NodeId;
use crate::ui::backend::test::TestBackend;
use crate::ui::core::geom::Rect;
use crate::ui::core::id::IdPath;
use crate::ui::core::tree::NodeKind;
use crate::views::{
    compute_editor_pane_layout, hit_test_search_bar, vertical_scrollbar_metrics, SearchBarHitResult,
};
use std::ffi::{OsStr, OsString};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn create_test_runtime() -> (AsyncRuntime, mpsc::Receiver<AppMessage>) {
    let (tx, rx) = mpsc::channel();
    (AsyncRuntime::new(tx).unwrap(), rx)
}

fn drain_runtime_messages(workbench: &mut Workbench, rx: &mpsc::Receiver<AppMessage>) -> bool {
    let mut changed = false;
    while let Ok(msg) = rx.try_recv() {
        workbench.handle_message(msg);
        changed = true;
    }
    changed
}

fn drive_until(
    workbench: &mut Workbench,
    rx: &mpsc::Receiver<AppMessage>,
    timeout: Duration,
    mut done: impl FnMut(&Workbench) -> bool,
) {
    let start = Instant::now();
    loop {
        drain_runtime_messages(workbench, rx);
        workbench.tick();
        if done(workbench) {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timeout waiting for condition");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 捕获当前线程的 tracing 输出，供测试断言错误确实被记录。
/// FsOpError 等诊断已从内存 buffer 改为写日志（避免静默吞错），
/// 测试相应改为捕获 tracing 而非读 `workbench.logs`。
#[derive(Clone)]
struct CapturedLog(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl CapturedLog {
    fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())))
    }

    fn contains(&self, needle: &str) -> bool {
        String::from_utf8_lossy(&self.0.lock().unwrap()).contains(needle)
    }

    fn subscriber(&self) -> impl tracing::Subscriber {
        tracing_subscriber::fmt()
            .with_writer(self.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .finish()
    }
}

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = CapturedLog;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn render_once(workbench: &mut Workbench, w: u16, h: u16) {
    let mut backend = TestBackend::new(w, h);
    workbench.render(&mut backend, Rect::new(0, 0, w, h));
    let _ = workbench.flush_post_render_sync();
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

fn mouse_with_modifiers(
    kind: MouseEventKind,
    x: u16,
    y: u16,
    modifiers: KeyModifiers,
) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers,
    })
}

fn search_bar_button_pos(workbench: &Workbench, button: SearchBarHitResult) -> Option<(u16, u16)> {
    let state = workbench.store.state();
    let area = *workbench.frame_layout.editor.inner_areas.first()?;
    let pane = state.editor.pane(0)?;
    let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
    let search = layout.search_area?;
    for col in search.x..search.right() {
        if hit_test_search_bar(&layout, &pane.search_bar, col, search.y) == Some(button) {
            return Some((col, search.y));
        }
    }
    None
}

#[test]
fn test_workbench_new() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);
    assert!(workbench.sidebar_visible());
}

#[test]
fn test_file_watcher_tracks_workspace_and_syncs_open_files() {
    let dir = tempdir().unwrap();
    let open_path = dir.path().join("open.rs");
    let close_path = dir.path().join("close.rs");
    std::fs::write(&open_path, "fn open() {}\n").expect("write open path");
    std::fs::write(&close_path, "fn close() {}\n").expect("write close path");

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: open_path.clone(),
        content: "fn open() {}\n".to_string(),
    }));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: close_path.clone(),
        content: "fn close() {}\n".to_string(),
    }));

    let watcher = workbench.file_watcher.as_ref().expect("file watcher");
    assert_eq!(
        watcher.workspace_root(),
        dir.path().canonicalize().expect("canonicalize root")
    );
    assert!(watcher.open_files().contains(&open_path));
    assert!(watcher.open_files().contains(&close_path));

    let close_tab_id = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.path.as_ref() == Some(&close_path))
                .map(|tab| tab.id.raw())
        })
        .expect("close tab id");

    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::CloseTabsById {
        pane: 0,
        tab_ids: vec![close_tab_id],
    }));

    let watcher = workbench.file_watcher.as_ref().expect("file watcher");
    assert!(watcher.open_files().contains(&open_path));
    assert!(!watcher.open_files().contains(&close_path));
    assert!(!watcher
        .open_files()
        .contains(&dir.path().join("never-opened.rs")));

    let created_path = dir.path().join("from-watcher.txt");
    std::fs::write(&created_path, "new").expect("write created path");

    drive_until(&mut workbench, &rx, Duration::from_secs(5), |wb| {
        wb.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|row| row.name.as_os_str() == OsStr::new("from-watcher.txt"))
    });
}

#[test]
fn test_toggle_sidebar() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(workbench.sidebar_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(!workbench.sidebar_visible());
}

#[test]
fn test_ctrl_j_opens_diagnostics_overlay() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(!workbench.overlay_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.overlay_visible());
    assert_eq!(workbench.focus(), FocusTarget::Overlay);
}

#[test]
fn test_problems_overlay_click_uses_problems_scroll_offset() {
    use crate::kernel::{ProblemItem, ProblemRange, ProblemSeverity};

    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    // 注入 8 条诊断（同一文件、行号递增以保证排序后顺序稳定）。
    let path = dir.path().join("diag.rs");
    std::fs::write(&path, "\n".repeat(16)).unwrap();
    let items: Vec<ProblemItem> = (0..8)
        .map(|i| ProblemItem {
            path: path.clone(),
            range: ProblemRange {
                start_line: i,
                start_col: 0,
                end_line: i,
                end_col: 1,
            },
            severity: ProblemSeverity::Error,
            message: format!("diag {i}"),
            source: None,
        })
        .collect();
    let _ = workbench.dispatch_kernel(KernelAction::LspDiagnostics {
        path: path.clone(),
        items,
    });
    assert_eq!(workbench.store.state().problems.items().len(), 8);

    // 打开诊断浮层，焦点转到 Overlay。
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::OpenDiagnostics));
    assert_eq!(workbench.focus(), FocusTarget::Overlay);

    // 渲染一次以填充 frame_layout.overlay_area。
    render_once(&mut workbench, 80, 24);

    // 定死视口高度让滚动确定（max_scroll = 8 - 3 = 5）。
    let _ = workbench.dispatch_kernel(KernelAction::ProblemsSetViewHeight { height: 3 });

    // 向下滚动 Problems 列表：scroll(3) → problems.scroll_offset == 3，
    // 而 search 浮层的 panel_view.scroll_offset 始终保持 0。
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollDown));
    assert_eq!(workbench.store.state().problems.scroll_offset(), 3);
    assert_eq!(workbench.store.state().search.panel_view.scroll_offset, 0);

    // 点击可见区第一行（visible_row = 0）。Problems 的列表从 popup.y + 2 起
    // （内层去边框 +1，标题 +1，见 overlay.rs 的 list_top 计算）。
    let popup = workbench
        .frame_layout
        .overlay_area
        .expect("overlay area should be laid out after render");
    let list_top = popup.y + 2;
    let click = mouse(
        MouseEventKind::Down(MouseButton::Left),
        popup.x + 1,
        list_top,
    );
    let result = workbench.handle_input(&click);
    assert!(result.is_consumed());

    // 修复前：误用 search 偏移 0 → row = 0 + 0 = 0（click_row(0) 因已选中 0 而 no-op），selected 仍为 0。
    // 修复后：用 problems 偏移 3 → row = 0 + 3 = 3，selected 变为 3。
    assert_eq!(
        workbench.store.state().problems.selected_index(),
        3,
        "点击滚动后浮层的第一可见行，应选中 problems.scroll_offset 对应的诊断项"
    );
}

#[test]
fn test_command_line_opens_and_runs_command() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    // F1 唤起 `:` 命令行（无模态时不能用字面 `:`）。
    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::F(1),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(workbench.store.state().ui.command_line.active);
    assert_eq!(workbench.focus(), FocusTarget::CommandLine);

    // 输入命令名（匹配 "View: Diagnostics"）后回车执行 → 打开诊断浮层。
    for ch in "diagnostics".chars() {
        let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        }));
    }
    assert_eq!(workbench.store.state().ui.command_line.input, "diagnostics");

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));

    assert!(!workbench.store.state().ui.command_line.active);
    assert!(workbench.overlay_visible());
    assert_eq!(workbench.focus(), FocusTarget::Overlay);
}

#[test]
fn test_command_line_escape_closes() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::F(1),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(workbench.store.state().ui.command_line.active);

    let _ = workbench.handle_input(&InputEvent::Key(KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    }));
    assert!(!workbench.store.state().ui.command_line.active);
    assert_eq!(workbench.focus(), FocusTarget::Editor);
}

#[test]
fn test_drag_tab_renders_ghost_label_near_cursor() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("ghost_drag.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let tab_id = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|p| {
            p.tabs
                .iter()
                .find(|t| t.path.as_ref() == Some(&path))
                .map(|t| t.id)
        })
        .expect("tab id");

    let tab_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Tab { pane: 0, tab_id: id } if id == tab_id.raw()))
        .expect("tab node");

    let start_x = tab_node.rect.x.saturating_add(1);
    let start_y = tab_node.rect.y;
    let drag_x = 50u16;
    let drag_y = 20u16;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drag_x,
        drag_y,
    ));

    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));

    let buf = backend.buffer();
    let ghost_x = drag_x.saturating_add(1);
    let ghost_y = drag_y.saturating_add(1);
    assert_eq!(buf.cell(ghost_x, ghost_y).unwrap().symbol, "▏");
    assert_eq!(
        buf.cell(ghost_x.saturating_add(2), ghost_y).unwrap().symbol,
        "g"
    );
}

#[test]
fn test_drag_sidebar_splitter_updates_sidebar_width() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let splitter_id = IdPath::root("workbench")
        .push_str("sidebar_splitter")
        .finish();
    let splitter = workbench
        .ui_tree
        .node(splitter_id)
        .expect("sidebar splitter");
    assert!(splitter.rect.w > 0 && splitter.rect.h > 0);

    let start_x = splitter.rect.x;
    let start_y = splitter.rect.y.saturating_add(1);
    let drag_x = start_x.saturating_add(10);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drag_x,
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drag_x,
        start_y,
    ));

    let container = workbench
        .frame_layout
        .sidebar_container_area
        .expect("sidebar container");
    let desired = drag_x.saturating_sub(container.x).saturating_add(1);
    let expected = util::clamp_sidebar_width(container.w, desired);

    assert_eq!(workbench.store.state().ui.sidebar_width, Some(expected));

    render_once(&mut workbench, 120, 40);
    assert_eq!(
        workbench.frame_layout.sidebar_area.expect("sidebar area").w,
        expected
    );
}

#[test]
fn test_command_line_active_mouse_down_does_not_steal_focus() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::OpenCommandLine));
    assert!(workbench.store.state().ui.command_line.active);
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::CommandLine);

    let editor_area = *workbench
        .frame_layout
        .editor
        .inner_areas
        .first()
        .expect("editor area");
    let click_x = editor_area.x.saturating_add(1);
    let click_y = editor_area.y.saturating_add(1);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        click_x,
        click_y,
    ));

    assert!(workbench.store.state().ui.command_line.active);
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::CommandLine);
}

#[test]
fn test_splitter_capture_clears_when_sidebar_hidden_before_mouse_up() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let splitter_id = IdPath::root("workbench")
        .push_str("sidebar_splitter")
        .finish();
    let splitter = workbench
        .ui_tree
        .node(splitter_id)
        .expect("sidebar splitter");

    let start_x = splitter.rect.x;
    let start_y = splitter.rect.y.saturating_add(1);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(1),
        start_y,
    ));
    assert!(workbench.ui_runtime.capture().is_some());

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ToggleSidebar));
    assert!(!workbench.store.state().ui.sidebar_visible);

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::OpenDiagnostics));
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::Overlay);

    let _ = workbench.handle_input(&mouse(MouseEventKind::Up(MouseButton::Left), 0, 0));

    assert_eq!(workbench.ui_runtime.capture(), None);
}

#[test]
fn test_drag_explorer_file_into_folder_moves_path() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let src_dir = root.join("src");
    let dst_dir = root.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    let from = src_dir.join("a.txt");
    std::fs::write(&from, "hello\n").unwrap();
    let to = dst_dir.join("a.txt");

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    // Expand `src` so its entries become visible and draggable.
    let src_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("src"))
        .expect("src row");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|r| !r.is_dir && r.name.as_os_str() == OsStr::new("a.txt"))
    });

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let dst_drop = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| match n.kind {
            NodeKind::ExplorerFolderDrop { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .is_some_and(|(path, is_dir)| is_dir && path == dst_dir),
            _ => false,
        })
        .expect("dst folder drop node");

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drop_x = dst_drop.rect.x.saturating_add(1);
    let drop_y = dst_drop.rect.y;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drop_x,
        drop_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drop_x,
        drop_y,
    ));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        to.exists()
            && !from.exists()
            && w.store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(file_id))
                .is_some_and(|(path, is_dir)| !is_dir && path == to)
    });
    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .path_and_kind_for(NodeId::from_raw(file_id)),
        Some((to, false))
    );
}

#[test]
fn test_drag_explorer_file_into_folder_conflict_cancel_keeps_original() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let src_dir = root.join("src");
    let dst_dir = root.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    let from = src_dir.join("a.txt");
    let to = dst_dir.join("a.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    // Expand `src` so its entries become visible and draggable.
    let src_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("src"))
        .expect("src row");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|r| !r.is_dir && r.name.as_os_str() == OsStr::new("a.txt"))
    });

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let dst_drop = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| match n.kind {
            NodeKind::ExplorerFolderDrop { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .is_some_and(|(path, is_dir)| is_dir && path == dst_dir),
            _ => false,
        })
        .expect("dst folder drop node");

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drop_x = dst_drop.rect.x.saturating_add(1);
    let drop_y = dst_drop.rect.y;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drop_x,
        drop_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drop_x,
        drop_y,
    ));

    assert!(workbench.store.state().ui.confirm_dialog.visible);
    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "TO");
    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .path_and_kind_for(NodeId::from_raw(file_id)),
        Some((from.clone(), false))
    );

    let _ = workbench.dispatch_kernel(KernelAction::ConfirmDialogCancel);

    assert!(!workbench.store.state().ui.confirm_dialog.visible);
    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "TO");
    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .path_and_kind_for(NodeId::from_raw(file_id)),
        Some((from, false))
    );
}

#[test]
fn test_drag_explorer_file_into_folder_conflict_accept_overwrites() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let src_dir = root.join("src");
    let dst_dir = root.join("dst");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    let from = src_dir.join("a.txt");
    let to = dst_dir.join("a.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    // Expand `src` so its entries become visible and draggable.
    let src_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("src"))
        .expect("src row");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: src_row,
        now: Instant::now(),
    });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|r| !r.is_dir && r.name.as_os_str() == OsStr::new("a.txt"))
    });

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let dst_drop = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| match n.kind {
            NodeKind::ExplorerFolderDrop { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .is_some_and(|(path, is_dir)| is_dir && path == dst_dir),
            _ => false,
        })
        .expect("dst folder drop node");

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drop_x = dst_drop.rect.x.saturating_add(1);
    let drop_y = dst_drop.rect.y;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drop_x,
        drop_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drop_x,
        drop_y,
    ));

    assert!(workbench.store.state().ui.confirm_dialog.visible);
    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "TO");

    let _ = workbench.dispatch_kernel(KernelAction::ConfirmDialogAccept);

    drive_until(&mut workbench, &rx, Duration::from_secs(5), |_w| {
        to.exists() && !from.exists()
    });

    assert_eq!(std::fs::read_to_string(&to).unwrap(), "FROM");

    let dst_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("dst"))
        .expect("dst row");
    let now = Instant::now();
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: dst_row, now });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: dst_row, now });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store.state().explorer.rows.iter().any(|row| {
            !row.is_dir
                && w.store
                    .state()
                    .explorer
                    .path_and_kind_for(row.id)
                    .is_some_and(|(path, is_dir)| !is_dir && path == to)
        })
    });

    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .rows
            .iter()
            .filter_map(|row| workbench.store.state().explorer.path_and_kind_for(row.id))
            .find(|(path, is_dir)| !*is_dir && *path == to),
        Some((to.clone(), false))
    );
}

#[test]
fn test_drag_explorer_file_onto_file_row_moves_into_that_files_parent_dir() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let from = src_dir.join("a.txt");
    std::fs::write(&from, "hello\n").unwrap();
    let root_target = root.join("root.txt");
    std::fs::write(&root_target, "target\n").unwrap();
    let to = root.join("a.txt");

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    // Expand `src` so its entries become visible and draggable.
    let src_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("src"))
        .expect("src row");
    let now = Instant::now();
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: src_row, now });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: src_row, now });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|r| !r.is_dir && r.name.as_os_str() == OsStr::new("a.txt"))
    });

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let root_target_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == root_target)
                .map(|_| node_id),
            _ => None,
        })
        .expect("root target node");
    let root_target_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == root_target_id))
        .expect("root target rect");

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drop_x = root_target_node.rect.x.saturating_add(1);
    let drop_y = root_target_node.rect.y;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drop_x,
        drop_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drop_x,
        drop_y,
    ));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        to.exists()
            && !from.exists()
            && w.store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(file_id))
                .is_some_and(|(path, is_dir)| !is_dir && path == to)
    });
    assert!(root_target.exists(), "drop target file should not be moved");
    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .path_and_kind_for(NodeId::from_raw(file_id)),
        Some((to, false))
    );
}

#[test]
fn test_drag_explorer_file_into_root_empty_space_moves_into_root() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let from = src_dir.join("a.txt");
    std::fs::write(&from, "hello\n").unwrap();
    let to = root.join("a.txt");

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    // Expand `src` so its entries become visible and draggable.
    let src_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.is_dir && r.name.as_os_str() == OsStr::new("src"))
        .expect("src row");
    let now = Instant::now();
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: src_row, now });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow { row: src_row, now });

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|r| !r.is_dir && r.name.as_os_str() == OsStr::new("a.txt"))
    });

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let root_drop_id = IdPath::root("workbench")
        .push_str("explorer_root_drop")
        .finish();
    let root_drop = workbench
        .ui_tree
        .node(root_drop_id)
        .expect("root drop node");
    assert!(root_drop.rect.w > 0 && root_drop.rect.h > 0);

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drop_x = root_drop.rect.x.saturating_add(1);
    let drop_y = root_drop.rect.y;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drop_x,
        drop_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drop_x,
        drop_y,
    ));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        to.exists()
            && !from.exists()
            && w.store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(file_id))
                .is_some_and(|(path, is_dir)| !is_dir && path == to)
    });
    assert_eq!(
        workbench
            .store
            .state()
            .explorer
            .path_and_kind_for(NodeId::from_raw(file_id)),
        Some((to, false))
    );
}

#[test]
fn test_drag_explorer_file_renders_ghost_label_near_cursor() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let from = root.join("a.txt");
    std::fs::write(&from, "hello\n").unwrap();

    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let file_id = workbench
        .ui_tree
        .nodes()
        .iter()
        .find_map(|n| match n.kind {
            NodeKind::ExplorerRow { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(NodeId::from_raw(node_id))
                .filter(|(path, is_dir)| !*is_dir && *path == from)
                .map(|_| node_id),
            _ => None,
        })
        .expect("explorer file node");
    let file_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::ExplorerRow { node_id } if node_id == file_id))
        .expect("file node rect");

    let start_x = file_node.rect.x.saturating_add(1);
    let start_y = file_node.rect.y;
    let drag_x = 50u16;
    let drag_y = 20u16;

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    // Drag start threshold is 2.
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x.saturating_add(2),
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drag_x,
        drag_y,
    ));

    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));

    let buf = backend.buffer();
    let ghost_x = drag_x.saturating_add(1);
    let ghost_y = drag_y.saturating_add(1);
    assert_eq!(buf.cell(ghost_x, ghost_y).unwrap().symbol, "▏");
    assert_eq!(
        buf.cell(ghost_x.saturating_add(2), ghost_y).unwrap().symbol,
        "a"
    );
}

#[test]
fn test_explorer_highlights_active_open_file_when_another_row_is_selected() {
    let dir = tempdir().unwrap();
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let active_path = root.join("active.rs");
    let selected_path = root.join("selected.rs");
    std::fs::write(&active_path, "fn active() {}\n").unwrap();
    std::fs::write(&selected_path, "fn selected() {}\n").unwrap();

    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(&root, runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: active_path.clone(),
        content: "fn active() {}\n".to_string(),
    }));

    let selected_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|row| !row.is_dir && row.name.as_os_str() == OsStr::new("selected.rs"))
        .expect("selected file row");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: selected_row,
        now: Instant::now(),
    });

    render_once(&mut workbench, 120, 40);

    let find_row_rect = |path: &std::path::Path| {
        workbench
            .ui_tree
            .nodes()
            .iter()
            .find_map(|n| match n.kind {
                NodeKind::ExplorerRow { node_id } => workbench
                    .store
                    .state()
                    .explorer
                    .path_and_kind_for(NodeId::from_raw(node_id))
                    .filter(|(row_path, is_dir)| !*is_dir && row_path == path)
                    .map(|_| n.rect),
                _ => None,
            })
            .expect("explorer row rect")
    };

    let active_rect = find_row_rect(&active_path);
    let selected_rect = find_row_rect(&selected_path);

    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));
    let buf = backend.buffer();

    let active_cell = buf
        .cell(active_rect.x, active_rect.y)
        .expect("active row cell");
    assert_eq!(active_cell.style.fg, Some(workbench.theme.core.header_fg));
    assert!(active_cell
        .style
        .mods
        .contains(crate::ui::core::style::Mod::BOLD));

    let selected_cell = buf
        .cell(selected_rect.x, selected_rect.y)
        .expect("selected row cell");
    assert_eq!(
        selected_cell.style.bg,
        Some(workbench.theme.core.palette_selected_bg)
    );
}

#[test]
fn test_open_file_and_save_runs_async_runtime() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    let insert = KeyEvent {
        code: KeyCode::Char('X'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    let save = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(save));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        let Some(tab) = w.store.state().editor.pane(0).and_then(|p| p.active_tab()) else {
            return false;
        };
        !tab.dirty
    });

    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "Xhello\n");
}

#[test]
fn test_editor_search_runs_async_task_and_updates_matches() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello world\nhello again\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    let find = KeyEvent {
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(find));

    for ch in "hello".chars() {
        let ev = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(ev));
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .is_some_and(|p| !p.search_bar.searching && !p.search_bar.matches.is_empty())
    });

    let pane = workbench.store.state().editor.pane(0).unwrap();
    assert!(pane.search_bar.visible);
    assert!(!pane.search_bar.matches.is_empty());
}

#[test]
fn test_editor_search_bar_nav_buttons_click_dispatches_commands() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello world\nhello again\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    let find = KeyEvent {
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(find));
    for ch in "hello".chars() {
        let ev = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(ev));
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store.state().editor.pane(0).is_some_and(|p| {
            p.search_bar.visible && !p.search_bar.searching && p.search_bar.matches.len() >= 2
        })
    });

    render_once(&mut workbench, 120, 40);

    let (prev_x, row) = search_bar_button_pos(&workbench, SearchBarHitResult::PrevMatch)
        .expect("prev button position");
    let (next_x, _) =
        search_bar_button_pos(&workbench, SearchBarHitResult::NextMatch).expect("next button");
    let (close_x, _) =
        search_bar_button_pos(&workbench, SearchBarHitResult::Close).expect("close button");

    assert_eq!(
        workbench
            .store
            .state()
            .editor
            .pane(0)
            .unwrap()
            .search_bar
            .current_match_index,
        Some(0)
    );

    let next_result =
        workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), next_x, row));
    assert!(next_result.is_consumed());
    assert_eq!(
        workbench
            .store
            .state()
            .editor
            .pane(0)
            .unwrap()
            .search_bar
            .current_match_index,
        Some(1)
    );

    let prev_result =
        workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), prev_x, row));
    assert!(prev_result.is_consumed());
    assert_eq!(
        workbench
            .store
            .state()
            .editor
            .pane(0)
            .unwrap()
            .search_bar
            .current_match_index,
        Some(0)
    );

    let close_result = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        close_x,
        row,
    ));
    assert!(close_result.is_consumed());
    assert!(
        !workbench
            .store
            .state()
            .editor
            .pane(0)
            .unwrap()
            .search_bar
            .visible
    );
}

#[test]
fn test_global_search_runs_async_task_and_populates_results() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "needle\n").unwrap();
    std::fs::write(&b, "x needle y\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusSearch));
    for ch in "needle".chars() {
        let ev = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(ev));
    }

    let start = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(start));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let s = &w.store.state().search;
        !s.searching && s.total_matches > 0 && !s.items.is_empty()
    });

    let s = &workbench.store.state().search;
    assert!(s.total_matches >= 2);
    assert!(!s.items.is_empty());
}

#[test]
fn test_explorer_create_file_runs_async_fs_and_updates_tree() {
    let dir = tempdir().unwrap();
    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFile));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('x'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    let path = dir.path().join("x");
    let file_name = OsString::from("x");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        path.is_file()
            && w.store
                .state()
                .explorer
                .rows
                .iter()
                .any(|row| row.name == file_name)
    });

    assert!(path.is_file());
}

#[test]
fn test_explorer_create_dir_then_expand_loads_entries() {
    let dir = tempdir().unwrap();
    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFolder));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('d'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    let dir_path = dir.path().join("d");
    let dir_name = OsString::from("d");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        dir_path.is_dir()
            && w.store
                .state()
                .explorer
                .rows
                .iter()
                .any(|row| row.name == dir_name)
    });

    let child_path = dir_path.join("child.txt");
    std::fs::write(&child_path, "hello\n").unwrap();

    let dir_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|row| row.name.as_os_str() == OsStr::new("d"))
        .expect("directory row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: dir_row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerActivate);

    let child_name = OsString::from("child.txt");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|row| row.name == child_name)
    });

    assert!(child_path.is_file());
}

#[test]
fn test_explorer_expand_dir_load_error_collapses_node() {
    let dir = tempdir().unwrap();
    let gone_path = dir.path().join("gone");
    std::fs::create_dir_all(&gone_path).unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    std::fs::remove_dir_all(&gone_path).unwrap();

    let row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.name.as_os_str() == OsStr::new("gone"))
        .expect("gone dir row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerActivate);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        let maybe_row = w
            .store
            .state()
            .explorer
            .rows
            .iter()
            .find(|r| r.name.as_os_str() == OsStr::new("gone"));
        if let Some(row) = maybe_row {
            matches!(row.load_state, crate::models::LoadState::NotLoaded) && !row.is_expanded
        } else {
            // With workspace-level watcher enabled, deleted directories may be removed from tree
            // before load error handling finishes.
            true
        }
    });
}

#[test]
fn test_explorer_delete_file_runs_async_fs_and_updates_tree() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("to_delete.txt");
    std::fs::write(&file_path, "x\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.name.as_os_str() == OsStr::new("to_delete.txt"))
        .expect("file row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row,
        now: Instant::now(),
    });

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerDelete));
    let _ = workbench.dispatch_kernel(KernelAction::ConfirmDialogAccept);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        !file_path.exists()
            && !w
                .store
                .state()
                .explorer
                .rows
                .iter()
                .any(|r| r.name.as_os_str() == OsStr::new("to_delete.txt"))
    });

    assert!(!file_path.exists());
}

#[test]
fn test_explorer_create_file_error_is_logged() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("x");
    std::fs::write(&file_path, "exists\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let log = CapturedLog::new();
    tracing::subscriber::with_default(log.subscriber(), || {
        let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFile));
        let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('x'));
        let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

        drive_until(&mut workbench, &rx, Duration::from_secs(2), |_w| {
            log.contains("create_file")
        });
    });

    let count = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .filter(|r| r.name.as_os_str() == OsStr::new("x"))
        .count();
    assert_eq!(count, 1);
}

#[test]
fn test_save_failure_is_logged_and_does_not_clear_dirty() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("sub");
    std::fs::create_dir_all(&subdir).unwrap();
    let file_path = subdir.join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    std::fs::remove_dir_all(&subdir).unwrap();

    let insert = KeyEvent {
        code: KeyCode::Char('X'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    let save = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(save));

    let log = CapturedLog::new();
    tracing::subscriber::with_default(log.subscriber(), || {
        drive_until(&mut workbench, &rx, Duration::from_secs(2), |_w| {
            log.contains("write_file")
        });
    });

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|p| p.active_tab())
        .expect("tab exists");
    assert!(tab.dirty);
}

#[test]
fn test_out_of_order_file_reloaded_messages_keep_latest_content() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("race.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "base".to_string(),
    }));

    workbench.handle_message(AppMessage::FileReloaded {
        request: ReloadRequest {
            pane: 0,
            path: path.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 2,
        },
        content: "newer".to_string(),
    });
    workbench.handle_message(AppMessage::FileReloaded {
        request: ReloadRequest {
            pane: 0,
            path: path.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
        content: "older".to_string(),
    });

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.path.as_ref() == Some(&path))
        })
        .expect("tab exists");

    assert_eq!(
        tab.buffer.text(),
        "newer",
        "stale reload result should not overwrite newer disk content"
    );
}

#[test]
fn test_context_menu_visible_mouse_events_do_not_steal_focus() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::OpenDiagnostics));
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::Overlay);

    let editor_area = *workbench
        .frame_layout
        .editor
        .inner_areas
        .first()
        .expect("editor area");
    let menu_x = editor_area.x.saturating_add(1);
    let menu_y = editor_area.y.saturating_add(1);

    let _ = workbench.dispatch_kernel(KernelAction::ContextMenuOpen {
        request: crate::kernel::state::ContextMenuRequest::EditorArea { pane: 0 },
        x: menu_x,
        y: menu_y,
    });
    assert!(workbench.store.state().ui.context_menu.visible);

    let focus_with_menu = workbench.store.state().ui.focus;

    let click_x = editor_area
        .x
        .saturating_add(editor_area.w.saturating_sub(1));
    let click_y = editor_area
        .y
        .saturating_add(editor_area.h.saturating_sub(1));

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        click_x,
        click_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        click_x,
        click_y,
    ));

    assert_eq!(workbench.store.state().ui.focus, focus_with_menu);
}

#[test]
fn test_editor_right_click_selects_word_under_cursor_for_context_actions() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("main.rs");
    let line = "let content = content + 1;\n";
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: line.to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let (click_x, click_y) = {
        let state = workbench.store.state();
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let pane = state.editor.pane(0).expect("pane");
        let layout = crate::views::compute_editor_pane_layout(area, pane, &state.editor.config);
        let token_col = line.find("content").expect("content token") as u16;
        (
            layout
                .content_area
                .x
                .saturating_add(token_col)
                .saturating_add(1),
            layout.content_area.y,
        )
    };

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Right),
        click_x,
        click_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Right),
        click_x,
        click_y,
    ));

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("active tab");
    let selection = tab
        .buffer
        .selection()
        .expect("right click should select token under cursor");
    let ((start_row, start_col), (end_row, end_col)) = selection.range();

    assert_eq!((start_row, start_col), (0, 4));
    assert_eq!((end_row, end_col), (0, 11));
    assert!(workbench.store.state().ui.context_menu.visible);
}

#[test]
fn test_editor_right_click_inside_selection_keeps_existing_selection() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("main.rs");
    let line = "fn to_ratatui_style(s: Style) -> RStyle {\n";
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: line.to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let (drag_start_x, drag_end_x, right_click_x, click_y) = {
        let state = workbench.store.state();
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let pane = state.editor.pane(0).expect("pane");
        let layout = crate::views::compute_editor_pane_layout(area, pane, &state.editor.config);

        let start_col = line.find("to_ratatui_style").expect("start token") as u16;
        let end_col = line.find('{').expect("line end token") as u16;
        let right_click_col = line.find("Style").expect("style token") as u16 + 1;

        (
            layout.content_area.x.saturating_add(start_col),
            layout.content_area.x.saturating_add(end_col),
            layout.content_area.x.saturating_add(right_click_col),
            layout.content_area.y,
        )
    };

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        drag_start_x,
        click_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        drag_end_x,
        click_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        drag_end_x,
        click_y,
    ));

    let before_range = {
        let tab = workbench
            .store
            .state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .expect("active tab");
        let selection = tab.buffer.selection().expect("selection after drag");
        assert!(
            !selection.is_empty(),
            "drag should create non-empty selection"
        );
        selection.range()
    };

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Right),
        right_click_x,
        click_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Right),
        right_click_x,
        click_y,
    ));

    let after_range = {
        let tab = workbench
            .store
            .state()
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .expect("active tab");
        tab.buffer
            .selection()
            .expect("selection should remain")
            .range()
    };

    assert_eq!(after_range, before_range);
    assert!(workbench.store.state().ui.context_menu.visible);
}

#[test]
fn test_editor_drag_overflow_keeps_horizontal_follow_on_later_drag() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("wide.txt");
    let long_line = "x".repeat(240);
    let content = (0..80)
        .map(|_| format!("{long_line}\n"))
        .collect::<String>();
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content,
    }));

    render_once(&mut workbench, 120, 40);

    let (start_x, start_y, overflow_drag_y, inside_drag_y, far_right_x) = {
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let state = workbench.store.state();
        let pane = state.editor.pane(0).expect("pane");
        let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
        (
            layout.content_area.x,
            layout.content_area.y,
            layout.content_area.bottom().saturating_add(2),
            layout.content_area.bottom().saturating_sub(1),
            layout.content_area.right().saturating_add(30),
        )
    };

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x,
        overflow_drag_y,
    ));

    let overflow_line_offset = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.viewport.line_offset)
        .expect("active tab");
    assert!(
        overflow_line_offset > 0,
        "overflow drag should scroll viewport down"
    );

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        far_right_x,
        inside_drag_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        far_right_x,
        inside_drag_y,
    ));

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("active tab");
    assert!(
        tab.viewport.horiz_offset > 0,
        "after overflow drag, dragging to far right should still auto-follow horizontally"
    );
}

#[test]
fn test_markdown_checkbox_click_toggles_task_marker() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("todo.md");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "intro\n- [ ] finish refactor\n".to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let (x, y) = {
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let state = workbench.store.state();
        let pane = state.editor.pane(0).expect("pane");
        let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
        (
            layout.content_area.x.saturating_add(2), // "[" in "• [ ] ..."
            layout.content_area.y.saturating_add(1),
        )
    };

    let _ = workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    let _ = workbench.handle_input(&mouse(MouseEventKind::Up(MouseButton::Left), x, y));

    let line_after_click = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .and_then(|tab| tab.buffer.line(1))
        .expect("line 1");
    assert_eq!(
        line_after_click.trim_end_matches('\n'),
        "- [x] finish refactor"
    );
}

#[test]
fn test_shift_scroll_down_moves_editor_horizontally() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("wide.rs");
    let content = format!("{}\n", "x".repeat(240));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content,
    }));

    render_once(&mut workbench, 120, 40);

    let (x, y, line_before) = {
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let state = workbench.store.state();
        let pane = state.editor.pane(0).expect("pane");
        let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
        let line_before = pane.active_tab().expect("active tab").viewport.line_offset;
        (
            layout.content_area.x.saturating_add(1),
            layout.content_area.y,
            line_before,
        )
    };

    let _ = workbench.handle_input(&mouse_with_modifiers(
        MouseEventKind::ScrollDown,
        x,
        y,
        KeyModifiers::SHIFT,
    ));

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("active tab");
    assert!(tab.viewport.horiz_offset > 0);
    assert_eq!(tab.viewport.line_offset, line_before);
}

#[test]
fn test_editor_vertical_scrollbar_shows_only_on_right_edge_hover() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("long.rs");
    let content = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content,
    }));

    render_once(&mut workbench, 120, 40);

    let (scrollbar, hover_y) = {
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let state = workbench.store.state();
        let pane = state.editor.pane(0).expect("pane");
        let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
        (
            layout.v_scrollbar_area.expect("vertical scrollbar area"),
            layout.editor_area.y,
        )
    };

    let scrollbar_glyph_count = |workbench: &mut Workbench, scrollbar: Rect| -> usize {
        let mut backend = TestBackend::new(120, 40);
        workbench.render(&mut backend, Rect::new(0, 0, 120, 40));
        let buf = backend.buffer();

        let mut count = 0usize;
        for y in scrollbar.y..scrollbar.bottom() {
            let symbol = &buf
                .cell(scrollbar.x, y)
                .expect("scrollbar cell should exist")
                .symbol;
            if symbol == "█" || symbol == "│" {
                count += 1;
            }
        }
        count
    };

    let hidden_count = scrollbar_glyph_count(&mut workbench, scrollbar);
    assert_eq!(hidden_count, 0, "scrollbar should be hidden by default");

    let _ = workbench.handle_input(&mouse(MouseEventKind::Moved, scrollbar.x, hover_y));
    let shown_count = scrollbar_glyph_count(&mut workbench, scrollbar);
    assert!(
        shown_count > 0,
        "scrollbar should appear on right-edge hover"
    );

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Moved,
        scrollbar.x.saturating_sub(1),
        hover_y,
    ));
    let hidden_again_count = scrollbar_glyph_count(&mut workbench, scrollbar);
    assert_eq!(
        hidden_again_count, 0,
        "scrollbar should hide after leaving right edge"
    );
}

#[test]
fn test_editor_vertical_scrollbar_drag_updates_line_offset_without_selection() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("long.rs");
    let content = (0..200)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content,
    }));

    render_once(&mut workbench, 120, 40);

    let (thumb_x, down_y, drag_y) = {
        let area = *workbench
            .frame_layout
            .editor
            .inner_areas
            .first()
            .expect("editor area");
        let state = workbench.store.state();
        let pane = state.editor.pane(0).expect("pane");
        let layout = compute_editor_pane_layout(area, pane, &state.editor.config);
        let tab = pane.active_tab().expect("active tab");
        let metrics = vertical_scrollbar_metrics(
            &layout,
            tab.buffer.len_lines().max(1),
            layout.editor_area.h as usize,
            tab.viewport.line_offset,
        )
        .expect("vertical scrollbar metrics");
        (
            metrics.thumb_area.x,
            metrics.thumb_area.y,
            metrics.track_area.bottom().saturating_sub(1),
        )
    };

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        thumb_x,
        down_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        thumb_x,
        drag_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        thumb_x,
        drag_y,
    ));

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("active tab");
    assert!(tab.viewport.line_offset > 0);
    assert!(tab.buffer.selection().is_none());
}

#[test]
fn test_lsp_definition_sets_transient_destination_highlight() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let path = dir.path().join("main.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn target() {}\nfn main() { target(); }\n".to_string(),
    }));

    assert!(workbench.definition_jump_highlight.is_none());
    assert!(workbench.pending_definition_highlight.is_none());

    let _ = workbench.dispatch_kernel(KernelAction::LspDefinition {
        path: path.clone(),
        line: 0,
        column: 3,
    });

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("active tab");
    let (row, _col) = tab.buffer.cursor();

    let highlight = workbench
        .definition_jump_highlight
        .expect("definition jump should arm transient highlight");
    assert_eq!(highlight.pane, 0);
    assert_eq!(highlight.tab_id, tab.id);
    assert_eq!(highlight.row, row);
    assert_eq!(highlight.row, 0);
    assert!(workbench.pending_definition_highlight.is_none());
}

#[test]
fn test_lsp_definition_highlight_is_deferred_until_target_file_opens() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let current_path = dir.path().join("current.rs");
    let target_path = dir.path().join("target.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: current_path,
        content: "fn main() { helper(); }\n".to_string(),
    }));

    let _ = workbench.dispatch_kernel(KernelAction::LspDefinition {
        path: target_path.clone(),
        line: 1,
        column: 0,
    });

    assert!(workbench.definition_jump_highlight.is_none());
    let pending = workbench
        .pending_definition_highlight
        .as_ref()
        .expect("target should be pending before file load");
    assert_eq!(pending.path, target_path);
    assert_eq!(pending.row, 1);

    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: target_path.clone(),
        content: "line0\nline1\n".to_string(),
    }));

    let highlight = workbench
        .definition_jump_highlight
        .expect("highlight should activate once target file opens");
    assert_eq!(highlight.pane, 0);
    assert_eq!(highlight.row, 1);
    assert!(workbench.pending_definition_highlight.is_none());

    let expired = DefinitionJumpHighlight {
        started_at: Instant::now() - DEFINITION_JUMP_HIGHLIGHT_DURATION - Duration::from_millis(1),
        ..highlight
    };
    workbench.definition_jump_highlight = Some(expired);

    assert!(workbench.tick());
    assert!(workbench.definition_jump_highlight.is_none());
}
