use super::*;
use crate::core::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::editor::{ReloadCause, ReloadRequest};
use crate::models::NodeId;
use crate::ui::backend::test::TestBackend;
use crate::ui::core::geom::Rect;
use crate::ui::core::id::IdPath;
use crate::ui::core::style::Color;
use crate::ui::core::tree::{NodeKind, SplitDrop};
use crate::views::{compute_editor_pane_layout, hit_test_search_bar, SearchBarHitResult};
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

fn activity_bar_slot_pos(workbench: &Workbench, index: u16) -> Option<(u16, u16)> {
    let area = workbench.layout_cache.activity_bar_area?;
    let slot_h = util::activity_slot_height(area.h).max(1);
    let slot_top = area.y.saturating_add(index.saturating_mul(slot_h));
    if slot_top >= area.bottom() {
        return None;
    }
    let remaining = area.bottom().saturating_sub(slot_top);
    let h = slot_h.min(remaining).max(1);
    let x = area.x.saturating_add(area.w.saturating_sub(1) / 2);
    let y = slot_top.saturating_add(h.saturating_sub(1) / 2);
    Some((x, y))
}

fn bottom_panel_activity_pos(workbench: &Workbench) -> Option<(u16, u16)> {
    let index = util::activity_items()
        .iter()
        .position(|item| *item == util::ActivityItem::Panel)?;
    activity_bar_slot_pos(workbench, index as u16)
}

fn search_bar_button_pos(workbench: &Workbench, button: SearchBarHitResult) -> Option<(u16, u16)> {
    let state = workbench.store.state();
    let area = *workbench.layout_cache.editor_inner_areas.first()?;
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
    let workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
fn test_toggle_bottom_panel() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    assert!(!workbench.bottom_panel_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
}

#[test]
fn test_activity_bar_removes_search_find_replace_buttons() {
    assert_eq!(
        util::activity_items(),
        &[
            util::ActivityItem::Explorer,
            util::ActivityItem::Panel,
            util::ActivityItem::Palette,
            util::ActivityItem::Git,
            util::ActivityItem::Settings,
        ]
    );
}

#[test]
fn test_bottom_panel_activity_click_opens_terminal_when_hidden() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    render_once(&mut workbench, 120, 40);
    let (x, y) = bottom_panel_activity_pos(&workbench).expect("bottom panel activity item");
    assert!(!workbench.bottom_panel_visible());

    let result = workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
    assert_eq!(workbench.focus(), FocusTarget::BottomPanel);
    assert_eq!(
        workbench.store.state().ui.bottom_panel.active_tab,
        BottomPanelTab::Terminal
    );
}

#[test]
fn test_bottom_panel_activity_click_closes_when_visible() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Logs,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    render_once(&mut workbench, 120, 40);
    let (x, y) = bottom_panel_activity_pos(&workbench).expect("bottom panel activity item");
    assert!(workbench.bottom_panel_visible());

    let result = workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), x, y));

    assert!(result.is_consumed());
    assert!(!workbench.bottom_panel_visible());
}

#[test]
fn test_bottom_panel_activity_highlight_follows_visibility() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    render_once(&mut workbench, 120, 40);
    let (x, y) = bottom_panel_activity_pos(&workbench).expect("bottom panel activity item");
    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));
    let _ = workbench.flush_post_render_sync();
    assert_eq!(
        backend.buffer().cell(x, y).expect("activity cell").style.bg,
        Some(workbench.ui_theme.activity_bg)
    );

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Logs,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    render_once(&mut workbench, 120, 40);
    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));
    let _ = workbench.flush_post_render_sync();
    assert_eq!(
        backend.buffer().cell(x, y).expect("activity cell").style.bg,
        Some(workbench.ui_theme.activity_active_bg)
    );
}

