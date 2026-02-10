use super::util;
use super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::FocusTarget;
use crate::ui::core::geom::Pos;
use crate::ui::core::id::IdPath;
use crate::ui::core::tree::Sense;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MouseTarget {
    ContextMenu,
    ThemeEditor,
    CommandPalette,
    SidebarSplitter,
    EditorSplitter,
    ByFocus,
    Explorer,
    Search,
    Editor,
    BottomPanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusPlan {
    BottomPanel,
    ActivityBar,
    SidebarTabs,
    SidebarArea,
    EditorPane { pane: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MouseRouteReason {
    ContextMenuModal,
    ThemeEditorModal,
    CommandPaletteModal,
    SidebarSplitterHit,
    EditorSplitterHit,
    FocusByArea,
    FocusDispatch,
    NoRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MouseDispatchPlan {
    pub(super) target: MouseTarget,
    pub(super) focus_plan: Option<FocusPlan>,
    pub(super) reason: MouseRouteReason,
}

pub(super) fn mouse_target_from_focus(
    focus: FocusTarget,
    sidebar_tab: crate::kernel::SidebarTab,
) -> MouseTarget {
    match focus {
        FocusTarget::Explorer => {
            if sidebar_tab == crate::kernel::SidebarTab::Search {
                MouseTarget::Search
            } else {
                MouseTarget::Explorer
            }
        }
        FocusTarget::Editor => MouseTarget::Editor,
        FocusTarget::BottomPanel => MouseTarget::BottomPanel,
        FocusTarget::CommandPalette => MouseTarget::CommandPalette,
        FocusTarget::ThemeEditor => MouseTarget::ThemeEditor,
    }
}

impl MouseDispatchPlan {
    pub(super) fn modal(target: MouseTarget, reason: MouseRouteReason) -> Self {
        Self {
            target,
            focus_plan: None,
            reason,
        }
    }

    pub(super) fn with_focus(target: MouseTarget, focus_plan: Option<FocusPlan>) -> Self {
        Self {
            target,
            focus_plan,
            reason: if focus_plan.is_some() {
                MouseRouteReason::FocusByArea
            } else {
                MouseRouteReason::FocusDispatch
            },
        }
    }
}

fn editor_pane_at(workbench: &Workbench, x: u16, y: u16) -> Option<usize> {
    workbench
        .layout_cache
        .editor_areas
        .iter()
        .enumerate()
        .find_map(|(index, area)| util::rect_contains(*area, x, y).then_some(index))
}

fn focus_plan_for_area(workbench: &Workbench, event: &MouseEvent) -> Option<FocusPlan> {
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if workbench
                .layout_cache
                .bottom_panel_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::BottomPanel)
            } else if workbench
                .layout_cache
                .activity_bar_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::ActivityBar)
            } else if workbench
                .layout_cache
                .sidebar_tabs_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::SidebarTabs)
            } else if workbench
                .layout_cache
                .sidebar_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::SidebarArea)
            } else {
                editor_pane_at(workbench, event.column, event.row)
                    .map(|pane| FocusPlan::EditorPane { pane })
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            if workbench
                .layout_cache
                .bottom_panel_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::BottomPanel)
            } else if workbench
                .layout_cache
                .sidebar_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                Some(FocusPlan::SidebarArea)
            } else {
                editor_pane_at(workbench, event.column, event.row)
                    .map(|pane| FocusPlan::EditorPane { pane })
            }
        }
        _ => None,
    }
}

fn split_target(workbench: &Workbench, event: &MouseEvent) -> Option<MouseTarget> {
    let pos = Pos::new(event.column, event.row);
    let hit = workbench
        .ui_tree
        .hit_test_with_sense(pos, Sense::DRAG_SOURCE);

    let sidebar_splitter = IdPath::root("workbench")
        .push_str("sidebar_splitter")
        .finish();
    if hit.is_some_and(|node| node.id == sidebar_splitter)
        || workbench.sidebar_split_dragging
            && workbench.store.state().ui.sidebar_visible
            && workbench.layout_cache.sidebar_container_area.is_some()
    {
        return Some(MouseTarget::SidebarSplitter);
    }

    let editor_splitter = IdPath::root("workbench")
        .push_str("editor_splitter")
        .finish();
    if hit.is_some_and(|node| node.id == editor_splitter) || workbench.editor_split_dragging {
        return Some(MouseTarget::EditorSplitter);
    }

    None
}

pub(super) fn plan_mouse_dispatch(workbench: &Workbench, event: &MouseEvent) -> MouseDispatchPlan {
    if workbench.store.state().ui.context_menu.visible {
        return MouseDispatchPlan::modal(
            MouseTarget::ContextMenu,
            MouseRouteReason::ContextMenuModal,
        );
    }

    if workbench.store.state().ui.theme_editor.visible {
        return MouseDispatchPlan::modal(
            MouseTarget::ThemeEditor,
            MouseRouteReason::ThemeEditorModal,
        );
    }

    if workbench.store.state().ui.command_palette.visible
        && workbench.store.state().ui.focus == FocusTarget::CommandPalette
    {
        return MouseDispatchPlan::modal(
            MouseTarget::CommandPalette,
            MouseRouteReason::CommandPaletteModal,
        );
    }

    if let Some(target) = split_target(workbench, event) {
        let reason = match target {
            MouseTarget::SidebarSplitter => MouseRouteReason::SidebarSplitterHit,
            MouseTarget::EditorSplitter => MouseRouteReason::EditorSplitterHit,
            _ => MouseRouteReason::NoRoute,
        };
        return MouseDispatchPlan {
            target,
            focus_plan: None,
            reason,
        };
    }

    let focus_plan = focus_plan_for_area(workbench, event);
    if focus_plan.is_some() {
        MouseDispatchPlan::with_focus(MouseTarget::ByFocus, focus_plan)
    } else {
        MouseDispatchPlan::with_focus(MouseTarget::ByFocus, None)
    }
}
