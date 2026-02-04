use super::super::Workbench;
use crate::kernel::{BottomPanelTab, SearchResultItem, SearchViewport};
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::{BorderKind, Painter};
use crate::ui::core::style::{Mod, Style as UiStyle};
use unicode_width::UnicodeWidthStr;

impl Workbench {
    pub(super) fn paint_bottom_panel(&mut self, painter: &mut Painter, area: UiRect) {
        let tab = self.store.state().ui.bottom_panel.active_tab.clone();
        if area.is_empty() {
            return;
        }

        let base_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(area, base_style);

        let tabs_h = 1.min(area.h);
        let tabs_area = UiRect::new(area.x, area.y, area.w, tabs_h);
        let content_area = UiRect::new(
            area.x,
            area.y.saturating_add(tabs_h),
            area.w,
            area.h.saturating_sub(tabs_h),
        );

        self.paint_bottom_panel_tabs(painter, tabs_area, &tab);

        match tab {
            BottomPanelTab::Problems => self.paint_bottom_panel_problems(painter, content_area),
            BottomPanelTab::CodeActions => {
                self.paint_bottom_panel_code_actions(painter, content_area)
            }
            BottomPanelTab::Locations => self.paint_bottom_panel_locations(painter, content_area),
            BottomPanelTab::Symbols => self.paint_bottom_panel_symbols(painter, content_area),
            BottomPanelTab::SearchResults => {
                self.paint_bottom_panel_search_results(painter, content_area)
            }
            BottomPanelTab::Logs => self.paint_bottom_panel_logs(painter, content_area),
            BottomPanelTab::Terminal => self.paint_bottom_panel_terminal(painter, content_area),
        }
    }

    fn paint_bottom_panel_tabs(&self, painter: &mut Painter, area: UiRect, active: &BottomPanelTab) {
        if area.is_empty() {
            return;
        }

        let tab_active = UiStyle::default()
            .fg(self.ui_theme.header_fg)
            .add_mod(Mod::BOLD);
        let tab_inactive = UiStyle::default().fg(self.ui_theme.palette_muted_fg);

        let y = area.y;
        let mut x = area.x;
        for (tab, label) in self.bottom_panel_tabs() {
            let style = if &tab == active {
                tab_active
            } else {
                tab_inactive
            };
            painter.text_clipped(Pos::new(x, y), label.as_str(), style, area);
            x = x.saturating_add(label.width().min(u16::MAX as usize) as u16);
            if x >= area.right() {
                break;
            }
        }
    }

