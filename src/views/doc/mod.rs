use crate::core::text_window;
use crate::kernel::editor::{highlight_snippet, HighlightKind, LanguageId};
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style;
use crate::ui::core::theme::Theme;
use crate::views::editor::markdown::{
    self, MarkdownDocument, MdBlockKind, MdRenderedLine, MdSpanKind,
};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const MAX_RENDER_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderCacheKey {
    pub text_hash: u64,
    pub width: u16,
}

#[derive(Debug, Clone)]
pub struct DocLine {
    pub text: String,
    pub spans: Vec<DocSpan>,
    pub offset_map: Option<Vec<(usize, usize)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocSpanKind {
    Syntax(HighlightKind),
    Markdown(MdSpanKind),
    Selection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocSpan {
    pub start: usize,
    pub end: usize,
    pub kind: DocSpanKind,
}

#[derive(Debug, Clone)]
pub struct RenderCache {
    key: Option<RenderCacheKey>,
    lines: Arc<Vec<DocLine>>,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self {
            key: None,
            lines: Arc::new(Vec::new()),
        }
    }
}

impl RenderCache {
    pub fn key(&self) -> Option<RenderCacheKey> {
        self.key
    }

    pub fn lines(&self) -> &[DocLine] {
        self.lines.as_slice()
    }

    pub fn clear(&mut self) {
        self.key = None;
        self.lines = Arc::new(Vec::new());
    }

