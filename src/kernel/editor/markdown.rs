//! Markdown WYSIWYG rendering engine.
//!
//! The buffer always stores raw Markdown. This module produces display text for
//! non-cursor lines (hiding markers, applying formatting) and source-line
//! highlighting for the cursor line.

use crate::models::{EditOp, OpKind};
use unicode_width::UnicodeWidthStr;

/// Block-level classification for each line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdBlockKind {
    Paragraph,
    Heading(u8),
    Table,
    CodeFence,
    CodeBlock,
    BlockQuote,
    UnorderedList,
    OrderedList,
    HorizontalRule,
    Blank,
}

/// A styled span within the display text of a rendered line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdSpanKind {
    Heading(u8),
    Link,
    Code,
    Bold,
    Italic,
    Strike,
    Marker,
    BlockquoteText,
    BlockquoteBar,
    HorizontalRule,
}

/// A semantic style span within the display text of a rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdStyleSpan {
    pub start: usize,
    pub end: usize,
    pub kind: MdSpanKind,
}

/// The rendered output for a single non-cursor line.
#[derive(Debug, Clone)]
pub struct MdRenderedLine {
    pub text: String,
    pub spans: Vec<MdStyleSpan>,
    /// Maps display byte offset → source byte offset for mouse click mapping.
    pub offset_map: Vec<(usize, usize)>,
}