#[test]
fn test_drag_tab_to_split_right_creates_two_panes() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let a_path = dir.path().join("a.rs");
    let b_path = dir.path().join("b.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a_path.clone(),
        content: "fn a() {}\n".to_string(),
    }));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: b_path.clone(),
        content: "fn b() {}\n".to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let b_tab_id = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|p| {
            p.tabs
                .iter()
                .find(|t| t.path.as_ref() == Some(&b_path))
                .map(|t| t.id)
        })
        .expect("b tab");

    let tab_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Tab { pane: 0, tab_id } if tab_id == b_tab_id.raw()))
        .expect("tab node");
    let right_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| {
            matches!(
                n.kind,
                NodeKind::EditorSplitDrop {
                    pane: 0,
                    drop: SplitDrop::Right
                }
            )
        })
        .expect("right split drop node");

    let start_x = tab_node.rect.x.saturating_add(1);
    let start_y = tab_node.rect.y;
    let drop_x = right_node.rect.x.saturating_add(1);
    let drop_y = right_node.rect.y.saturating_add(1);

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

    assert_eq!(workbench.store.state().ui.editor_layout.panes, 2);
    assert_eq!(
        workbench.store.state().ui.editor_layout.split_direction,
        crate::kernel::SplitDirection::Vertical
    );

    let pane0_has_a = workbench
        .store
        .state()
        .editor
        .pane(0)
        .is_some_and(|p| p.tabs.iter().any(|t| t.path.as_ref() == Some(&a_path)));
    let pane1_has_b = workbench
        .store
        .state()
        .editor
        .pane(1)
        .is_some_and(|p| p.tabs.iter().any(|t| t.id == b_tab_id));
    assert!(pane0_has_a);
    assert!(pane1_has_b);
}

#[test]
fn test_drag_tab_to_split_down_creates_two_panes() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let a_path = dir.path().join("a.rs");
    let b_path = dir.path().join("b.rs");
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a_path.clone(),
        content: "fn a() {}\n".to_string(),
    }));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: b_path.clone(),
        content: "fn b() {}\n".to_string(),
    }));

    render_once(&mut workbench, 120, 40);

    let b_tab_id = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|p| {
            p.tabs
                .iter()
                .find(|t| t.path.as_ref() == Some(&b_path))
                .map(|t| t.id)
        })
        .expect("b tab");

    let tab_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Tab { pane: 0, tab_id } if tab_id == b_tab_id.raw()))
        .expect("tab node");
    let down_node = workbench
        .ui_tree
        .nodes()
        .iter()
        .find(|n| {
            matches!(
                n.kind,
                NodeKind::EditorSplitDrop {
                    pane: 0,
                    drop: SplitDrop::Down
                }
            )
        })
        .expect("down split drop node");

    let start_x = tab_node.rect.x.saturating_add(1);
    let start_y = tab_node.rect.y;
    // Drop on the left side of the down zone to avoid the overlapping right zone.
    let drop_x = down_node.rect.x.saturating_add(1);
    let drop_y = down_node.rect.y.saturating_add(1);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
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

    assert_eq!(workbench.store.state().ui.editor_layout.panes, 2);
    assert_eq!(
        workbench.store.state().ui.editor_layout.split_direction,
        crate::kernel::SplitDirection::Horizontal
    );

    let pane0_has_a = workbench
        .store
        .state()
        .editor
        .pane(0)
        .is_some_and(|p| p.tabs.iter().any(|t| t.path.as_ref() == Some(&a_path)));
    let pane1_has_b = workbench
        .store
        .state()
        .editor
        .pane(1)
        .is_some_and(|p| p.tabs.iter().any(|t| t.id == b_tab_id));
    assert!(pane0_has_a);
    assert!(pane1_has_b);
}

#[test]
fn test_drag_tab_renders_ghost_label_near_cursor() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
        .layout_cache
        .sidebar_container_area
        .expect("sidebar container");
    let desired = drag_x.saturating_sub(container.x).saturating_add(1);
    let expected = util::clamp_sidebar_width(container.w, desired);

    assert_eq!(workbench.store.state().ui.sidebar_width, Some(expected));

    render_once(&mut workbench, 120, 40);
    assert_eq!(
        workbench.layout_cache.sidebar_area.expect("sidebar area").w,
        expected
    );
}