    pub fn get_or_render(
        &mut self,
        text: &str,
        width: u16,
        max_lines: usize,
    ) -> (RenderCacheKey, Arc<Vec<DocLine>>, bool) {
        let key = RenderCacheKey {
            text_hash: text_hash(text),
            width,
        };
        if self.key == Some(key) {
            return (key, Arc::clone(&self.lines), true);
        }

        self.lines = Arc::new(render_markdown(text, width, max_lines));
        self.key = Some(key);
        (key, Arc::clone(&self.lines), false)
    }
}

pub fn text_hash(text: &str) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn natural_width(markdown: &str) -> usize {
    natural_width_with_tab_size(markdown, 4)
}

pub fn natural_width_with_tab_size(markdown: &str, tab_size: u8) -> usize {
    markdown
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(|line| line_width_with_tabs(line.trim_end(), tab_size))
        .max()
        .unwrap_or(0)
}

pub fn clamp_scroll_offset(scroll: usize, total_lines: usize, view_height: usize) -> usize {
    if view_height == 0 {
        return 0;
    }
    let max_scroll = total_lines.saturating_sub(view_height);
    scroll.min(max_scroll)
}

pub fn paint_doc_lines(
    painter: &mut Painter,
    area: Rect,
    lines: &[DocLine],
    theme: &Theme,
    base_style: Style,
    scroll_y: usize,
    tab_size: u8,
) {
    if area.is_empty() || lines.is_empty() {
        return;
    }

    let view_h = area.h as usize;
    let start = scroll_y.min(lines.len());
    let end = (start + view_h).min(lines.len());

    for (i, line) in lines[start..end].iter().enumerate() {
        let y = area.y.saturating_add(i.min(u16::MAX as usize) as u16);
        if y >= area.bottom() {
            break;
        }
        let row_clip = Rect::new(area.x, y, area.w, 1);
        paint_doc_line(
            painter,
            Pos::new(area.x, y),
            area.w,
            DocPaintLineParams {
                line,
                theme,
                base_style,
                clip: row_clip,
                horiz_offset: 0,
                tab_size,
            },
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DocPaintLineParams<'a> {
    pub line: &'a DocLine,
    pub theme: &'a Theme,
    pub base_style: Style,
    pub clip: Rect,
    pub horiz_offset: u32,
    pub tab_size: u8,
}

pub fn paint_doc_line(painter: &mut Painter, pos: Pos, width: u16, params: DocPaintLineParams<'_>) {
    let DocPaintLineParams {
        line,
        theme,
        base_style,
        clip,
        horiz_offset,
        tab_size,
    } = params;

    if width == 0 || clip.is_empty() {
        return;
    }

    let right = pos.x.saturating_add(width).min(clip.right());
    if pos.x >= right {
        return;
    }

    let spans = line.spans.as_slice();
    let mut span_idx = 0usize;
    let mut x = pos.x;
    let mut byte_offset = 0usize;
    let mut display_col = 0u32;
    let tab_size = tab_size.max(1) as u32;

    for g in line.text.graphemes(true) {
        let g_start = byte_offset;
        byte_offset = byte_offset.saturating_add(g.len());

        let is_tab = g == "\t";
        let g_width = if is_tab {
            let rem = display_col % tab_size;
            if rem == 0 {
                tab_size
            } else {
                tab_size - rem
            }
        } else {
            g.width() as u32
        };
        if g_width == 0 {
            continue;
        }

        if display_col + g_width <= horiz_offset {
            display_col = display_col.saturating_add(g_width);
            continue;
        }

        if x >= right {
            break;
        }

        let style = style_for_doc_spans_at(spans, &mut span_idx, g_start, theme, base_style);

        let w = g_width.min(u16::MAX as u32) as u16;
        let visible_w = w.min(right.saturating_sub(x));
        if visible_w == 0 {
            break;
        }

        if is_tab {
            painter.fill_rect(Rect::new(x, pos.y, visible_w, 1), style);
            x = x.saturating_add(visible_w);
            display_col = display_col.saturating_add(g_width);
            continue;
        }

        if x.saturating_add(w) > right {
            break;
        }

        painter.text_clipped(Pos::new(x, pos.y), g, style, clip);
        x = x.saturating_add(w);
        display_col = display_col.saturating_add(g_width);
    }
}

fn line_width_with_tabs(line: &str, tab_size: u8) -> usize {
    let tab_size = tab_size.max(1) as usize;
    let mut col = 0usize;
    let mut first = true;
    for seg in line.split('\t') {
        if !first {
            let rem = col % tab_size;
            col = col.saturating_add(if rem == 0 { tab_size } else { tab_size - rem });
        }
        first = false;
        col = col.saturating_add(UnicodeWidthStr::width(seg));
    }
    col
}

fn style_for_doc_spans_at(
    spans: &[DocSpan],
    span_idx: &mut usize,
    offset: usize,
    theme: &Theme,
    base_style: Style,
) -> Style {
    while *span_idx < spans.len() && spans[*span_idx].end <= offset {
        *span_idx += 1;
    }

    let mut style = base_style;
    let mut selected = false;

    let mut probe = *span_idx;
    while let Some(span) = spans.get(probe) {
        if span.start > offset {
            break;
        }
        if offset >= span.start && offset < span.end {
            match span.kind {
                DocSpanKind::Selection => {
                    selected = true;
                    style = Style::default()
                        .bg(theme.palette_selected_bg)
                        .fg(theme.palette_selected_fg);
                }
                DocSpanKind::Syntax(kind) => {
                    if !selected {
                        style =
                            style.patch(Style::default().fg(theme.syntax_fg(kind.color_group())));
                    }
                }
                DocSpanKind::Markdown(kind) => {
                    if !selected {
                        style = style.patch(style_for_markdown_span(kind, theme));
                    }
                }
            }
        }
        probe += 1;
    }

    style
}

fn style_for_markdown_span(kind: MdSpanKind, theme: &Theme) -> Style {
    match kind {
        MdSpanKind::Heading(level) => {
            let fg = match level {
                1 => theme.md_heading1_fg,
                2 => theme.md_heading2_fg,
                3 => theme.md_heading3_fg,
                4 => theme.md_heading4_fg,
                5 => theme.md_heading5_fg,
                _ => theme.md_heading6_fg,
            };
            Style::default()
                .fg(fg)
                .add_mod(crate::ui::core::style::Mod::BOLD)
        }
        MdSpanKind::Link => Style::default().fg(theme.md_link_fg),
        MdSpanKind::Code => Style::default()
            .fg(theme.md_code_fg)
            .add_mod(crate::ui::core::style::Mod::BOLD),
        MdSpanKind::Bold => Style::default().add_mod(crate::ui::core::style::Mod::BOLD),
        MdSpanKind::Italic => Style::default().add_mod(crate::ui::core::style::Mod::ITALIC),
        MdSpanKind::Strike => Style::default().add_mod(crate::ui::core::style::Mod::DIM),
        MdSpanKind::Marker => Style::default()
            .fg(theme.md_marker_fg)
            .add_mod(crate::ui::core::style::Mod::DIM),
        MdSpanKind::BlockquoteText => Style::default().fg(theme.md_blockquote_fg),
        MdSpanKind::BlockquoteBar => Style::default().fg(theme.md_blockquote_bar),
        MdSpanKind::HorizontalRule => Style::default().fg(theme.md_hr_fg),
    }
}

pub fn from_markdown_rendered(rendered: MdRenderedLine) -> DocLine {
    let spans = rendered
        .spans
        .into_iter()
        .map(|span| DocSpan {
            start: span.start,
            end: span.end,
            kind: DocSpanKind::Markdown(span.kind),
        })
        .collect::<Vec<_>>();

    DocLine {
        text: rendered.text,
        spans,
        offset_map: Some(rendered.offset_map),
    }
}

pub fn render_markdown(markdown: &str, width: u16, max_lines: usize) -> Vec<DocLine> {
    if width == 0 || max_lines == 0 || markdown.is_empty() {
        return Vec::new();
    }

    let rope = ropey::Rope::from_str(markdown);
    let md = MarkdownDocument::new(&rope);
    let mut total = rope.len_lines().max(1);
    if markdown.ends_with('\n') {
        total = total.saturating_sub(1);
    }

    let mut out: Vec<DocLine> = Vec::new();
    let mut in_code = false;
    let mut code_lang: Option<LanguageId> = None;
    let mut code_lines: Vec<String> = Vec::new();

    for line_idx in 0..total {
        if out.len() >= max_lines {
            break;
        }

        match md.block_kind(line_idx) {
            MdBlockKind::CodeFence => {
                if in_code {
                    push_code_block_lines(&mut out, code_lang, &code_lines, max_lines);
                    code_lines.clear();
                    in_code = false;
                    code_lang = None;
                } else {
                    in_code = true;
                    code_lang = md
                        .fence_language(line_idx)
                        .and_then(markdown::language_id_for_fence);
                }
            }
            MdBlockKind::CodeBlock if in_code => {
                code_lines.push(rope_line_without_newline(&rope, line_idx));
            }
            _ => {
                if in_code {
                    push_code_block_lines(&mut out, code_lang, &code_lines, max_lines);
                    code_lines.clear();
                    in_code = false;
                    code_lang = None;
                }
                let rendered = md.render_line(line_idx, &rope, width as usize);
                wrap_and_push_rendered_line(&mut out, rendered, width, max_lines);
            }
        }
    }

    if in_code && out.len() < max_lines {
        push_code_block_lines(&mut out, code_lang, &code_lines, max_lines);
    }

    out
}

fn wrap_and_push_rendered_line(
    out: &mut Vec<DocLine>,
    rendered: markdown::MdRenderedLine,
    width: u16,
    max_lines: usize,
) {
    if width == 0 || out.len() >= max_lines {
        return;
    }

    if rendered.text.is_empty() {
        out.push(DocLine {
            text: String::new(),
            spans: Vec::new(),
            offset_map: None,
        });
        return;
    }

    let max_w = width as usize;
    let text_width = rendered.text.as_str().width();

    if text_width <= max_w {
        out.push(from_markdown_rendered(rendered));
        return;
    }

    // Wrap the rendered text while preserving spans.
    let text = &rendered.text;
    let spans = &rendered.spans;
    let mut pos = 0usize;

    while pos < text.len() && out.len() < max_lines {
        let remaining = &text[pos..];
        let end = text_window::truncate_to_width(remaining, max_w);
        if end == 0 {
            break;
        }

        let mut split = pos + end;

        // Prefer breaking at whitespace.
        if split < text.len() {
            if let Some(ws) = text[pos..split].rfind([' ', '\t']) {
                if ws > 0 {
                    split = pos + ws;
                }
            }
        }

        let seg_text = text[pos..split].to_string();
        let seg_spans = spans
            .iter()
            .filter_map(|span| {
                let s = span.start.max(pos);
                let e = span.end.min(split);
                if s < e {
                    Some(DocSpan {
                        start: s - pos,
                        end: e - pos,
                        kind: DocSpanKind::Markdown(span.kind),
                    })
                } else {
                    None
                }
            })
            .collect();

        out.push(DocLine {
            text: seg_text,
            spans: seg_spans,
            offset_map: None,
        });

        pos = split;
        // Skip whitespace at the break point.
        while pos < text.len() && matches!(text.as_bytes().get(pos), Some(b' ' | b'\t')) {
            pos += 1;
        }
    }
}

fn push_code_block_lines(
    out: &mut Vec<DocLine>,
    language: Option<LanguageId>,
    lines: &[String],
    max_lines: usize,
) {
    if lines.is_empty() || out.len() >= max_lines {
        return;
    }

    let highlights = language.map(|lang| highlight_snippet(lang, &lines.join("\n")));
    for (idx, line) in lines.iter().enumerate() {
        if out.len() >= max_lines {
            break;
        }
        let hl = highlights
            .as_ref()
            .and_then(|h| h.get(idx))
            .filter(|v| !v.is_empty())
            .cloned();
        let spans = hl
            .unwrap_or_default()
            .into_iter()
            .map(|span| DocSpan {
                start: span.start,
                end: span.end,
                kind: DocSpanKind::Syntax(span.kind),
            })
            .collect::<Vec<_>>();
        out.push(DocLine {
            text: line.clone(),
            spans,
            offset_map: None,
        });
    }
}

fn rope_line_without_newline(rope: &ropey::Rope, line: usize) -> String {
    let total = rope.len_lines().max(1);
    if line >= total {
        return String::new();
    }

    let mut s = rope.line(line).to_string();
    while s.ends_with('\n') || s.ends_with('\r') {
        s.pop();
    }
    s
}

#[cfg(test)]
#[path = "../../../tests/unit/views/doc.rs"]
mod tests;
