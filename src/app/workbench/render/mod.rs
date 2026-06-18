use super::Workbench;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget, SearchViewport};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use std::time::Duration;
use std::time::Instant;

mod command_line;
mod dialogs;
mod editor;
mod git;
mod layout;
mod overlay;
mod sidebar;
mod status;

pub(super) fn render(workbench: &mut Workbench, backend: &mut dyn Backend, area: Rect) {
    layout::render(workbench, backend, area);
}

pub(super) fn cursor_position(workbench: &Workbench) -> Option<(u16, u16)> {
    layout::cursor_position(workbench)
}

impl Workbench {
    fn sync_editor_viewport_size(&mut self, pane: usize, layout: &crate::views::EditorPaneLayout) {
        if pane >= self.render_cache.viewport.editor_content_sizes.len() {
            return;
        }

        let width = layout.content_area.w;
        let height = layout.editor_area.h;
        if width == 0 || height == 0 {
            return;
        }

        let next = (width, height);
        if self.render_cache.viewport.editor_content_sizes[pane] == next {
            return;
        }
        self.render_cache.viewport.editor_content_sizes[pane] = next;
    }

    fn sync_explorer_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }

        if self.render_cache.viewport.explorer_view_height == Some(height) {
            return;
        }
        self.render_cache.viewport.explorer_view_height = Some(height);
    }

    fn sync_search_view_height(&mut self, viewport: SearchViewport, height: u16) {
        if height == 0 {
            return;
        }

        let slot = match viewport {
            SearchViewport::Sidebar => {
                &mut self.render_cache.viewport.search_sidebar_results_height
            }
            SearchViewport::BottomPanel => {
                &mut self.render_cache.viewport.search_panel_results_height
            }
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
        if self.render_cache.viewport.problems_panel_height == Some(height) {
            return;
        }
        self.render_cache.viewport.problems_panel_height = Some(height);
    }

    fn sync_locations_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.render_cache.viewport.locations_panel_height == Some(height) {
            return;
        }
        self.render_cache.viewport.locations_panel_height = Some(height);
    }

    fn sync_code_actions_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.render_cache.viewport.code_actions_panel_height == Some(height) {
            return;
        }
        self.render_cache.viewport.code_actions_panel_height = Some(height);
    }

    fn sync_symbols_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.render_cache.viewport.symbols_panel_height == Some(height) {
            return;
        }
        self.render_cache.viewport.symbols_panel_height = Some(height);
    }

    pub fn flush_post_render_sync(&mut self) -> bool {
        let mut changed = false;

        let pane_count = self.render_cache.viewport.editor_content_sizes.len();
        self.render_cache
            .viewport
            .applied_editor_content_sizes
            .resize_with(pane_count, || (0, 0));

        for pane in 0..pane_count {
            let next = self.render_cache.viewport.editor_content_sizes[pane];
            if next.0 == 0 || next.1 == 0 {
                continue;
            }
            if self.render_cache.viewport.applied_editor_content_sizes[pane] == next {
                continue;
            }

            self.render_cache.viewport.applied_editor_content_sizes[pane] = next;
            changed |= self.dispatch_kernel(KernelAction::Editor(EditorAction::SetViewportSize {
                pane,
                width: next.0 as usize,
                height: next.1 as usize,
            }));

            if self.store.state().ui.focus == FocusTarget::Editor
                && self.store.state().ui.editor_layout.active_pane == pane
            {
                let timing = &self.store.state().editor.config.lsp_input_timing;
                let inlay_delay = Duration::from_millis(timing.identifier_debounce_ms.inlay_hints);
                let folding_delay =
                    Duration::from_millis(timing.identifier_debounce_ms.folding_range);
                self.lsp_sync.debounce.inlay_hints = Some(Instant::now() + inlay_delay);
                self.lsp_sync.debounce.folding_range = Some(Instant::now() + folding_delay);
            }
        }

        if let Some(height) = self.render_cache.viewport.explorer_view_height {
            if self.render_cache.viewport.applied_explorer_view_height != Some(height) {
                self.render_cache.viewport.applied_explorer_view_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::ExplorerSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.search_sidebar_results_height {
            if self
                .render_cache
                .viewport
                .applied_search_sidebar_results_height
                != Some(height)
            {
                self.render_cache
                    .viewport
                    .applied_search_sidebar_results_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SearchSetViewHeight {
                    viewport: SearchViewport::Sidebar,
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.search_panel_results_height {
            if self
                .render_cache
                .viewport
                .applied_search_panel_results_height
                != Some(height)
            {
                self.render_cache
                    .viewport
                    .applied_search_panel_results_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SearchSetViewHeight {
                    viewport: SearchViewport::BottomPanel,
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.problems_panel_height {
            if self.render_cache.viewport.applied_problems_panel_height != Some(height) {
                self.render_cache.viewport.applied_problems_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::ProblemsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.locations_panel_height {
            if self.render_cache.viewport.applied_locations_panel_height != Some(height) {
                self.render_cache.viewport.applied_locations_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::LocationsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.code_actions_panel_height {
            if self.render_cache.viewport.applied_code_actions_panel_height != Some(height) {
                self.render_cache.viewport.applied_code_actions_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::CodeActionsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        if let Some(height) = self.render_cache.viewport.symbols_panel_height {
            if self.render_cache.viewport.applied_symbols_panel_height != Some(height) {
                self.render_cache.viewport.applied_symbols_panel_height = Some(height);
                changed |= self.dispatch_kernel(KernelAction::SymbolsSetViewHeight {
                    height: height as usize,
                });
            }
        }

        changed
    }
}