/// Per-file Markdown document state tracking block kinds.
pub struct MarkdownDocument {
    block_kinds: Vec<MdBlockKind>,
    fence_lang: Vec<Option<String>>,
    fence_opening: Vec<bool>,
    table_lines: Vec<Option<TableLineInfo>>,
    table_blocks: Vec<TableBlock>,
    version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableLineInfo {
    block_index: usize,
    row_kind: TableRowKind,
}

#[derive(Debug, Clone)]
struct TableBlock {
    start_line: usize,
    end_line: usize,
    aligns: Vec<TableAlign>,
    col_widths: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableRowKind {
    Header,
    Separator,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
struct TableCell {
    display_text: String,
    source_start: usize,
    source_end: usize,
}

impl MarkdownDocument {
    pub fn new(rope: &ropey::Rope) -> Self {
        let (block_kinds, fence_lang, fence_opening, table_lines, table_blocks) =
            classify_blocks(rope);
        Self {
            block_kinds,
            fence_lang,
            fence_opening,
            table_lines,
            table_blocks,
            version: 0,
        }
    }

    pub fn reparse(&mut self, rope: &ropey::Rope, version: u64) {
        let (block_kinds, fence_lang, fence_opening, table_lines, table_blocks) =
            classify_blocks(rope);
        self.block_kinds = block_kinds;
        self.fence_lang = fence_lang;
        self.fence_opening = fence_opening;
        self.table_lines = table_lines;
        self.table_blocks = table_blocks;
        self.version = version;
    }

    pub fn apply_edit(&mut self, op: &EditOp, rope: &ropey::Rope, version: u64) {
        let old_len = self.block_kinds.len();
        let new_len = rope.len_lines().max(1);
        if old_len != new_len {
            self.reparse(rope, version);
            return;
        }

        let (start_char, inserted, deleted) = match &op.kind {
            OpKind::Insert { char_offset, text } => (*char_offset, text.as_str(), ""),
            OpKind::Delete { start, deleted, .. } => (*start, "", deleted.as_str()),
            OpKind::Replace {
                start,
                inserted,
                deleted,
                ..
            } => (*start, inserted.as_str(), deleted.as_str()),
        };

        let inserted_lines = inserted.matches('\n').count();
        let deleted_lines = deleted.matches('\n').count();
        if inserted_lines != deleted_lines {
            self.reparse(rope, version);
            return;
        }

        if inserted.len().max(deleted.len()) > 8 * 1024 || inserted_lines > 64 {
            self.reparse(rope, version);
            return;
        }

        let start_char = start_char.min(rope.len_chars());
        let start_line = rope.char_to_line(start_char);
        let mut line_start = start_line.saturating_sub(2);
        let mut line_end = (start_line + inserted_lines + 3).min(new_len);

        while line_start > 0 && is_structural_kind(self.block_kind(line_start - 1)) {
            line_start -= 1;
        }
        while line_end < old_len && is_structural_kind(self.block_kind(line_end)) {
            line_end += 1;
        }

        // Edits touching fence/table regions or introducing fence/table markers
        // can ripple context far away; keep incremental path strict and safe.
        if has_structural_markers_around(rope, line_start, line_end) {
            self.reparse(rope, version);
            return;
        }

        for line in line_start..line_end {
            let src = rope_line_without_newline(rope, line);
            let trimmed = src.trim_start();
            self.block_kinds[line] = classify_line(trimmed, &src);
            self.fence_lang[line] = None;
            self.fence_opening[line] = false;
            self.table_lines[line] = None;
        }
        self.table_blocks
            .retain(|b| b.end_line <= line_start || b.start_line >= line_end);
        self.version = version;
    }

    pub fn block_kind(&self, line: usize) -> MdBlockKind {
        self.block_kinds
            .get(line)
            .copied()
            .unwrap_or(MdBlockKind::Paragraph)
    }

    /// Render a non-cursor line into display text + style spans + offset map.
    pub fn render_line(
        &self,
        line: usize,
        rope: &ropey::Rope,
        viewport_width: usize,
    ) -> MdRenderedLine {
        let kind = self.block_kind(line);
        let src = rope_line_without_newline(rope, line);

        match kind {
            MdBlockKind::Heading(level) => render_heading(&src, level),
            MdBlockKind::UnorderedList => render_unordered_list(&src),
            MdBlockKind::OrderedList => render_ordered_list(&src),
            MdBlockKind::BlockQuote => render_blockquote(&src),
            MdBlockKind::HorizontalRule => render_hr(viewport_width),
            MdBlockKind::Table => render_table_line(self, line, rope),
            MdBlockKind::CodeFence => {
                let opening = self.fence_opening.get(line).copied().unwrap_or(true);
                let lang = self.fence_language(line);
                render_code_fence(&src, opening, lang, viewport_width)
            }
            MdBlockKind::CodeBlock => render_code_block(&src),
            MdBlockKind::Blank => MdRenderedLine {
                text: String::new(),
                spans: Vec::new(),
                offset_map: Vec::new(),
            },
            MdBlockKind::Paragraph => render_paragraph(&src),
        }
    }

    /// Produce highlight spans for the cursor line (raw source with dimmed markers).
    pub fn highlight_source_line(
        &self,
        line: usize,
        rope: &ropey::Rope,
    ) -> Vec<super::HighlightSpan> {
        let kind = self.block_kind(line);
        let src = rope_line_without_newline(rope, line);
        highlight_source(&src, kind)
    }

    /// Get the fence language for a code block line (looks up the enclosing fence).
    pub fn fence_language(&self, line: usize) -> Option<&str> {
        self.fence_lang.get(line).and_then(|l| l.as_deref())
    }
}

// ---------------------------------------------------------------------------
// Block-level classifier
// ---------------------------------------------------------------------------

fn classify_blocks(
    rope: &ropey::Rope,
) -> (
    Vec<MdBlockKind>,
    Vec<Option<String>>,
    Vec<bool>,
    Vec<Option<TableLineInfo>>,
    Vec<TableBlock>,
) {
    #[derive(Clone)]
    struct FenceState {
        marker: u8,
        len: usize,
        lang: Option<String>,
    }

    let total = rope.len_lines().max(1);
    let mut kinds = Vec::with_capacity(total);
    let mut fence_langs: Vec<Option<String>> = Vec::with_capacity(total);
    let mut fence_opening = Vec::with_capacity(total);
    let mut table_lines: Vec<Option<TableLineInfo>> = Vec::with_capacity(total);
    let mut table_blocks: Vec<TableBlock> = Vec::new();
    let mut fence_state: Option<FenceState> = None;
    let mut i = 0usize;

    while i < total {
        let line = rope_line_without_newline(rope, i);
        let trimmed = line.trim_start();

        if let Some((marker, run_len, rest)) = parse_fence_marker(trimmed) {
            if let Some(state) = fence_state.as_ref() {
                // A closing fence must match delimiter and be at least as long.
                if marker == state.marker && run_len >= state.len && rest.trim().is_empty() {
                    kinds.push(MdBlockKind::CodeFence);
                    fence_langs.push(state.lang.clone());
                    fence_opening.push(false);
                    table_lines.push(None);
                    fence_state = None;
                    i += 1;
                    continue;
                }
            } else {
                let lang = rest
                    .trim()
                    .split_whitespace()
                    .next()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                kinds.push(MdBlockKind::CodeFence);
                fence_langs.push(lang.clone());
                fence_opening.push(true);
                table_lines.push(None);
                fence_state = Some(FenceState {
                    marker,
                    len: run_len,
                    lang,
                });
                i += 1;
                continue;
            }
        }

        if let Some(state) = fence_state.as_ref() {
            kinds.push(MdBlockKind::CodeBlock);
            fence_langs.push(state.lang.clone());
            fence_opening.push(false);
            table_lines.push(None);
            i += 1;
            continue;
        }

        if let Some(table) = detect_table_block(rope, i) {
            let block_index = table_blocks.len();
            table_blocks.push(TableBlock {
                start_line: i,
                end_line: table.end_line,
                aligns: table.aligns,
                col_widths: table.col_widths,
            });

            for line_idx in i..table.end_line {
                let row_kind = if line_idx == i {
                    TableRowKind::Header
                } else if line_idx == i + 1 {
                    TableRowKind::Separator
                } else {
                    TableRowKind::Body
                };
                kinds.push(MdBlockKind::Table);
                fence_langs.push(None);
                fence_opening.push(false);
                table_lines.push(Some(TableLineInfo {
                    block_index,
                    row_kind,
                }));
            }

            i = table.end_line;
            continue;
        }

        let kind = classify_line(trimmed, &line);
        kinds.push(kind);
        fence_langs.push(None);
        fence_opening.push(false);
        table_lines.push(None);
        i += 1;
    }

    (kinds, fence_langs, fence_opening, table_lines, table_blocks)
}

fn parse_fence_marker(trimmed: &str) -> Option<(u8, usize, &str)> {
    let bytes = trimmed.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }

    let mut run_len = 0;
    while run_len < bytes.len() && bytes[run_len] == marker {
        run_len += 1;
    }
    if run_len < 3 {
        return None;
    }

    Some((marker, run_len, &trimmed[run_len..]))
}

struct TableDetection {
    end_line: usize,
    aligns: Vec<TableAlign>,
    col_widths: Vec<usize>,
}

fn detect_table_block(rope: &ropey::Rope, start_line: usize) -> Option<TableDetection> {
    let total = rope.len_lines().max(1);
    if start_line + 1 >= total {
        return None;
    }

    let header_src = rope_line_without_newline(rope, start_line);
    let sep_src = rope_line_without_newline(rope, start_line + 1);

    let header = parse_table_cells(&header_src)?;
    let separator = parse_table_cells(&sep_src)?;
    if header.is_empty() || header.len() != separator.len() {
        return None;
    }

    let mut aligns = Vec::with_capacity(separator.len());
    for cell in &separator {
        aligns.push(parse_table_alignment(&cell.display_text)?);
    }

    let columns = header.len();
    let mut end_line = start_line + 2;
    while end_line < total {
        let row_src = rope_line_without_newline(rope, end_line);
        if row_src.trim().is_empty() {
            break;
        }
        let Some(cells) = parse_table_cells(&row_src) else {
            break;
        };
        if cells.len() != columns {
            break;
        }
        end_line += 1;
    }

    let mut col_widths = vec![3; columns];
    for line in start_line..end_line {
        if line == start_line + 1 {
            continue;
        }
        let src = rope_line_without_newline(rope, line);
        let Some(cells) = parse_table_cells(&src) else {
            continue;
        };
        for col in 0..columns {
            let raw = cells
                .get(col)
                .map(|cell| cell.display_text.as_str())
                .unwrap_or_default();
            let (rendered, _, _) = render_inline(raw, 0, 0);
            col_widths[col] = col_widths[col].max(rendered.width());
        }
    }

    Some(TableDetection {
        end_line,
        aligns,
        col_widths,
    })
}

fn parse_table_alignment(cell: &str) -> Option<TableAlign> {
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return None;
    }

