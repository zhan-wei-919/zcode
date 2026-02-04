use super::Workbench;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget, SearchViewport};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use std::time::Instant;

mod bottom_panel;
mod dialogs;
mod editor;
mod git;
mod layout;
mod sidebar;
mod status;
mod terminal;

pub(super) fn render(workbench: &mut Workbench, backend: &mut dyn Backend, area: Rect) {
    layout::render(workbench, backend, area);
}

pub(super) fn cursor_position(workbench: &Workbench) -> Option<(u16, u16)> {
    layout::cursor_position(workbench)
}

impl Workbench {
    fn sync_editor_viewport_size(&mut self, pane: usize, layout: &crate::views::EditorPaneLayout) {
        if pane >= self.last_editor_content_sizes.len() {
            return;
        }

        let width = layout.content_area.w;
        let height = layout.editor_area.h;
        if width == 0 || height == 0 {
            return;
        }

        let prev = self.last_editor_content_sizes[pane];
        let next = (width, height);
        if prev == next {
            return;
        }
        self.last_editor_content_sizes[pane] = next;

        let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::SetViewportSize {
            pane,
            width: width as usize,
            height: height as usize,
        }));

        if self.store.state().ui.focus == FocusTarget::Editor
            && self.store.state().ui.editor_layout.active_pane == pane
        {
            self.pending_inlay_hints_deadline =
                Some(Instant::now() + super::INLAY_HINTS_DEBOUNCE_DELAY);
            self.pending_folding_range_deadline =
                Some(Instant::now() + super::FOLDING_RANGE_DEBOUNCE_DELAY);
        }
    }

    fn sync_explorer_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }

        if self.last_explorer_view_height == Some(height) {
            return;
        }
        self.last_explorer_view_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::ExplorerSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_search_view_height(&mut self, viewport: SearchViewport, height: u16) {
        if height == 0 {
            return;
        }

        let slot = match viewport {
            SearchViewport::Sidebar => &mut self.last_search_sidebar_results_height,
            SearchViewport::BottomPanel => &mut self.last_search_panel_results_height,
        };

        if *slot == Some(height) {
            return;
        }
        *slot = Some(height);

        let _ = self.dispatch_kernel(KernelAction::SearchSetViewHeight {
            viewport,
            height: height as usize,
        });
    }

    fn sync_problems_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_problems_panel_height == Some(height) {
            return;
        }
        self.last_problems_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::ProblemsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_locations_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_locations_panel_height == Some(height) {
            return;
        }
        self.last_locations_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::LocationsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_code_actions_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_code_actions_panel_height == Some(height) {
            return;
        }
        self.last_code_actions_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::CodeActionsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_symbols_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_symbols_panel_height == Some(height) {
            return;
        }
        self.last_symbols_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::SymbolsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_terminal_view_size(&mut self, id: crate::kernel::TerminalId, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }

        let next = Some((width, height));
        if self.last_terminal_panel_size == next {
            return;
        }
        self.last_terminal_panel_size = next;

        let _ = self.dispatch_kernel(KernelAction::TerminalResize {
            id,
            cols: width,
            rows: height,
        });
    }
}
