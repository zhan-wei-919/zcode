use super::super::util::centered_rect;
use super::super::Workbench;
use crate::kernel::{OverlayKind, ProblemSeverity, SearchResultItem};
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::{BorderKind, Painter};
use crate::ui::core::style::{Mod, Style as UiStyle};
use unicode_width::UnicodeWidthStr;

impl Workbench {
    /// 居中 telescope 浮层：唯一活动的 picker。搜索结果 / 诊断 / 引用 / 符号 / code actions
    /// 都由它承载，关掉即消失，不占常驻屏幕空间。
    pub(super) fn paint_overlay(&mut self, painter: &mut Painter, area: UiRect) {
        let Some(kind) = self.store.state().ui.overlay.active else {
            self.frame_layout.overlay_area = None;
            return;
        };

        // 高度取可用区域的 70%，但留出边框与标题。
        let height = (area.h.saturating_mul(70) / 100).max(6).min(area.h);
        let popup = centered_rect(70, height, area);
        if popup.w < 3 || popup.h < 3 {
            self.frame_layout.overlay_area = None;
            return;
        }
        self.frame_layout.overlay_area = Some(popup);

        let base_style = UiStyle::default()
            .bg(self.theme.core.popup_bg)
            .fg(self.theme.core.palette_fg);
        painter.fill_rect(popup, base_style);
        painter.border(
            popup,
            UiStyle::default().fg(self.theme.core.focus_border),
            BorderKind::Plain,
        );

        let inner = UiRect::new(
            popup.x.saturating_add(1),
            popup.y.saturating_add(1),
            popup.w.saturating_sub(2),
            popup.h.saturating_sub(2),
        );
        if inner.is_empty() {
            return;
        }

        // 标题行。
        let title = overlay_title(kind);
        let title_style = UiStyle::default()
            .fg(self.theme.core.header_fg)
            .add_mod(Mod::BOLD);
        painter.text_clipped(Pos::new(inner.x, inner.y), title, title_style, inner);

        let content = UiRect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.w,
            inner.h.saturating_sub(1),
        );
        if content.is_empty() {
            return;
        }