    let left = trimmed.starts_with(':');
    let right = trimmed.ends_with(':');
    let core = trimmed.trim_matches(':');
    if core.len() < 3 || !core.chars().all(|ch| ch == '-') {
        return None;
    }

    Some(match (left, right) {
        (false, false) | (true, false) => TableAlign::Left,
        (false, true) => TableAlign::Right,
        (true, true) => TableAlign::Center,
    })
}

fn classify_line(trimmed: &str, _full: &str) -> MdBlockKind {
    if trimmed.is_empty() {
        return MdBlockKind::Blank;
    }

    // Heading: # ... (1-6 levels)
    if let Some(rest) = trimmed.strip_prefix('#') {
        let hashes = 1 + rest.len() - rest.trim_start_matches('#').len();
        let after_hashes = &trimmed[hashes..];
        if hashes <= 6 && (after_hashes.is_empty() || after_hashes.starts_with(' ')) {
            return MdBlockKind::Heading(hashes as u8);
        }
    }

    // Horizontal rule: ---, ***, ___  (3+ of same char, optionally with spaces)
    if is_horizontal_rule(trimmed) {
        return MdBlockKind::HorizontalRule;
    }

    // Block quote
    if trimmed.starts_with('>') {
        return MdBlockKind::BlockQuote;
    }

    // Unordered list: - item, * item, + item
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return MdBlockKind::UnorderedList;
    }

    // Ordered list: 1. item
    if let Some(dot_pos) = trimmed.find(". ") {
        if dot_pos <= 9 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return MdBlockKind::OrderedList;
        }
    }

    MdBlockKind::Paragraph
}

fn is_structural_kind(kind: MdBlockKind) -> bool {
    matches!(
        kind,
        MdBlockKind::CodeFence | MdBlockKind::CodeBlock | MdBlockKind::Table
    )
}

