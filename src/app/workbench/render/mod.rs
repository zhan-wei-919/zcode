use super::Workbench;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget, SearchViewport};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use std::time::Duration;
use std::time::Instant;

mod bottom_panel;
mod dialogs;
mod doc;
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
        if pane >= self.viewport_cache.editor_content_sizes.len() {
            return;
        }

        let width = layout.content_area.w;
        let height = layout.editor_area.h;
        if width == 0 || height == 0 {
            return;
        }

        let next = (width, height);
        if self.viewport_cache.editor_content_sizes[pane] == next {
            return;
        }
        self.viewport_cache.editor_content_sizes[pane] = next;
    }

    fn sync_explorer_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }

        if self.viewport_cache.explorer_view_height == Some(height) {
            return;
        }
        self.viewport_cache.explorer_view_height = Some(height);
    }

    fn sync_search_view_height(&mut self, viewport: SearchViewport, height: u16) {
        if height == 0 {
            return;
        }

        let slot = match viewport {
            SearchViewport::Sidebar => &mut self.viewport_cache.search_sidebar_results_height,
            SearchViewport::BottomPanel => &mut self.viewport_cache.search_panel_results_height,
        };

        if *slot == Some(height) {
            return;
        }
        *slot = Some(height);
    }

    fn sync_problems_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.viewport_cache.problems_panel_height == Some(height) {
            return;
        }
        self.viewport_cache.problems_panel_height = Some(height);
    }

    fn sync_locations_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.viewport_cache.locations_panel_height == Some(height) {
            return;
        }
        self.viewport_cache.locations_panel_height = Some(height);
    }

    fn sync_code_actions_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.viewport_cache.code_actions_panel_height == Some(height) {
            return;
        }
        self.viewport_cache.code_actions_panel_height = Some(height);
    }

    fn sync_symbols_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.viewport_cache.symbols_panel_height == Some(height) {
            return;
        }
        self.viewport_cache.symbols_panel_height = Some(height);
    }

    fn sync_terminal_view_size(&mut self, id: crate::kernel::TerminalId, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }

        let next = (width, height);
        if self.viewport_cache.terminal_panel_id == Some(id)
            && self.viewport_cache.terminal_panel_size == Some(next)
        {
            return;
        }

        self.viewport_cache.terminal_panel_id = Some(id);
        self.viewport_cache.terminal_panel_size = Some(next);
    }

    pub fn flush_post_render_sync(&mut self) -> bool {
        let mut changed = false;

        let pane_count = self.viewport_cache.editor_content_sizes.len();
        self.viewport_cache
            .applied_editor_content_sizes
            .resize_with(pane_count, || (0, 0));

        for pane in 0..pane_count {
            let next = self.viewport_cache.editor_content_sizes[pane];
            if next.0 == 0 || next.1 == 0 {
                continue;
            }
            if self.viewport_cache.applied_editor_content_sizes[pane] == next {
                continue;
            }

            self.viewport_cache.applied_editor_content_sizes[pane] = next;
            changed |= self.dispatch_kernel(KernelAction::Editor(EditorAction::SetViewportSize {
                pane,
                width: next.0 as usize,
                height: next.1 as usize,
            }));

            if self.store.state().ui.focus == FocusTarget::Editor
                && self.store.state().ui.editor_layout.active_pane == pane
            {
                let timing = self.store.state().editor.config.lsp_input_timing.clone();
                let inlay_delay = Duration::from_millis(timing.identifier_debounce_ms.inlay_hints);
                let folding_delay =
                    Duration::from_millis(timing.identifier_debounce_ms.folding_range);
                self.lsp_debounce.inlay_hints = Some(Instant::now() + inlay_delay);
                self.lsp_debounce.folding_range = Some(Instant::now() + folding_delay);
            }
        }

        if let Some(height) = self.viewport_cache.explorer_view_height {
            if self.viewport_cache.applied_explorer_view_height != Some(height) {
                self.viewport_cache.applied_explorer_view_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::ExplorerSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.search_sidebar_results_height {
            if self.viewport_cache.applied_search_sidebar_results_height != Some(height) {
                self.viewport_cache.applied_search_sidebar_results_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SearchSetViewHeight {
                    viewport: SearchViewport::Sidebar,
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.search_panel_results_height {
            if self.viewport_cache.applied_search_panel_results_height != Some(height) {
                self.viewport_cache.applied_search_panel_results_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SearchSetViewHeight {
                    viewport: SearchViewport::BottomPanel,
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.problems_panel_height {
            if self.viewport_cache.applied_problems_panel_height != Some(height) {
                self.viewport_cache.applied_problems_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::ProblemsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.locations_panel_height {
            if self.viewport_cache.applied_locations_panel_height != Some(height) {
                self.viewport_cache.applied_locations_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::LocationsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.code_actions_panel_height {
            if self.viewport_cache.applied_code_actions_panel_height != Some(height) {
                self.viewport_cache.applied_code_actions_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::CodeActionsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.viewport_cache.symbols_panel_height {
            if self.viewport_cache.applied_symbols_panel_height != Some(height) {
                self.viewport_cache.applied_symbols_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SymbolsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let (Some(id), Some((width, height))) = (
            self.viewport_cache.terminal_panel_id,
            self.viewport_cache.terminal_panel_size,
        ) {
            if self.viewport_cache.applied_terminal_panel_id != Some(id)
                || self.viewport_cache.applied_terminal_panel_size != Some((width, height))
            {
                self.viewport_cache.applied_terminal_panel_id = Some(id);
                self.viewport_cache.applied_terminal_panel_size = Some((width, height));
                changed |= self.dispatch_kernel(KernelAction::TerminalResize {
                    id,
                    cols: width,
                    rows: height,
                });
            }
        }

        changed
    }
}