#[test]
fn test_command_palette_visible_mouse_down_does_not_steal_focus() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::CommandPalette));
    assert!(workbench.store.state().ui.command_palette.visible);
    assert_eq!(
        workbench.store.state().ui.focus,
        FocusTarget::CommandPalette
    );

    let editor_area = *workbench
        .layout_cache
        .editor_inner_areas
        .first()
        .expect("editor area");
    let click_x = editor_area.x.saturating_add(1);
    let click_y = editor_area.y.saturating_add(1);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        click_x,
        click_y,
    ));

    assert!(workbench.store.state().ui.command_palette.visible);
    assert_eq!(
        workbench.store.state().ui.focus,
        FocusTarget::CommandPalette
    );
}

#[test]
fn test_splitter_capture_clears_when_sidebar_hidden_before_mouse_up() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::BottomPanel);

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(&root, runtime, None, None).unwrap();

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
fn test_focus_bottom_panel() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
    assert_eq!(workbench.focus(), FocusTarget::BottomPanel);
}

#[test]
fn test_drag_bottom_panel_splitter_updates_height_ratio() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ToggleBottomPanel));
    render_once(&mut workbench, 120, 40);

    let splitter_id = IdPath::root("workbench")
        .push_str("bottom_panel_splitter")
        .finish();
    let splitter = workbench
        .ui_tree
        .node(splitter_id)
        .expect("bottom panel splitter");

    let start_x = splitter.rect.x.saturating_add(2);
    let start_y = splitter.rect.y;
    let drag_y = start_y.saturating_sub(6);

    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_x,
        start_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        start_x,
        drag_y,
    ));
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        start_x,
        drag_y,
    ));

    assert_ne!(workbench.store.state().ui.bottom_panel.height_ratio, 333);
}

#[test]
fn test_terminal_mouse_scroll_up_moves_into_history() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    render_once(&mut workbench, 120, 40);

    let id = workbench
        .store
        .state()
        .terminal
        .active
        .expect("terminal session");
    let mut bytes = Vec::new();
    for idx in 0..120 {
        bytes.extend_from_slice(format!("line-{idx}\n").as_bytes());
    }
    let _ = workbench.dispatch_kernel(KernelAction::TerminalOutput { id, bytes });

    let panel = workbench
        .layout_cache
        .bottom_panel_area
        .expect("bottom panel area");
    let x = panel.x.saturating_add(2);
    let y = panel.y.saturating_add(2);

    let _ = workbench.handle_input(&mouse(MouseEventKind::ScrollUp, x, y));

    let offset = workbench
        .store
        .state()
        .terminal
        .active_session()
        .expect("terminal session")
        .scroll_offset;
    assert!(offset > 0, "scroll-up should move terminal into history");
}

#[test]
fn test_terminal_pageup_pagedown_scroll_history() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    let id = workbench
        .store
        .state()
        .terminal
        .active
        .expect("terminal session");
    let mut bytes = Vec::new();
    for idx in 0..120 {
        bytes.extend_from_slice(format!("line-{idx}\n").as_bytes());
    }
    let _ = workbench.dispatch_kernel(KernelAction::TerminalOutput { id, bytes });

    let page_up = KeyEvent {
        code: KeyCode::PageUp,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(page_up));

    let mut offset = workbench
        .store
        .state()
        .terminal
        .active_session()
        .expect("terminal session")
        .scroll_offset;
    assert!(offset > 0, "PageUp should scroll terminal history");

    let page_down = KeyEvent {
        code: KeyCode::PageDown,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    for _ in 0..16 {
        let _ = workbench.handle_input(&InputEvent::Key(page_down));
    }

    offset = workbench
        .store
        .state()
        .terminal
        .active_session()
        .expect("terminal session")
        .scroll_offset;
    assert_eq!(offset, 0, "PageDown should return to terminal bottom");
}