fn has_structural_markers_around(rope: &ropey::Rope, line_start: usize, line_end: usize) -> bool {
    let total = rope.len_lines().max(1);
    let start = line_start.saturating_sub(1);
    let end = (line_end + 1).min(total);
    for line in start..end {
        let src = rope_line_without_newline(rope, line);
        if line_has_structural_marker(&src) {
            return true;
        }
    }
    false
}

fn line_has_structural_marker(src: &str) -> bool {
    let trimmed = src.trim_start();
    parse_fence_marker(trimmed).is_some() || parse_table_cells(src).is_some()
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().find(|c| *c != ' ');
    let Some(marker) = first else {
        return false;
    };
    if !matches!(marker, '-' | '*' | '_') {
        return false;
    }
    let count = trimmed.chars().filter(|c| *c == marker).count();
    count >= 3 && trimmed.chars().all(|c| c == marker || c == ' ')
}

// ---------------------------------------------------------------------------
// Line rendering helpers
// ---------------------------------------------------------------------------

fn rope_line_without_newline(rope: &ropey::Rope, line: usize) -> String {
    if line >= rope.len_lines() {
        return String::new();
    }
    let slice = rope.line(line);
    let s = slice.to_string();
    s.trim_end_matches(&['\n', '\r'][..]).to_string()
}

// PLACEHOLDER_RENDER_FUNCTIONS