    fn paint_bottom_panel_problems(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_problems_view_height(area.h);

        let problems_state = &self.store.state().problems;
        let problems = problems_state.items();
        if problems.is_empty() {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No problems", style, area);
            return;
        }

        let start = problems_state.scroll_offset().min(problems.len());
        let end = (start + height).min(problems.len());
        let selected = problems_state
            .selected_index()
            .min(problems.len().saturating_sub(1));

        for (row, (i, item)) in problems
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let y = area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= area.bottom() {
                break;
            }

            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.range.start_line.saturating_add(1);
            let col = item.range.start_col.saturating_add(1);
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.palette_muted_fg
            });
            let severity_style = match item.severity {
                crate::kernel::problems::ProblemSeverity::Error => {
                    UiStyle::default().fg(self.ui_theme.error_fg)
                }
                crate::kernel::problems::ProblemSeverity::Warning => {
                    UiStyle::default().fg(self.ui_theme.warning_fg)
                }
                crate::kernel::problems::ProblemSeverity::Information => {
                    UiStyle::default().fg(self.ui_theme.palette_muted_fg)
                }
                crate::kernel::problems::ProblemSeverity::Hint => {
                    UiStyle::default().fg(self.ui_theme.palette_muted_fg)
                }
            };

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);

            let file_info = format!("{}:{}:{} ", file_name, line, col);
            let file_style = UiStyle::default().fg(self.ui_theme.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
            x = x.saturating_add(file_info.width().min(u16::MAX as usize) as u16);

            let sev = format!("[{}] ", item.severity.label());
            painter.text_clipped(Pos::new(x, y), sev.as_str(), severity_style, row_clip);
            x = x.saturating_add(sev.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, y), item.message.as_str(), UiStyle::default(), row_clip);
        }
    }

    fn paint_bottom_panel_locations(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_locations_view_height(area.h);

        let locations_state = &self.store.state().locations;
        let locations = locations_state.items();
        if locations.is_empty() {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No locations", style, area);
            return;
        }

        let start = locations_state.scroll_offset().min(locations.len());
        let end = (start + height).min(locations.len());
        let selected = locations_state
            .selected_index()
            .min(locations.len().saturating_sub(1));

        for (row, (i, item)) in locations
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let y = area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= area.bottom() {
                break;
            }
            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.line.saturating_add(1);
            let col = item.column.saturating_add(1);
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.palette_muted_fg
            });

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);

            let file_info = format!("{}:{}:{} ", file_name, line, col);
            let file_style = UiStyle::default().fg(self.ui_theme.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
        }
    }

    fn paint_bottom_panel_code_actions(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_code_actions_view_height(area.h);

        let actions_state = &self.store.state().code_actions;
        let actions = actions_state.items();
        if actions.is_empty() {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No actions", style, area);
            return;
        }

        let start = actions_state.scroll_offset().min(actions.len());
        let end = (start + height).min(actions.len());
        let selected = actions_state
            .selected_index()
            .min(actions.len().saturating_sub(1));

        for (row, (i, action)) in actions
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let y = area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= area.bottom() {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.palette_muted_fg
            });

            let title_style = if action.is_preferred {
                UiStyle::default()
                    .fg(self.ui_theme.accent_fg)
                    .add_mod(Mod::BOLD)
            } else {
                UiStyle::default().fg(self.ui_theme.palette_fg)
            };

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);
            painter.text_clipped(Pos::new(x, y), action.title.as_str(), title_style, row_clip);
        }
    }

    fn paint_bottom_panel_symbols(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_symbols_view_height(area.h);

        let symbols_state = &self.store.state().symbols;
        let symbols = symbols_state.items();
        if symbols.is_empty() {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No symbols", style, area);
            return;
        }

        let start = symbols_state.scroll_offset().min(symbols.len());
        let end = (start + height).min(symbols.len());
        let selected = symbols_state
            .selected_index()
            .min(symbols.len().saturating_sub(1));

        for (row, (i, item)) in symbols
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let y = area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= area.bottom() {
                break;
            }
            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.line.saturating_add(1);
            let col = item.column.saturating_add(1);
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.palette_muted_fg
            });

            let kind = symbol_kind_label(item.kind);
            let indent = "  ".repeat(item.level.min(32));

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;

            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);

            let file_info = format!("{}:{}:{} ", file_name, line, col);
            let file_style = UiStyle::default().fg(self.ui_theme.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
            x = x.saturating_add(file_info.width().min(u16::MAX as usize) as u16);

            let kind_text = format!("[{}] ", kind);
            let kind_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(x, y), kind_text.as_str(), kind_style, row_clip);
            x = x.saturating_add(kind_text.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, y), indent.as_str(), UiStyle::default(), row_clip);
            x = x.saturating_add(indent.width().min(u16::MAX as usize) as u16);

            let name_style = UiStyle::default().fg(self.ui_theme.palette_fg);
            painter.text_clipped(Pos::new(x, y), item.name.as_str(), name_style, row_clip);
            x = x.saturating_add(item.name.as_str().width().min(u16::MAX as usize) as u16);

            if let Some(detail) = item.detail.as_deref().filter(|s| !s.is_empty()) {
                painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
                x = x.saturating_add(1);
                let detail_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
                painter.text_clipped(Pos::new(x, y), detail, detail_style, row_clip);
            }
        }
    }

    fn paint_bottom_panel_logs(&self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        if area.w < 3 || area.h < 3 {
            return;
        }

        let border_style = UiStyle::default().fg(self.ui_theme.focus_border);
        painter.border(area, border_style, BorderKind::Plain);

        let inner = UiRect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.w.saturating_sub(2),
            area.h.saturating_sub(2),
        );
        if inner.is_empty() {
            return;
        }

        if self.logs.is_empty() {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(inner.x, inner.y), "No logs yet", style, inner);
            return;
        }

        let height = inner.h as usize;
        let visible = height.min(self.logs.len());
        let start = self.logs.len().saturating_sub(visible);

        for (row, line) in self.logs.iter().skip(start).enumerate() {
            let y = inner.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= inner.bottom() {
                break;
            }
            let row_clip = UiRect::new(inner.x, y, inner.w, 1);
            painter.text_clipped(Pos::new(inner.x, y), line.as_str(), UiStyle::default(), row_clip);
        }
    }

    fn paint_bottom_panel_search_results(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let summary_h = 1.min(area.h);
        let summary_area = UiRect::new(area.x, area.y, area.w, summary_h);
        let list_area = UiRect::new(
            area.x,
            area.y.saturating_add(summary_h),
            area.w,
            area.h.saturating_sub(summary_h),
        );

        self.sync_search_view_height(SearchViewport::BottomPanel, list_area.h);
        let snapshot = self
            .store
            .state()
            .search
            .snapshot(SearchViewport::BottomPanel);

        let summary = if snapshot.searching {
            format!(
                "Searching... {} files ({} with matches)",
                snapshot.files_searched, snapshot.files_with_matches
            )
        } else if let Some(err) = snapshot.last_error {
            format!("Error: {}", err)
        } else if snapshot.total_matches > 0 {
            format!(
                "{} results in {} files",
                snapshot.total_matches, snapshot.file_count
            )
        } else if !snapshot.search_text.is_empty() {
            "No results".to_string()
        } else {
            "Enter search term in Search sidebar".to_string()
        };

        let summary_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
        painter.text_clipped(
            Pos::new(summary_area.x, summary_area.y),
            summary,
            summary_style,
            summary_area,
        );

        if list_area.is_empty() {
            return;
        }

        if snapshot.items.is_empty() {
            return;
        }

        let search_state = &self.store.state().search;

        let height = list_area.h as usize;
        let start = snapshot.scroll_offset.min(snapshot.items.len());
        let end = (start + height).min(snapshot.items.len());
        let selected = snapshot
            .selected_index
            .min(snapshot.items.len().saturating_sub(1));

        for (row, (i, item)) in snapshot
            .items
            .iter()
            .enumerate()
            .take(end)
            .skip(start)
            .enumerate()
        {
            let y = list_area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= list_area.bottom() {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.palette_muted_fg
            });
            let row_clip = UiRect::new(list_area.x, y, list_area.w, 1);
            let mut x = list_area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);

            match *item {
                SearchResultItem::FileHeader { file_index } => {
                    let Some(file) = search_state.files.get(file_index) else {
                        continue;
                    };
                    let file_name = file
                        .path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.path.to_string_lossy().to_string());
                    let icon = if file.expanded { "▼" } else { "▶" };
                    let match_count = file.matches.len();
                    painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
                    x = x.saturating_add(1);

                    let icon_text = format!("{} ", icon);
                    painter.text_clipped(Pos::new(x, y), icon_text.as_str(), UiStyle::default(), row_clip);
                    x = x.saturating_add(icon_text.width().min(u16::MAX as usize) as u16);

                    let file_style = UiStyle::default().fg(self.ui_theme.accent_fg);
                    painter.text_clipped(Pos::new(x, y), file_name.as_str(), file_style, row_clip);
                    x = x.saturating_add(file_name.width().min(u16::MAX as usize) as u16);

                    let count_text = format!(" ({})", match_count);
                    let count_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
                    painter.text_clipped(
                        Pos::new(x, y),
                        count_text.as_str(),
                        count_style,
                        row_clip,
                    );
                }
                SearchResultItem::MatchLine {
                    file_index,
                    match_index,
                } => {
                    let Some(file) = search_state.files.get(file_index) else {
                        continue;
                    };
                    let Some(match_info) = file.matches.get(match_index) else {
                        continue;
                    };
                    painter.text_clipped(Pos::new(x, y), "  ", UiStyle::default(), row_clip);
                    x = x.saturating_add(2);

                    let line_text = format!("L{}:", match_info.line + 1);
                    let line_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
                    painter.text_clipped(Pos::new(x, y), line_text.as_str(), line_style, row_clip);
                    x = x.saturating_add(line_text.width().min(u16::MAX as usize) as u16);

                    painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
                    x = x.saturating_add(1);

                    let col_text = format!("col {}", match_info.col + 1);
                    let col_style = UiStyle::default().fg(self.ui_theme.header_fg);
                    painter.text_clipped(Pos::new(x, y), col_text.as_str(), col_style, row_clip);
                }
            }
        }
    }
}

fn symbol_kind_label(kind: u32) -> &'static str {
    match kind {
        1 => "file",
        2 => "mod",
        3 => "ns",
        4 => "pkg",
        5 => "class",
        6 => "method",
        7 => "prop",
        8 => "field",
        9 => "ctor",
        10 => "enum",
        11 => "iface",
        12 => "fn",
        13 => "var",
        14 => "const",
        15 => "str",
        16 => "num",
        17 => "bool",
        18 => "array",
        19 => "obj",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "op",
        26 => "type",
        _ => "?",
    }
}