#[test]
fn test_terminal_drag_selection_highlights_cells() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    render_once(&mut workbench, 120, 40);

    let id = workbench
        .store
        .state()
        .terminal
        .active
        .expect("terminal session");
    let _ = workbench.dispatch_kernel(KernelAction::TerminalOutput {
        id,
        bytes: b"select-this\n".to_vec(),
    });

    render_once(&mut workbench, 120, 40);
    let panel = workbench
        .layout_cache
        .bottom_panel_area
        .expect("bottom panel area");
    let y = panel.y.saturating_add(1);
    let x0 = panel.x;
    let x1 = panel.x.saturating_add(5);

    let _ = workbench.handle_input(&mouse(MouseEventKind::Down(MouseButton::Left), x0, y));
    let _ = workbench.handle_input(&mouse(MouseEventKind::Drag(MouseButton::Left), x1, y));
    let _ = workbench.handle_input(&mouse(MouseEventKind::Up(MouseButton::Left), x1, y));

    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));

    let cell = backend
        .buffer()
        .cell(panel.x.saturating_add(2), y)
        .expect("selection cell");
    assert_eq!(cell.style.bg, Some(workbench.ui_theme.palette_selected_bg));
}

#[test]
fn test_terminal_renders_ansi_colors_from_vt100_cells() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));

    render_once(&mut workbench, 120, 40);

    let id = workbench
        .store
        .state()
        .terminal
        .active
        .expect("terminal session");
    let bytes = b"\x1b[32mzhanwei@zhanwei\x1b[0m:\x1b[34m~/project/zcode\x1b[0m$ ".to_vec();
    let _ = workbench.dispatch_kernel(KernelAction::TerminalOutput { id, bytes });

    let mut backend = TestBackend::new(120, 40);
    workbench.render(&mut backend, Rect::new(0, 0, 120, 40));

    let panel = workbench
        .layout_cache
        .bottom_panel_area
        .expect("bottom panel area");
    let y = panel.y.saturating_add(1);

    let username_cell = backend.buffer().cell(panel.x, y).expect("username cell");
    assert_eq!(username_cell.symbol, "z");
    assert_eq!(username_cell.style.fg, Some(Color::Indexed(2)));

    let path_cell = backend
        .buffer()
        .cell(panel.x.saturating_add(16), y)
        .expect("path cell");
    assert_eq!(path_cell.symbol, "~");
    assert_eq!(path_cell.style.fg, Some(Color::Indexed(4)));
}

#[test]
fn test_terminal_selection_text_trims_line_tail_spaces() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });

    let id = workbench
        .store
        .state()
        .terminal
        .active
        .expect("terminal session");
    let _ = workbench.dispatch_kernel(KernelAction::TerminalOutput {
        id,
        bytes: b"abc   \n".to_vec(),
    });

    workbench.terminal_selection = Some(super::TerminalSelection {
        anchor: super::TerminalCellPos { row: 0, col: 0 },
        cursor: super::TerminalCellPos { row: 0, col: 5 },
    });

    let selected = workbench
        .terminal_selection_text()
        .expect("terminal selection text");
    assert_eq!(selected, "abc");
}

#[test]
fn test_open_file_and_save_runs_async_runtime() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
fn test_theme_editor_sync_hsl_supports_indexed_colors() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    workbench.theme.syntax_comment_fg = Color::Indexed(65);
    assert_eq!(
        workbench.store.state().ui.theme_editor.selected_token,
        crate::kernel::state::ThemeEditorToken::Comment
    );
    assert_eq!(
        crate::ui::core::theme_adapter::color_to_rgb(workbench.theme.syntax_comment_fg),
        crate::ui::core::theme_adapter::color_to_rgb(Color::Indexed(65))
    );

    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: 1 });
    assert_eq!(workbench.store.state().ui.theme_editor.hue, 1);
    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
        saturation: 1,
        lightness: 1,
    });
    assert_eq!(
        (
            workbench.store.state().ui.theme_editor.hue,
            workbench.store.state().ui.theme_editor.saturation,
            workbench.store.state().ui.theme_editor.lightness,
        ),
        (1, 1, 1)
    );

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::OpenThemeEditor));

    workbench.sync_theme_editor_hsl();

    let (r, g, b) = crate::ui::core::theme_adapter::color_to_rgb(Color::Indexed(65)).unwrap();
    let expected = crate::app::theme::rgb_to_hsl(r, g, b);
    let state = &workbench.store.state().ui.theme_editor;
    assert_eq!((state.hue, state.saturation, state.lightness), expected);
}