fn render_heading(src: &str, level: u8) -> MdRenderedLine {
    // Strip leading `# ` markers
    let trimmed = src.trim_start();
    let hashes = trimmed.len() - trimmed.trim_start_matches('#').len();
    let after = &trimmed[hashes..];
    let content = after.strip_prefix(' ').unwrap_or(after);
    let prefix_len = src.len() - content.len(); // bytes consumed by markers

    let (text, mut spans, offset_map) = render_inline(content, 0, prefix_len);
    if !text.is_empty() {
        spans.insert(
            0,
            MdStyleSpan {
                start: 0,
                end: text.len(),
                kind: MdSpanKind::Heading(level),
            },
        );
    }

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_unordered_list(src: &str) -> MdRenderedLine {
    // Replace `- ` / `* ` / `+ ` with `• ` while preserving source mapping.
    let trimmed = src.trim_start();
    let indent = &src[..src.len() - trimmed.len()];
    let rest = &trimmed[2..]; // skip `- ` etc.

    let mut text = String::with_capacity(src.len());
    text.push_str(indent);
    text.push_str("• ");
    let content_start = text.len();
    let src_content_start = src.len() - rest.len();

    let (inline_text, inline_spans, inline_map) =
        render_inline(rest, content_start, src_content_start);
    text.push_str(&inline_text);

    let bullet_start = indent.len();
    let bullet_end = bullet_start + "• ".len();
    let mut spans = vec![MdStyleSpan {
        start: bullet_start,
        end: bullet_end,
        kind: MdSpanKind::Marker,
    }];
    spans.extend(inline_spans);

    let src_indent_len = indent.len();
    let mut offset_map =
        Vec::with_capacity(src_indent_len + (bullet_end - bullet_start) + inline_map.len() + 1);
    for i in 0..src_indent_len {
        offset_map.push((i, i));
    }
    for i in 0..(bullet_end - bullet_start) {
        offset_map.push((src_indent_len + i, src_indent_len));
    }
    offset_map.extend(inline_map);
    finalize_offset_map(&mut offset_map, text.len(), src.len());

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_ordered_list(src: &str) -> MdRenderedLine {
    // Keep the number marker and parse inline markdown in the content.
    let trimmed = src.trim_start();
    let indent = &src[..src.len() - trimmed.len()];
    let dot_pos = trimmed.find(". ").unwrap_or(0);
    let number = &trimmed[..dot_pos + 2]; // "1. "
    let rest = &trimmed[dot_pos + 2..];

    let mut text = String::with_capacity(src.len());
    text.push_str(indent);
    text.push_str(number);
    let content_start = text.len();

    let (inline_text, inline_spans, inline_map) = render_inline(rest, content_start, content_start);
    text.push_str(&inline_text);

    let num_start = indent.len();
    let num_end = num_start + number.len();
    let mut spans = vec![MdStyleSpan {
        start: num_start,
        end: num_end,
        kind: MdSpanKind::Marker,
    }];
    spans.extend(inline_spans);

    let mut offset_map = Vec::with_capacity(content_start + inline_map.len() + 1);
    for i in 0..content_start {
        offset_map.push((i, i));
    }
    offset_map.extend(inline_map);
    finalize_offset_map(&mut offset_map, text.len(), src.len());

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_blockquote(src: &str) -> MdRenderedLine {
    let trimmed = src.trim_start();
    let indent = &src[..src.len() - trimmed.len()];
    let rest = trimmed.strip_prefix('>').unwrap_or(trimmed);
    let rest = rest.strip_prefix(' ').unwrap_or(rest);

    let mut text = String::with_capacity(src.len());
    text.push_str(indent);
    text.push_str("│ ");
    let content_start = text.len();
    let src_content_start = src.len() - rest.len();

    let (inline_text, inline_spans, inline_map) =
        render_inline(rest, content_start, src_content_start);
    text.push_str(&inline_text);

    let bar_start = indent.len();
    let bar_end = bar_start + "│ ".len();
    let mut spans = vec![MdStyleSpan {
        start: bar_start,
        end: bar_end,
        kind: MdSpanKind::BlockquoteBar,
    }];

    if !inline_text.is_empty() {
        spans.push(MdStyleSpan {
            start: content_start,
            end: content_start + inline_text.len(),
            kind: MdSpanKind::BlockquoteText,
        });
    }
    spans.extend(inline_spans);

    let mut offset_map = Vec::with_capacity(content_start + inline_map.len() + 1);
    for i in 0..content_start {
        offset_map.push((i, 0));
    }
    offset_map.extend(inline_map);
    finalize_offset_map(&mut offset_map, text.len(), src.len());

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_hr(width: usize) -> MdRenderedLine {
    let w = width.max(1);
    let text: String = "─".repeat(w);
    let spans = vec![MdStyleSpan {
        start: 0,
        end: text.len(),
        kind: MdSpanKind::HorizontalRule,
    }];
    MdRenderedLine {
        text,
        spans,
        offset_map: Vec::new(),
    }
}

fn parse_table_cells(src: &str) -> Option<Vec<TableCell>> {
    if !src.contains('|') {
        return None;
    }

    let bytes = src.as_bytes();
    let mut separators = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i = i.saturating_add(2);
            continue;
        }
        if bytes[i] == b'|' {
            separators.push(i);
        }
        i += 1;
    }
    if separators.is_empty() {
        return None;
    }

    let mut segments = Vec::with_capacity(separators.len() + 1);
    let mut start = 0usize;
    for sep in separators {
        segments.push((start, sep));
        start = sep + 1;
    }
    segments.push((start, src.len()));

    if src.starts_with('|') && !segments.is_empty() {
        segments.remove(0);
    }
    if src.ends_with('|') && !segments.is_empty() {
        segments.pop();
    }

    if segments.is_empty() {
        return None;
    }

    let mut cells = Vec::with_capacity(segments.len());
    for (seg_start, seg_end) in segments {
        if seg_start > seg_end || seg_end > src.len() {
            return None;
        }

        let raw = &src[seg_start..seg_end];
        let leading = raw.len().saturating_sub(raw.trim_start().len());
        let trailing = raw.len().saturating_sub(raw.trim_end().len());
        let source_start = seg_start + leading;
        let source_end = seg_end.saturating_sub(trailing);
        let display_text = if source_start <= source_end {
            src[source_start..source_end].to_string()
        } else {
            String::new()
        };

        cells.push(TableCell {
            display_text,
            source_start: source_start.min(src.len()),
            source_end: source_end.min(src.len()),
        });
    }

    Some(cells)
}

fn render_table_line(doc: &MarkdownDocument, line: usize, rope: &ropey::Rope) -> MdRenderedLine {
    let Some(info) = doc.table_lines.get(line).copied().flatten() else {
        let src = rope_line_without_newline(rope, line);
        return render_paragraph(&src);
    };
    let Some(block) = doc.table_blocks.get(info.block_index) else {
        let src = rope_line_without_newline(rope, line);
        return render_paragraph(&src);
    };

    if info.row_kind == TableRowKind::Separator {
        let src = rope_line_without_newline(rope, line);
        return render_table_separator(&block.col_widths, src.len());
    }

    let src = rope_line_without_newline(rope, line);
    let cells = parse_table_cells(&src).unwrap_or_default();
    render_table_row(
        &cells,
        &block.col_widths,
        &block.aligns,
        info.row_kind,
        src.len(),
    )
}

fn render_table_separator(widths: &[usize], source_len: usize) -> MdRenderedLine {
    let mut text = String::new();
    let mut offset_map = Vec::new();
    let cols = widths.len();

    for (idx, width) in widths.iter().copied().enumerate() {
        if idx > 0 {
            let pos = text.len();
            text.push('┼');
            offset_map.push((pos, 0));
        }

        let segment_width = if cols <= 1 {
            width
        } else if idx == 0 || idx + 1 == cols {
            width + 1
        } else {
            width + 2
        };

        for _ in 0..segment_width {
            let pos = text.len();
            text.push('─');
            offset_map.push((pos, 0));
        }
    }

    let spans = if text.is_empty() {
        Vec::new()
    } else {
        vec![MdStyleSpan {
            start: 0,
            end: text.len(),
            kind: MdSpanKind::Marker,
        }]
    };

    finalize_offset_map(&mut offset_map, text.len(), source_len);
    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_table_row(
    cells: &[TableCell],
    widths: &[usize],
    aligns: &[TableAlign],
    row_kind: TableRowKind,
    source_len: usize,
) -> MdRenderedLine {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut offset_map = Vec::new();

    for col in 0..widths.len() {
        let cell = cells.get(col);
        let source_start = cell.map_or(source_len, |c| c.source_start);
        let source_end = cell.map_or(source_len, |c| c.source_end);

        if col > 0 {
            let space_left = text.len();
            text.push(' ');
            offset_map.push((space_left, source_start));

            let bar_start = text.len();
            text.push('│');
            let bar_end = text.len();
            offset_map.push((bar_start, source_start));
            spans.push(MdStyleSpan {
                start: bar_start,
                end: bar_end,
                kind: MdSpanKind::Marker,
            });

            let space_right = text.len();
            text.push(' ');
            offset_map.push((space_right, source_start));
        }

        let raw = cell.map(|c| c.display_text.as_str()).unwrap_or_default();

        let (inline_text_probe, _, _) = render_inline(raw, 0, 0);
        let inline_width = inline_text_probe.width();
        let target_width = widths[col];
        let extra = target_width.saturating_sub(inline_width);
        let align = aligns.get(col).copied().unwrap_or(TableAlign::Left);
        let (left_pad, right_pad) = match align {
            TableAlign::Left => (0, extra),
            TableAlign::Right => (extra, 0),
            TableAlign::Center => (extra / 2, extra - (extra / 2)),
        };

        for _ in 0..left_pad {
            let pos = text.len();
            text.push(' ');
            offset_map.push((pos, source_start));
        }

        let inline_display_offset = text.len();
        let (inline_text, mut inline_spans, inline_map) =
            render_inline(raw, inline_display_offset, source_start);
        text.push_str(&inline_text);
        spans.append(&mut inline_spans);
        offset_map.extend(inline_map);

        if row_kind == TableRowKind::Header && !inline_text.is_empty() {
            spans.push(MdStyleSpan {
                start: inline_display_offset,
                end: inline_display_offset + inline_text.len(),
                kind: MdSpanKind::Bold,
            });
        }

        for _ in 0..right_pad {
            let pos = text.len();
            text.push(' ');
            offset_map.push((pos, source_end));
        }
    }

    finalize_offset_map(&mut offset_map, text.len(), source_len);
    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_code_fence(
    src: &str,
    opening: bool,
    lang: Option<&str>,
    viewport_width: usize,
) -> MdRenderedLine {
    let (text, label_range) = if opening {
        let label = if let Some(lang) = lang.filter(|s| !s.is_empty()) {
            format!("code: {lang}")
        } else {
            "code".to_string()
        };
        let mut line = format!("─ {label} ");
        let fill = viewport_width.max(line.chars().count().max(8));
        let used = line.chars().count();
        if used < fill {
            line.push_str(&"─".repeat(fill - used));
        } else if !line.ends_with('─') {
            line.push('─');
        }
        let start = line.find(&label).unwrap_or(0);
        let end = start + label.len();
        (line, Some((start, end)))
    } else {
        ("─".repeat(viewport_width.max(8)), None)
    };

    let mut spans = if text.is_empty() {
        Vec::new()
    } else {
        vec![MdStyleSpan {
            start: 0,
            end: text.len(),
            kind: MdSpanKind::Marker,
        }]
    };
    if let Some((label_start, label_end)) = label_range {
        spans.push(MdStyleSpan {
            start: label_start,
            end: label_end,
            kind: MdSpanKind::Code,
        });
    }

    let mut offset_map = Vec::with_capacity(text.len() + 1);
    for i in 0..text.len() {
        offset_map.push((i, 0));
    }
    finalize_offset_map(&mut offset_map, text.len(), src.len());

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_code_block(src: &str) -> MdRenderedLine {
    let text = src.to_string();
    let spans = if text.is_empty() {
        Vec::new()
    } else {
        vec![MdStyleSpan {
            start: 0,
            end: text.len(),
            kind: MdSpanKind::Code,
        }]
    };

    let mut offset_map = Vec::with_capacity(text.len() + 1);
    for i in 0..text.len() {
        offset_map.push((i, i));
    }
    finalize_offset_map(&mut offset_map, text.len(), src.len());

    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

fn render_paragraph(src: &str) -> MdRenderedLine {
    let (text, spans, offset_map) = render_inline(src, 0, 0);
    MdRenderedLine {
        text,
        spans,
        offset_map,
    }
}

// PLACEHOLDER_INLINE_PARSER

// ---------------------------------------------------------------------------
// Inline formatting parser
// ---------------------------------------------------------------------------

/// Parse inline markdown and produce display text, style spans, and offset map.
fn render_inline(
    src: &str,
    display_offset: usize,
    source_offset: usize,
) -> (String, Vec<MdStyleSpan>, Vec<(usize, usize)>) {
    let mut text = String::with_capacity(src.len());
    let mut spans = Vec::new();
    let mut offset_map = Vec::new();
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Bold: **...**
        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some((content, end)) = find_closing(src, i + 2, "**") {
                let display_start = display_offset + text.len();
                text.push_str(content);
                let display_end = display_offset + text.len();
                spans.push(MdStyleSpan {
                    start: display_start,
                    end: display_end,
                    kind: MdSpanKind::Bold,
                });
                // offset_map: hidden `**` at start, content maps 1:1, hidden `**` at end
                for (j, byte) in content.bytes().enumerate() {
                    let _ = byte;
                    offset_map.push((display_start + j, source_offset + i + 2 + j));
                }
                i = end;
                continue;
            }
        }

        // Italic: *...*  (but not **)
        if bytes[i] == b'*' && (i + 1 >= len || bytes[i + 1] != b'*') {
            if let Some((content, end)) = find_closing_single(src, i + 1, b'*') {
                let display_start = display_offset + text.len();
                text.push_str(content);
                let display_end = display_offset + text.len();
                spans.push(MdStyleSpan {
                    start: display_start,
                    end: display_end,
                    kind: MdSpanKind::Italic,
                });
                for (j, _) in content.bytes().enumerate() {
                    offset_map.push((display_start + j, source_offset + i + 1 + j));
                }
                i = end;
                continue;
            }
        }

        // Strikethrough: ~~...~~
        if i + 1 < len && bytes[i] == b'~' && bytes[i + 1] == b'~' {
            if let Some((content, end)) = find_closing(src, i + 2, "~~") {
                let display_start = display_offset + text.len();
                text.push_str(content);
                let display_end = display_offset + text.len();
                spans.push(MdStyleSpan {
                    start: display_start,
                    end: display_end,
                    kind: MdSpanKind::Strike,
                });
                for (j, _) in content.bytes().enumerate() {
                    offset_map.push((display_start + j, source_offset + i + 2 + j));
                }
                i = end;
                continue;
            }
        }

        // Inline code: `...`
        if bytes[i] == b'`' {
            if let Some((content, content_start, end)) = parse_code_span(src, i) {
                let display_start = display_offset + text.len();
                text.push_str(content);
                let display_end = display_offset + text.len();
                spans.push(MdStyleSpan {
                    start: display_start,
                    end: display_end,
                    kind: MdSpanKind::Code,
                });
                for (j, _) in content.bytes().enumerate() {
                    offset_map.push((display_start + j, source_offset + content_start + j));
                }
                i = end;
                continue;
            }
        }

        // Link: [text](url)
        if bytes[i] == b'[' {
            if let Some((link_text, link_text_start, url_end)) = parse_link(src, i) {
                let display_start = display_offset + text.len();
                text.push_str(link_text);
                let display_end = display_offset + text.len();
                spans.push(MdStyleSpan {
                    start: display_start,
                    end: display_end,
                    kind: MdSpanKind::Link,
                });
                // Map display bytes to source bytes within [text]
                for (j, _) in link_text.bytes().enumerate() {
                    offset_map.push((display_start + j, source_offset + link_text_start + j));
                }
                i = url_end;
                continue;
            }
        }

        // Plain character
        let display_pos = display_offset + text.len();
        offset_map.push((display_pos, source_offset + i));
        text.push(bytes[i] as char);
        // Handle multi-byte UTF-8
        if bytes[i] >= 0x80 {
            // Rewind: use char-based approach
            text.pop();
            let ch = src[i..].chars().next().unwrap_or(' ');
            let ch_len = ch.len_utf8();
            let display_pos = display_offset + text.len();
            // Remove the last offset_map entry we just pushed
            offset_map.pop();
            for j in 0..ch_len {
                offset_map.push((display_pos + j, source_offset + i + j));
            }
            text.push(ch);
            i += ch_len;
        } else {
            i += 1;
        }
    }

    finalize_offset_map(
        &mut offset_map,
        display_offset + text.len(),
        source_offset + src.len(),
    );

    (text, spans, offset_map)
}

fn find_closing<'a>(src: &'a str, start: usize, marker: &str) -> Option<(&'a str, usize)> {
    let rest = src.get(start..)?;
    let pos = rest.find(marker)?;
    if pos == 0 {
        return None; // empty content
    }
    let content = &rest[..pos];
    Some((content, start + pos + marker.len()))
}

fn find_closing_single(src: &str, start: usize, marker: u8) -> Option<(&str, usize)> {
    let bytes = src.as_bytes();
    for j in start..bytes.len() {
        if bytes[j] == marker {
            if j == start {
                return None; // empty
            }
            return Some((&src[start..j], j + 1));
        }
    }
    None
}

fn parse_code_span(src: &str, start: usize) -> Option<(&str, usize, usize)> {
    let bytes = src.as_bytes();
    if bytes.get(start).copied() != Some(b'`') {
        return None;
    }

    let mut marker_len = 0;
    while start + marker_len < bytes.len() && bytes[start + marker_len] == b'`' {
        marker_len += 1;
    }

    let content_start = start + marker_len;
    let mut i = content_start;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }

        let mut run_len = 0;
        while i + run_len < bytes.len() && bytes[i + run_len] == b'`' {
            run_len += 1;
        }
        if run_len == marker_len {
            if i == content_start {
                return None;
            }
            return Some((&src[content_start..i], content_start, i + marker_len));
        }
        i += run_len;
    }

    None
}

fn parse_link(src: &str, start: usize) -> Option<(&str, usize, usize)> {
    let bytes = src.as_bytes();
    if bytes.get(start).copied() != Some(b'[') {
        return None;
    }

    let mut text_end = None;
    let mut depth = 0usize;
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                if depth == 0 {
                    text_end = Some(i);
                    break;
                }
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    let text_end = text_end?;
    let text = &src[start + 1..text_end];
    if text.is_empty() {
        return None;
    }

    // Expect ( immediately after ]
    if bytes.get(text_end + 1).copied() != Some(b'(') {
        return None;
    }

    let mut paren_depth = 0usize;
    let mut j = text_end + 2;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'(' => {
                paren_depth += 1;
                j += 1;
            }
            b')' => {
                if paren_depth == 0 {
                    return Some((text, start + 1, j + 1));
                }
                paren_depth -= 1;
                j += 1;
            }
            _ => j += 1,
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Offset map builders
// ---------------------------------------------------------------------------

fn finalize_offset_map(
    offset_map: &mut Vec<(usize, usize)>,
    display_end: usize,
    source_end: usize,
) {
    if !offset_map
        .last()
        .is_some_and(|&(d, s)| d == display_end && s == source_end)
    {
        offset_map.push((display_end, source_end));
    }
}

// ---------------------------------------------------------------------------
// Source-line highlighting (cursor line)
// ---------------------------------------------------------------------------

fn highlight_source(src: &str, kind: MdBlockKind) -> Vec<super::HighlightSpan> {
    // For the cursor line, we dim the markdown markers so the user can see them
    // but they're visually de-emphasized.
    let mut spans = Vec::new();

    match kind {
        MdBlockKind::Heading(level) => {
            let hashes = level as usize;
            let prefix_end = if src.len() > hashes && src.as_bytes().get(hashes) == Some(&b' ') {
                hashes + 1
            } else {
                hashes
            };
            // Trim leading whitespace
            let leading_ws = src.len() - src.trim_start().len();
            if prefix_end + leading_ws > 0 {
                spans.push(super::HighlightSpan {
                    start: 0,
                    end: leading_ws + prefix_end,
                    kind: super::HighlightKind::Comment, // dim markers
                });
            }
        }
        MdBlockKind::CodeFence | MdBlockKind::HorizontalRule => {
            if !src.is_empty() {
                spans.push(super::HighlightSpan {
                    start: 0,
                    end: src.len(),
                    kind: super::HighlightKind::Comment,
                });
            }
        }
        MdBlockKind::BlockQuote => {
            let trimmed = src.trim_start();
            let indent = src.len() - trimmed.len();
            let marker_end = if trimmed.starts_with("> ") {
                indent + 2
            } else if trimmed.starts_with('>') {
                indent + 1
            } else {
                0
            };
            if marker_end > 0 {
                spans.push(super::HighlightSpan {
                    start: 0,
                    end: marker_end,
                    kind: super::HighlightKind::Comment,
                });
            }
        }
        MdBlockKind::UnorderedList => {
            let trimmed = src.trim_start();
            let indent = src.len() - trimmed.len();
            if trimmed.len() >= 2 {
                spans.push(super::HighlightSpan {
                    start: indent,
                    end: indent + 2,
                    kind: super::HighlightKind::Comment,
                });
            }
        }
        _ => {}
    }

    spans
}

/// Map a display byte offset to a source byte offset using the offset map.
pub fn display_to_source_byte(offset_map: &[(usize, usize)], display_byte: usize) -> usize {
    // Binary search for the closest display byte
    match offset_map.binary_search_by_key(&display_byte, |&(d, _)| d) {
        Ok(idx) => offset_map[idx].1,
        Err(idx) => {
            if idx == 0 {
                0
            } else {
                offset_map[idx - 1].1
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/markdown.rs"]
mod tests;