        match kind {
            OverlayKind::Problems => self.paint_overlay_problems(painter, content),
            OverlayKind::CodeActions => self.paint_overlay_code_actions(painter, content),
            OverlayKind::Locations => self.paint_overlay_locations(painter, content),
            OverlayKind::Symbols => self.paint_overlay_symbols(painter, content),
            OverlayKind::Search => self.paint_overlay_search(painter, content),
        }
    }

    fn paint_overlay_problems(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_problems_view_height(area.h);

        let problems_state = &self.store.state().problems;
        let problems = problems_state.items();
        if problems.is_empty() {
            let style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
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
                self.theme.core.focus_border
            } else {
                self.theme.core.palette_muted_fg
            });
            let severity_style = match item.severity {
                ProblemSeverity::Error => UiStyle::default().fg(self.theme.core.error_fg),
                ProblemSeverity::Warning => UiStyle::default().fg(self.theme.core.warning_fg),
                ProblemSeverity::Information => {
                    UiStyle::default().fg(self.theme.core.palette_muted_fg)
                }
                ProblemSeverity::Hint => UiStyle::default().fg(self.theme.core.palette_muted_fg),
            };

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);

            let file_info = format!("{}:{}:{} ", file_name, line, col);
            let file_style = UiStyle::default().fg(self.theme.core.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
            x = x.saturating_add(file_info.width().min(u16::MAX as usize) as u16);

            let sev = format!("[{}] ", item.severity.label());
            painter.text_clipped(Pos::new(x, y), sev.as_str(), severity_style, row_clip);
            x = x.saturating_add(sev.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(
                Pos::new(x, y),
                item.message.as_str(),
                UiStyle::default(),
                row_clip,
            );
        }
    }

    fn paint_overlay_locations(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_locations_view_height(area.h);

        let locations_state = &self.store.state().locations;
        let locations = locations_state.items();
        if locations.is_empty() {
            let style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
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
                self.theme.core.focus_border
            } else {
                self.theme.core.palette_muted_fg
            });

            let row_clip = UiRect::new(area.x, y, area.w, 1);
            let mut x = area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_clip);
            x = x.saturating_add(marker.width().min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
            x = x.saturating_add(1);

            let file_info = format!("{}:{}:{} ", file_name, line, col);
            let file_style = UiStyle::default().fg(self.theme.core.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
        }
    }

    fn paint_overlay_code_actions(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_code_actions_view_height(area.h);

        let actions_state = &self.store.state().code_actions;
        let actions = actions_state.items();
        if actions.is_empty() {
            let style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No actions", style, area);
            return;
        }

        let start = actions_state.scroll_offset().min(actions.len());
        let end = (start + height).min(actions.len());
        let selected = actions_state
            .selected_index()
            .min(actions.len().saturating_sub(1));

        for (row, (i, action)) in actions.iter().enumerate().take(end).skip(start).enumerate() {
            let y = area.y.saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= area.bottom() {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.theme.core.focus_border
            } else {
                self.theme.core.palette_muted_fg
            });

            let title_style = if action.is_preferred {
                UiStyle::default()
                    .fg(self.theme.core.accent_fg)
                    .add_mod(Mod::BOLD)
            } else {
                UiStyle::default().fg(self.theme.core.palette_fg)
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

    fn paint_overlay_symbols(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let height = area.h as usize;
        self.sync_symbols_view_height(area.h);

        let symbols_state = &self.store.state().symbols;
        let symbols = symbols_state.items();
        if symbols.is_empty() {
            let style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "No symbols", style, area);
            return;
        }

        let start = symbols_state.scroll_offset().min(symbols.len());
        let end = (start + height).min(symbols.len());
        let selected = symbols_state
            .selected_index()
            .min(symbols.len().saturating_sub(1));

        for (row, (i, item)) in symbols.iter().enumerate().take(end).skip(start).enumerate() {
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
                self.theme.core.focus_border
            } else {
                self.theme.core.palette_muted_fg
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
            let file_style = UiStyle::default().fg(self.theme.core.accent_fg);
            painter.text_clipped(Pos::new(x, y), file_info.as_str(), file_style, row_clip);
            x = x.saturating_add(file_info.width().min(u16::MAX as usize) as u16);

            let kind_text = format!("[{}] ", kind);
            let kind_style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
            painter.text_clipped(Pos::new(x, y), kind_text.as_str(), kind_style, row_clip);
            x = x.saturating_add(kind_text.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(
                Pos::new(x, y),
                indent.as_str(),
                UiStyle::default(),
                row_clip,
            );
            x = x.saturating_add(indent.width().min(u16::MAX as usize) as u16);

            let name_style = UiStyle::default().fg(self.theme.core.palette_fg);
            painter.text_clipped(Pos::new(x, y), item.name.as_str(), name_style, row_clip);
            x = x.saturating_add(item.name.as_str().width().min(u16::MAX as usize) as u16);

            if let Some(detail) = item.detail.as_deref().filter(|s| !s.is_empty()) {
                painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
                x = x.saturating_add(1);
                let detail_style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
                painter.text_clipped(Pos::new(x, y), detail, detail_style, row_clip);
            }
        }
    }

    fn paint_overlay_search(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        // 顶部 query 行（telescope 风格），下方层级结果。
        let query_h = 1.min(area.h);
        let query_area = UiRect::new(area.x, area.y, area.w, query_h);
        let summary_h = 1.min(area.h.saturating_sub(query_h));
        let summary_area = UiRect::new(area.x, area.y.saturating_add(query_h), area.w, summary_h);
        let list_area = UiRect::new(
            area.x,
            area.y.saturating_add(query_h + summary_h),
            area.w,
            area.h.saturating_sub(query_h + summary_h),
        );

        let prompt = "/ ";
        let query = self.store.state().search.query.clone();
        let prompt_style = UiStyle::default().fg(self.theme.core.accent_fg);
        painter.text_clipped(
            Pos::new(query_area.x, query_area.y),
            prompt,
            prompt_style,
            query_area,
        );
        painter.text_clipped(
            Pos::new(
                query_area.x.saturating_add(prompt.width() as u16),
                query_area.y,
            ),
            query.as_str(),
            UiStyle::default().fg(self.theme.core.palette_fg),
            query_area,
        );

        self.sync_search_view_height(list_area.h);
        let snapshot = self.store.state().search.snapshot();

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
            "Type to search".to_string()
        };

        let summary_style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
        painter.text_clipped(
            Pos::new(summary_area.x, summary_area.y),
            summary,
            summary_style,
            summary_area,
        );

        if list_area.is_empty() || snapshot.items.is_empty() {
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
            let y = list_area
                .y
                .saturating_add(row.min(u16::MAX as usize) as u16);
            if y >= list_area.bottom() {
                break;
            }
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };
            let marker_style = UiStyle::default().fg(if is_selected {
                self.theme.core.focus_border
            } else {
                self.theme.core.palette_muted_fg
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
                    painter.text_clipped(
                        Pos::new(x, y),
                        icon_text.as_str(),
                        UiStyle::default(),
                        row_clip,
                    );
                    x = x.saturating_add(icon_text.width().min(u16::MAX as usize) as u16);

                    let file_style = UiStyle::default().fg(self.theme.core.accent_fg);
                    painter.text_clipped(Pos::new(x, y), file_name.as_str(), file_style, row_clip);
                    x = x.saturating_add(file_name.width().min(u16::MAX as usize) as u16);

                    let count_text = format!(" ({})", match_count);
                    let count_style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
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
                    let line_style = UiStyle::default().fg(self.theme.core.palette_muted_fg);
                    painter.text_clipped(Pos::new(x, y), line_text.as_str(), line_style, row_clip);
                    x = x.saturating_add(line_text.width().min(u16::MAX as usize) as u16);

                    painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_clip);
                    x = x.saturating_add(1);

                    let col_text = format!("col {}", match_info.col + 1);
                    let col_style = UiStyle::default().fg(self.theme.core.header_fg);
                    painter.text_clipped(Pos::new(x, y), col_text.as_str(), col_style, row_clip);
                }
            }
        }
    }
}

fn overlay_title(kind: OverlayKind) -> &'static str {
    match kind {
        OverlayKind::Search => "Search",
        OverlayKind::Problems => "Diagnostics",
        OverlayKind::CodeActions => "Code Actions",
        OverlayKind::Locations => "References",
        OverlayKind::Symbols => "Symbols",
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