#[test]
fn test_theme_editor_ui_theme_uses_ansi_fallback_when_not_truecolor() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    workbench.terminal_color_support =
        crate::ui::core::color_support::TerminalColorSupport::Ansi256;
    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorSetHue { hue: 120 });
    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorSetSaturationLightness {
        saturation: 100,
        lightness: 50,
    });

    workbench.apply_theme_editor_color();

    assert!(matches!(workbench.theme.syntax_comment_fg, Color::Rgb(..)));
    assert!(matches!(
        workbench.ui_theme.syntax_comment_fg,
        Color::Indexed(_)
    ));
}

#[test]
fn test_theme_editor_apply_color_updates_preview_in_ansi256_mode() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    workbench.terminal_color_support =
        crate::ui::core::color_support::TerminalColorSupport::Ansi256;

    // Pick a concrete ANSI256 color and ensure the preview uses it.
    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorSetAnsiIndex { index: 196 });

    workbench.apply_theme_editor_color();

    assert_eq!(workbench.ui_theme.syntax_comment_fg, Color::Indexed(196));
}

#[test]
fn test_theme_editor_ansi_palette_marker_tracks_mouse_cell() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    workbench.terminal_color_support =
        crate::ui::core::color_support::TerminalColorSupport::Ansi256;
    let _ = workbench.dispatch_kernel(KernelAction::ThemeEditorOpen);

    let mut backend = TestBackend::new(200, 60);
    workbench.render(&mut backend, Rect::new(0, 0, 200, 60));
    let area = workbench
        .theme_editor_layout
        .sv_palette_area
        .expect("sv palette area");
    assert!(area.w > 0 && area.h > 0);

    let click_x = area.x.saturating_add(area.w.saturating_sub(1));
    let click_y = area.y;
    let _ = workbench.handle_input(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        click_x,
        click_y,
    ));

    let mut backend = TestBackend::new(200, 60);
    workbench.render(&mut backend, Rect::new(0, 0, 200, 60));
    let buf = backend.buffer();
    assert_eq!(buf.cell(click_x, click_y).unwrap().symbol, "+");
}

#[test]
fn test_editor_search_runs_async_task_and_updates_matches() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello world\nhello again\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFile));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('x'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.logs
            .iter()
            .any(|line| line.contains("[fs:create_file]") && line.contains("x"))
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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.logs
            .iter()
            .any(|line| line.contains("[fs:write_file]") && line.contains("a.txt"))
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
fn test_file_reloaded_message_does_not_overwrite_dirty_tab_with_duplicate_path() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    let shared = dir.path().join("shared.rs");

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::SplitEditorVertical));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 0,
        path: shared.clone(),
        content: "pane0".to_string(),
    }));
    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
        pane: 1,
        path: shared.clone(),
        content: "pane1".to_string(),
    }));

    let _ = workbench.dispatch_kernel(KernelAction::Editor(EditorAction::InsertText {
        pane: 0,
        text: "_dirty".to_string(),
    }));

    let pane0_before = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.path.as_ref() == Some(&shared))
        })
        .expect("pane0 tab")
        .buffer
        .text();

    workbench.handle_message(AppMessage::FileReloaded {
        request: ReloadRequest {
            pane: 0,
            path: shared.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
        content: "disk-version".to_string(),
    });

    let pane0_after = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|pane| {
            pane.tabs
                .iter()
                .find(|tab| tab.path.as_ref() == Some(&shared))
        })
        .expect("pane0 tab");

    assert!(pane0_after.dirty, "dirty tab should not be reset by reload");
    assert_eq!(
        pane0_after.buffer.text(),
        pane0_before,
        "dirty tab content should not be replaced by disk message"
    );
}

#[test]
fn test_out_of_order_file_reloaded_messages_keep_latest_content() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

    render_once(&mut workbench, 120, 40);

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
    assert_eq!(workbench.store.state().ui.focus, FocusTarget::BottomPanel);

    let editor_area = *workbench
        .layout_cache
        .editor_inner_areas
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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
            .layout_cache
            .editor_inner_areas
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
    let mut workbench = Workbench::new(dir.path(), runtime, None, None).unwrap();

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
            .layout_cache
            .editor_inner_areas
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
