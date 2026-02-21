//! Syntax support (in-process): parsing + highlighting helpers.

use crate::kernel::language::LanguageId;
use crate::kernel::services::adapters::perf;
use crate::models::EditOp;
use ropey::Rope;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Comment,
    String,
    Regex,
    Keyword,
    Type,
    Number,
    Attribute,
    Lifetime,
    Function,
    Macro,
    Namespace,
    Variable,
    Constant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AbsHighlightSpan {
    start: usize,
    end: usize,
    kind: HighlightKind,
    depth: usize,
}

#[derive(Debug, Clone)]
struct RangeHighlightCache {
    start_line: usize,
    end_line_exclusive: usize,
    line_count: usize,
    byte_len: usize,
    lines: Arc<Vec<Vec<HighlightSpan>>>,
}

pub struct SyntaxDocument {
    language: LanguageId,
    parser: Parser,
    tree: Tree,
    range_cache: RefCell<Option<RangeHighlightCache>>,
}

impl SyntaxDocument {
    pub fn for_path(path: &Path, rope: &Rope) -> Option<Self> {
        let language = LanguageId::from_path(path)?;
        Self::new(language, rope)
    }

    fn new(language: LanguageId, rope: &Rope) -> Option<Self> {
        let mut parser = Parser::new();
        match language {
            LanguageId::Rust => parser.set_language(tree_sitter_rust::language()).ok()?,
            LanguageId::Go => parser.set_language(tree_sitter_go::language()).ok()?,
            LanguageId::Python => parser.set_language(tree_sitter_python::language()).ok()?,
            LanguageId::C => parser.set_language(tree_sitter_c::language()).ok()?,
            LanguageId::Cpp => parser.set_language(tree_sitter_cpp::language()).ok()?,
            LanguageId::Java => parser.set_language(tree_sitter_java::language()).ok()?,
            LanguageId::JavaScript | LanguageId::Jsx => parser
                .set_language(tree_sitter_javascript::language())
                .ok()?,
            LanguageId::TypeScript => parser
                .set_language(tree_sitter_typescript::language_typescript())
                .ok()?,
            LanguageId::Tsx => parser
                .set_language(tree_sitter_typescript::language_tsx())
                .ok()?,
            LanguageId::Json => parser.set_language(tree_sitter_json::language()).ok()?,
            LanguageId::Yaml => parser.set_language(tree_sitter_yaml::language()).ok()?,
            LanguageId::Html => parser.set_language(tree_sitter_html::language()).ok()?,
            LanguageId::Xml => parser.set_language(tree_sitter_xml::language_xml()).ok()?,
            LanguageId::Css => parser.set_language(tree_sitter_css::language()).ok()?,
            LanguageId::Toml => parser.set_language(tree_sitter_toml::language()).ok()?,
            LanguageId::Sql => parser.set_language(db3_sqlparser::language()).ok()?,
            LanguageId::Bash => parser.set_language(tree_sitter_bash::language()).ok()?,
            LanguageId::Markdown => return None,
        }

        let tree = parse_rope(&mut parser, rope, None)?;
        Some(Self {
            language,
            parser,
            tree,
            range_cache: RefCell::new(None),
        })
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    fn clear_highlight_cache(&self) {
        *self.range_cache.borrow_mut() = None;
    }

    pub fn reparse(&mut self, rope: &Rope) {
        if let Some(tree) = parse_rope(&mut self.parser, rope, None) {
            self.tree = tree;
        }
        self.clear_highlight_cache();
    }

    pub fn apply_edit(&mut self, rope: &Rope, op: &EditOp) {
        let Some(edit) = build_input_edit(rope, op) else {
            self.reparse(rope);
            return;
        };

        self.tree.edit(&edit);
        if let Some(tree) = parse_rope(&mut self.parser, rope, Some(&self.tree)) {
            self.tree = tree;
        } else {
            self.reparse(rope);
            return;
        }
        self.clear_highlight_cache();
    }

    pub fn is_in_string_or_comment(&self, byte_offset: usize) -> bool {
        let root = self.tree.root_node();
        let Some(mut node) = root.descendant_for_byte_range(byte_offset, byte_offset) else {
            return false;
        };

        loop {
            let kind = node.kind();
            if is_comment_kind(kind) || is_string_kind(kind) {
                return true;
            }
            match node.parent() {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    pub fn highlight_lines(
        &self,
        rope: &Rope,
        start_line: usize,
        end_line_exclusive: usize,
    ) -> Vec<Vec<HighlightSpan>> {
        self.highlight_lines_shared(rope, start_line, end_line_exclusive)
            .as_ref()
            .clone()
    }

    pub fn highlight_lines_shared(
        &self,
        rope: &Rope,
        start_line: usize,
        end_line_exclusive: usize,
    ) -> Arc<Vec<Vec<HighlightSpan>>> {
        let _scope = perf::scope("syntax.highlight");
        if start_line >= end_line_exclusive {
            return Arc::new(Vec::new());
        }

        let total_lines = rope.len_lines().max(1);
        let start_line = start_line.min(total_lines);
        let end_line_exclusive = end_line_exclusive.min(total_lines);
        if start_line >= end_line_exclusive {
            return Arc::new(Vec::new());
        }

        let byte_len = rope.len_bytes();
        if let Some(cache) = self.range_cache.borrow().as_ref() {
            if cache.start_line == start_line
                && cache.end_line_exclusive == end_line_exclusive
                && cache.line_count == total_lines
                && cache.byte_len == byte_len
            {
                return Arc::clone(&cache.lines);
            }
        }

        let _scope = perf::scope("syntax.highlight.range");
        let range_start = rope.line_to_byte(start_line);
        let range_end = rope.line_to_byte(end_line_exclusive);
        let spans = collect_highlights(self.language, &self.tree, rope, range_start, range_end);
        let lines = Arc::new(project_abs_spans_to_lines(
            rope,
            start_line,
            end_line_exclusive,
            &spans,
        ));
        *self.range_cache.borrow_mut() = Some(RangeHighlightCache {
            start_line,
            end_line_exclusive,
            line_count: total_lines,
            byte_len,
            lines: Arc::clone(&lines),
        });
        lines
    }
}

/// Highlight an arbitrary snippet (e.g. LSP hover / completion documentation code fences).
///
/// Returns per-line highlight spans with offsets relative to each line start.
pub fn highlight_snippet(language: LanguageId, text: &str) -> Vec<Vec<HighlightSpan>> {
    let rope = Rope::from_str(text);
    let total_lines = rope.len_lines().max(1);

    let mut parser = Parser::new();
    let language_set = match language {
        LanguageId::Rust => parser.set_language(tree_sitter_rust::language()).is_ok(),
        LanguageId::Go => parser.set_language(tree_sitter_go::language()).is_ok(),
        LanguageId::Python => parser.set_language(tree_sitter_python::language()).is_ok(),
        LanguageId::C => parser.set_language(tree_sitter_c::language()).is_ok(),
        LanguageId::Cpp => parser.set_language(tree_sitter_cpp::language()).is_ok(),
        LanguageId::Java => parser.set_language(tree_sitter_java::language()).is_ok(),
        LanguageId::JavaScript | LanguageId::Jsx => parser
            .set_language(tree_sitter_javascript::language())
            .is_ok(),
        LanguageId::TypeScript => parser
            .set_language(tree_sitter_typescript::language_typescript())
            .is_ok(),
        LanguageId::Tsx => parser
            .set_language(tree_sitter_typescript::language_tsx())
            .is_ok(),
        LanguageId::Json => parser.set_language(tree_sitter_json::language()).is_ok(),
        LanguageId::Yaml => parser.set_language(tree_sitter_yaml::language()).is_ok(),
        LanguageId::Html => parser.set_language(tree_sitter_html::language()).is_ok(),
        LanguageId::Xml => parser.set_language(tree_sitter_xml::language_xml()).is_ok(),
        LanguageId::Css => parser.set_language(tree_sitter_css::language()).is_ok(),
        LanguageId::Toml => parser.set_language(tree_sitter_toml::language()).is_ok(),
        LanguageId::Sql => parser.set_language(db3_sqlparser::language()).is_ok(),
        LanguageId::Bash => parser.set_language(tree_sitter_bash::language()).is_ok(),
        LanguageId::Markdown => false,
    };
    if !language_set {
        return vec![Vec::new(); total_lines];
    }

    let Some(tree) = parse_rope(&mut parser, &rope, None) else {
        return vec![Vec::new(); total_lines];
    };

    let start_byte = 0;
    let end_byte = rope.len_bytes();
    let spans = collect_highlights(language, &tree, &rope, start_byte, end_byte);
    project_abs_spans_to_lines(&rope, 0, total_lines, &spans)
}

fn parse_rope(parser: &mut Parser, rope: &Rope, old_tree: Option<&Tree>) -> Option<Tree> {
    let mut cache = RopeChunkCache::new(rope);
    parser.parse_with(
        &mut |byte_offset, _| cache.bytes_from(byte_offset),
        old_tree,
    )
}

struct RopeChunkCache<'a> {
    rope: &'a Rope,
    chunk: &'a str,
    start: usize,
    end: usize,
}

impl<'a> RopeChunkCache<'a> {
    fn new(rope: &'a Rope) -> Self {
        Self {
            rope,
            chunk: "",
            start: 0,
            end: 0,
        }
    }

    fn bytes_from(&mut self, byte_offset: usize) -> &'a [u8] {
        if byte_offset >= self.rope.len_bytes() {
            return &[];
        }

        if byte_offset < self.start || byte_offset >= self.end {
            let (chunk, chunk_start, _, _) = self.rope.chunk_at_byte(byte_offset);
            self.chunk = chunk;
            self.start = chunk_start;
            self.end = chunk_start + chunk.len();
        }

        let rel = byte_offset.saturating_sub(self.start);
        &self.chunk.as_bytes()[rel..]
    }
}

fn build_input_edit(rope: &Rope, op: &EditOp) -> Option<InputEdit> {
    let (start_char, old_text, new_text) = match &op.kind {
        crate::models::edit_op::OpKind::Insert { char_offset, text } => {
            (*char_offset, "", text.as_str())
        }
        crate::models::edit_op::OpKind::Delete {
            start,
            end: _,
            deleted,
        } => (*start, deleted.as_str(), ""),
        crate::models::edit_op::OpKind::Replace {
            start,
            end: _,
            deleted,
            inserted,
        } => (*start, deleted.as_str(), inserted.as_str()),
    };

    if start_char > rope.len_chars() {
        return None;
    }

    let start_byte = rope.char_to_byte(start_char);
    let start_position = point_for_char(rope, start_char, start_byte)?;

    let old_end_byte = start_byte + old_text.len();
    let new_end_byte = start_byte + new_text.len();

    let old_end_position = advance_point(start_position, old_text);
    let new_end_position = advance_point(start_position, new_text);

    Some(InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position,
        old_end_position,
        new_end_position,
    })
}

fn point_for_char(rope: &Rope, char_idx: usize, byte_idx: usize) -> Option<Point> {
    let row = rope.char_to_line(char_idx);
    let line_start = rope.line_to_byte(row);
    let col = byte_idx.saturating_sub(line_start);

    Some(Point { row, column: col })
}

fn advance_point(start: Point, text: &str) -> Point {
    let mut row = start.row;
    let mut col = start.column;

    for &b in text.as_bytes() {
        if b == b'\n' {
            row = row.saturating_add(1);
            col = 0;
        } else {
            col = col.saturating_add(1);
        }
    }

    Point { row, column: col }
}

fn merge_adjacent_spans(spans: &mut Vec<HighlightSpan>) {
    if spans.len() <= 1 {
        return;
    }

    let mut out: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        if let Some(prev) = out.last_mut() {
            if prev.kind == span.kind && span.start <= prev.end {
                prev.end = prev.end.max(span.end);
                continue;
            }
        }
        out.push(span);
    }
    *spans = out;
}

fn project_abs_spans_to_lines(
    rope: &Rope,
    start_line: usize,
    end_line_exclusive: usize,
    spans: &[AbsHighlightSpan],
) -> Vec<Vec<HighlightSpan>> {
    if start_line >= end_line_exclusive {
        return Vec::new();
    }

    let total_lines = rope.len_lines().max(1);
    let start_line = start_line.min(total_lines);
    let end_line_exclusive = end_line_exclusive.min(total_lines);
    if start_line >= end_line_exclusive {
        return Vec::new();
    }

    let range_start = rope.line_to_byte(start_line);
    let range_end = rope.line_to_byte(end_line_exclusive);
    let mut per_line = vec![Vec::new(); end_line_exclusive - start_line];

    for span in spans {
        let span_start = span.start.max(range_start).min(range_end);
        let span_end = span.end.max(range_start).min(range_end);
        if span_start >= span_end {
            continue;
        }

        let first_line = rope.byte_to_line(span_start);
        let last_line = rope.byte_to_line(span_end.saturating_sub(1));
        let line_lo = first_line.max(start_line);
        let line_hi = last_line.min(end_line_exclusive.saturating_sub(1));

        for line in line_lo..=line_hi {
            let line_start = rope.line_to_byte(line);
            let line_end = rope.line_to_byte((line + 1).min(total_lines));

            let s = span_start.max(line_start);
            let e = span_end.min(line_end);
            if s >= e {
                continue;
            }

            per_line[line - start_line].push(HighlightSpan {
                start: s - line_start,
                end: e - line_start,
                kind: span.kind,
            });
        }
    }

    for line_spans in &mut per_line {
        merge_adjacent_spans(line_spans);
    }

    per_line
}

fn collect_highlights(
    language: LanguageId,
    tree: &Tree,
    rope: &Rope,
    start_byte: usize,
    end_byte: usize,
) -> Vec<AbsHighlightSpan> {
    let root = tree.root_node();
    let mut stack = vec![(root, 0usize)];
    let mut spans = Vec::new();

    while let Some((node, depth)) = stack.pop() {
        let node_start = node.start_byte();
        let node_end = node.end_byte();

        if node_end <= start_byte || node_start >= end_byte {
            continue;
        }

        if let Some(kind) = classify_node(language, node, rope) {
            spans.push(AbsHighlightSpan {
                start: node_start,
                end: node_end,
                kind,
                depth,
            });

            if matches!(
                kind,
                HighlightKind::Comment
                    | HighlightKind::String
                    | HighlightKind::Regex
                    | HighlightKind::Attribute
            ) {
                continue;
            }
        }

        let child_count = node.child_count();
        for i in (0..child_count).rev() {
            if let Some(child) = node.child(i) {
                stack.push((child, depth.saturating_add(1)));
            }
        }
    }

    let mut normalized = normalize_overlapping_highlight_spans(spans, start_byte, end_byte);
    if language == LanguageId::Sql {
        let supplemental = collect_sql_fallback_spans(rope, start_byte, end_byte, &normalized);
        if !supplemental.is_empty() {
            normalized.extend(supplemental);
            normalized = normalize_overlapping_highlight_spans(normalized, start_byte, end_byte);
        }
    }

    normalized
}

fn collect_sql_fallback_spans(
    rope: &Rope,
    start_byte: usize,
    end_byte: usize,
    existing: &[AbsHighlightSpan],
) -> Vec<AbsHighlightSpan> {
    if start_byte >= end_byte {
        return Vec::new();
    }

    let start_char = rope.byte_to_char(start_byte);
    let end_char = rope.byte_to_char(end_byte);
    let text = rope.slice(start_char..end_char).to_string();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut existing_idx = 0usize;

    while i < bytes.len() {
        let abs = start_byte + i;
        while existing_idx < existing.len() && existing[existing_idx].end <= abs {
            existing_idx += 1;
        }
        if let Some(active) = existing
            .get(existing_idx)
            .filter(|span| span.start <= abs && abs < span.end)
        {
            i = active.end.saturating_sub(start_byte).min(bytes.len());
            continue;
        }

        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            let token_start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i,
                kind: HighlightKind::Comment,
                depth: 0,
            });
            continue;
        }

        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let token_start = i;
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i.min(bytes.len()),
                kind: HighlightKind::Comment,
                depth: 0,
            });
            continue;
        }

        if b == b'\'' {
            let token_start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i.min(bytes.len()),
                kind: HighlightKind::String,
                depth: 0,
            });
            continue;
        }

        if b.is_ascii_digit() {
            let token_start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i,
                kind: HighlightKind::Number,
                depth: 0,
            });
            continue;
        }

        if is_sql_identifier_start(b) {
            let token_start = i;
            i += 1;
            while i < bytes.len() && is_sql_identifier_continue(bytes[i]) {
                i += 1;
            }
            let token = &text[token_start..i];
            if let Some(kind) = classify_sql_word(token) {
                out.push(AbsHighlightSpan {
                    start: start_byte + token_start,
                    end: start_byte + i,
                    kind,
                    depth: 0,
                });
            }
            continue;
        }

        i += 1;
    }

    out
}

fn is_sql_identifier_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_sql_identifier_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn classify_sql_word(word: &str) -> Option<HighlightKind> {
    let upper = word.to_ascii_uppercase();
    if is_sql_type_name(upper.as_str()) {
        return Some(HighlightKind::Type);
    }
    if is_sql_reserved_word(upper.as_str()) {
        return Some(HighlightKind::Keyword);
    }
    None
}

fn normalize_overlapping_highlight_spans(
    spans: Vec<AbsHighlightSpan>,
    start_byte: usize,
    end_byte: usize,
) -> Vec<AbsHighlightSpan> {
    if spans.is_empty() || start_byte >= end_byte {
        return Vec::new();
    }

    let mut active_keys = Vec::with_capacity(spans.len());
    let mut events = Vec::with_capacity(spans.len().saturating_mul(2));
    for (seq, span) in spans.into_iter().enumerate() {
        let clipped_start = span.start.max(start_byte);
        let clipped_end = span.end.min(end_byte);
        if clipped_start >= clipped_end {
            continue;
        }

        let id = active_keys.len();
        active_keys.push(ActiveSpanKey::from_span(span, seq));
        events.push(SpanEvent {
            pos: clipped_start,
            kind: SpanEventKind::Start,
            id,
        });
        events.push(SpanEvent {
            pos: clipped_end,
            kind: SpanEventKind::End,
            id,
        });
    }

    if events.is_empty() {
        return Vec::new();
    }

    events.sort_by(|a, b| {
        a.pos
            .cmp(&b.pos)
            .then_with(|| match (a.kind, b.kind) {
                (SpanEventKind::End, SpanEventKind::Start) => Ordering::Less,
                (SpanEventKind::Start, SpanEventKind::End) => Ordering::Greater,
                _ => Ordering::Equal,
            })
            .then(a.id.cmp(&b.id))
    });

    let mut active: BTreeSet<ActiveSpanKey> = BTreeSet::new();
    let mut flattened: Vec<AbsHighlightSpan> = Vec::with_capacity(events.len() / 2);
    let mut prev_pos = events[0].pos;

    for event in events {
        if prev_pos < event.pos {
            if let Some(top) = active.iter().next_back() {
                flattened.push(AbsHighlightSpan {
                    start: prev_pos,
                    end: event.pos,
                    kind: top.kind,
                    depth: top.depth,
                });
            }
            prev_pos = event.pos;
        }

        let key = active_keys[event.id];
        match event.kind {
            SpanEventKind::Start => {
                active.insert(key);
            }
            SpanEventKind::End => {
                active.remove(&key);
            }
        }
    }

    merge_adjacent_abs_spans(&mut flattened);
    flattened
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanEventKind {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpanEvent {
    pos: usize,
    kind: SpanEventKind,
    id: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveSpanKey {
    depth: usize,
    len: usize,
    start: usize,
    end: usize,
    seq: usize,
    kind: HighlightKind,
}

impl ActiveSpanKey {
    fn from_span(span: AbsHighlightSpan, seq: usize) -> Self {
        Self {
            depth: span.depth,
            len: span.end.saturating_sub(span.start),
            start: span.start,
            end: span.end,
            seq,
            kind: span.kind,
        }
    }
}

impl Ord for ActiveSpanKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.depth
            .cmp(&other.depth)
            .then_with(|| other.len.cmp(&self.len))
            .then_with(|| other.start.cmp(&self.start))
            .then_with(|| other.end.cmp(&self.end))
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for ActiveSpanKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn merge_adjacent_abs_spans(spans: &mut Vec<AbsHighlightSpan>) {
    if spans.len() <= 1 {
        return;
    }

    let mut out: Vec<AbsHighlightSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        if let Some(prev) = out.last_mut() {
            if prev.kind == span.kind && prev.depth == span.depth && span.start <= prev.end {
                prev.end = prev.end.max(span.end);
                continue;
            }
        }
        out.push(span);
    }
    *spans = out;
}

fn classify_node(language: LanguageId, node: Node<'_>, rope: &Rope) -> Option<HighlightKind> {
    let kind = node.kind();

    if is_comment_kind(kind) {
        return Some(HighlightKind::Comment);
    }
    if is_regex_kind(kind) {
        return Some(HighlightKind::Regex);
    }
    if language == LanguageId::Python && is_string_kind(kind) {
        if let Some(kind) = classify_python_string(node, rope) {
            return Some(kind);
        }
    }
    if is_string_kind(kind) {
        return Some(HighlightKind::String);
    }
    if kind.contains("integer") || kind.contains("float") || kind.contains("number") {
        return Some(HighlightKind::Number);
    }
    if kind.ends_with("_literal") && (kind.contains("int") || kind.contains("imaginary")) {
        return Some(HighlightKind::Number);
    }
    if matches!(
        kind,
        "type_identifier" | "primitive_type" | "predefined_type"
    ) {
        return Some(HighlightKind::Type);
    }
    if matches!(
        kind,
        "attribute_item" | "inner_attribute_item" | "decorator"
    ) {
        return Some(HighlightKind::Attribute);
    }
    if kind == "lifetime" {
        return Some(HighlightKind::Lifetime);
    }
    match language {
        LanguageId::Rust => {
            if let Some(kind) = classify_rust_node(node) {
                return Some(kind);
            }
        }
        LanguageId::Go => {
            if let Some(kind) = classify_go_node(node) {
                return Some(kind);
            }
        }
        LanguageId::Python => {
            if let Some(kind) = classify_python_node(node, rope) {
                return Some(kind);
            }
        }
        LanguageId::C | LanguageId::Cpp | LanguageId::Java | LanguageId::Sql => {}
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Jsx | LanguageId::Tsx => {
            if let Some(kind) = classify_js_node(node) {
                return Some(kind);
            }
        }
        LanguageId::Json | LanguageId::Yaml | LanguageId::Toml | LanguageId::Markdown => {}
        LanguageId::Html | LanguageId::Xml => {
            if let Some(kind) = classify_markup_node(node) {
                return Some(kind);
            }
        }
        LanguageId::Css => {
            if let Some(kind) = classify_css_node(node) {
                return Some(kind);
            }
        }
        LanguageId::Bash => {
            if let Some(kind) = classify_bash_node(node) {
                return Some(kind);
            }
        }
    }
    if is_keyword(language, kind) {
        return Some(HighlightKind::Keyword);
    }
    None
}

fn classify_rust_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "identifier" => classify_rust_identifier(node),
        "field_identifier" => classify_rust_field_identifier(node),
        "macro_invocation" => Some(HighlightKind::Macro),
        _ => None,
    }
}

fn classify_rust_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "function_item" | "function_signature_item" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Function)
        }
        "call_expression" if node_is_field(parent, "function", node) => {
            Some(HighlightKind::Function)
        }
        "const_item" | "static_item" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Constant)
        }
        "let_declaration" if node_is_field(parent, "pattern", node) => {
            Some(HighlightKind::Variable)
        }
        "parameter" | "closure_parameters" => Some(HighlightKind::Variable),
        "scoped_identifier" => {
            if parent.parent().is_some_and(|grand| {
                grand.kind() == "call_expression" && node_is_field(grand, "function", parent)
            }) {
                // In `HashMap::new()`, only the `name` part (`new`) is Function;
                // the `path` part (`HashMap`) is Type.
                if node_is_field(parent, "name", node) {
                    Some(HighlightKind::Function)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_rust_field_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    if parent.kind() == "field_expression" {
        if parent.parent().is_some_and(|grand| {
            grand.kind() == "call_expression" && node_is_field(grand, "function", parent)
        }) {
            return Some(HighlightKind::Function);
        }
        return Some(HighlightKind::Variable);
    }
    if parent.kind() == "field_declaration" {
        return Some(HighlightKind::Variable);
    }
    None
}

fn classify_js_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => classify_js_identifier(node),
        "property_identifier" => classify_js_property_identifier(node),
        _ => None,
    }
}

fn classify_js_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "function_declaration"
        | "function"
        | "generator_function_declaration"
        | "generator_function"
            if node_is_field(parent, "name", node) =>
        {
            Some(HighlightKind::Function)
        }
        "call_expression" if node_is_field(parent, "function", node) => {
            Some(HighlightKind::Function)
        }
        "class_declaration" | "class" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Type)
        }
        "variable_declarator" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Variable)
        }
        "formal_parameters" => Some(HighlightKind::Variable),
        "method_definition" if node_is_field(parent, "name", node) => Some(HighlightKind::Function),
        _ => None,
    }
}

fn classify_js_property_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    if parent.kind() == "member_expression" {
        if parent.parent().is_some_and(|grand| {
            grand.kind() == "call_expression" && node_is_field(grand, "function", parent)
        }) {
            return Some(HighlightKind::Function);
        }
        return Some(HighlightKind::Variable);
    }
    None
}

fn classify_go_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "package_identifier" | "label_name" => Some(HighlightKind::Attribute),
        "identifier" => classify_go_identifier(node),
        "field_identifier" => classify_go_field_identifier(node),
        _ => None,
    }
}

fn classify_go_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "function_declaration" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Function)
        }
        "call_expression" if node_is_field(parent, "function", node) => {
            Some(HighlightKind::Function)
        }
        "parameter_declaration" => {
            if parent
                .parent()
                .is_some_and(|grand| grand.kind() == "type_parameter_list")
            {
                Some(HighlightKind::Type)
            } else {
                Some(HighlightKind::Variable)
            }
        }
        "variadic_parameter_declaration" => Some(HighlightKind::Variable),
        "const_spec" | "var_spec" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Variable)
        }
        "expression_list" => {
            let grand = parent.parent()?;
            if matches!(
                grand.kind(),
                "short_var_declaration"
                    | "assignment_statement"
                    | "range_clause"
                    | "receive_statement"
            ) && node_is_field(grand, "left", parent)
            {
                Some(HighlightKind::Variable)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_go_field_identifier(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "method_declaration" | "method_spec" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Function)
        }
        "field_declaration" => Some(HighlightKind::Variable),
        "selector_expression" if node_is_field(parent, "field", node) => {
            if parent.parent().is_some_and(|grand| {
                grand.kind() == "call_expression" && node_is_field(grand, "function", parent)
            }) {
                Some(HighlightKind::Function)
            } else {
                Some(HighlightKind::Variable)
            }
        }
        _ => None,
    }
}

fn classify_python_node(node: Node<'_>, rope: &Rope) -> Option<HighlightKind> {
    match node.kind() {
        "identifier" => classify_python_identifier(node, rope),
        _ => None,
    }
}

fn classify_python_identifier(node: Node<'_>, rope: &Rope) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "function_definition" if node_is_field(parent, "name", node) => {
            Some(HighlightKind::Function)
        }
        "class_definition" if node_is_field(parent, "name", node) => Some(HighlightKind::Type),
        "call" if node_is_field(parent, "function", node) => {
            Some(classify_python_callable_identifier(node, rope))
        }
        "attribute" if node_is_field(parent, "attribute", node) => {
            if parent.parent().is_some_and(|grand| {
                grand.kind() == "call" && node_is_field(grand, "function", parent)
            }) {
                Some(classify_python_callable_identifier(node, rope))
            } else {
                Some(HighlightKind::Variable)
            }
        }
        "parameters" | "lambda_parameters" => Some(HighlightKind::Variable),
        "typed_parameter"
        | "default_parameter"
        | "typed_default_parameter"
        | "list_splat_pattern"
        | "dictionary_splat_pattern" => Some(HighlightKind::Variable),
        "assignment" | "augmented_assignment" => {
            if node_in_field_subtree(parent, "left", node) {
                if is_python_constant_identifier(node, rope) {
                    Some(HighlightKind::Constant)
                } else {
                    Some(HighlightKind::Variable)
                }
            } else {
                None
            }
        }
        "for_statement" => {
            if node_in_field_subtree(parent, "left", node) {
                Some(HighlightKind::Variable)
            } else {
                None
            }
        }
        "named_expression" if node_is_field(parent, "name", node) => Some(HighlightKind::Variable),
        "keyword_argument" if node_is_field(parent, "name", node) => Some(HighlightKind::Variable),
        "global_statement" | "nonlocal_statement" => Some(HighlightKind::Variable),
        "aliased_import" if node_is_field(parent, "alias", node) => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn classify_python_string(node: Node<'_>, rope: &Rope) -> Option<HighlightKind> {
    let mut current = Some(node);
    while let Some(cursor) = current {
        if cursor.kind() == "call" && node_in_field_subtree(cursor, "arguments", node) {
            if !is_first_python_call_argument(cursor, node) {
                return None;
            }

            let function = cursor.child_by_field_name("function")?;
            let callee = classify_python_call_callee_name(function, rope)?;
            if is_python_regex_callee(callee.as_str()) {
                return Some(HighlightKind::Regex);
            }
            return None;
        }
        current = cursor.parent();
    }
    None
}

fn classify_python_call_callee_name(node: Node<'_>, rope: &Rope) -> Option<String> {
    match node.kind() {
        "identifier" => node_text_trimmed(rope, node),
        "attribute" => {
            let object = node.child_by_field_name("object")?;
            let attribute = node.child_by_field_name("attribute")?;
            let object_name = classify_python_call_callee_name(object, rope)?;
            let attribute_name = node_text_trimmed(rope, attribute)?;
            Some(format!("{object_name}.{attribute_name}"))
        }
        _ => None,
    }
}

fn is_python_regex_callee(callee: &str) -> bool {
    matches!(
        callee,
        "re.compile"
            | "re.search"
            | "re.match"
            | "re.fullmatch"
            | "re.sub"
            | "re.subn"
            | "re.findall"
            | "re.finditer"
            | "re.split"
            | "regex.compile"
            | "regex.search"
            | "regex.match"
            | "regex.fullmatch"
            | "regex.sub"
            | "regex.subn"
            | "regex.findall"
            | "regex.finditer"
            | "regex.split"
    )
}

fn is_first_python_call_argument(call: Node<'_>, node: Node<'_>) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };

    let Some(first) = arguments.named_child(0) else {
        return false;
    };

    node_contains(first, node)
}

fn node_contains(ancestor: Node<'_>, descendant: Node<'_>) -> bool {
    let mut current = Some(descendant);
    while let Some(cursor) = current {
        if same_node(cursor, ancestor) {
            return true;
        }
        current = cursor.parent();
    }
    false
}

fn classify_python_callable_identifier(node: Node<'_>, rope: &Rope) -> HighlightKind {
    if node_text_trimmed(rope, node).is_some_and(|name| is_python_type_name(name.as_str())) {
        HighlightKind::Type
    } else {
        HighlightKind::Function
    }
}

fn is_python_constant_identifier(node: Node<'_>, rope: &Rope) -> bool {
    node_text_trimmed(rope, node).is_some_and(|name| is_python_constant_name(name.as_str()))
}

fn is_python_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_python_constant_name(name: &str) -> bool {
    let mut has_uppercase = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            has_uppercase = true;
            continue;
        }
        if ch.is_ascii_digit() || ch == '_' {
            continue;
        }
        return false;
    }
    has_uppercase
}

fn node_text_trimmed(rope: &Rope, node: Node<'_>) -> Option<String> {
    let text = node_text(rope, node)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn node_text(rope: &Rope, node: Node<'_>) -> Option<String> {
    if node.end_byte() > rope.len_bytes() {
        return None;
    }
    let start_char = rope.byte_to_char(node.start_byte());
    let end_char = rope.byte_to_char(node.end_byte());
    Some(rope.slice(start_char..end_char).to_string())
}

fn node_is_field(parent: Node<'_>, field_name: &str, node: Node<'_>) -> bool {
    parent
        .child_by_field_name(field_name)
        .is_some_and(|field| same_node(field, node))
}

fn node_in_field_subtree(parent: Node<'_>, field_name: &str, node: Node<'_>) -> bool {
    let Some(field_node) = parent.child_by_field_name(field_name) else {
        return false;
    };

    let mut current = Some(node);
    while let Some(cursor) = current {
        if same_node(cursor, field_node) {
            return true;
        }
        if same_node(cursor, parent) {
            break;
        }
        current = cursor.parent();
    }
    false
}

fn same_node(left: Node<'_>, right: Node<'_>) -> bool {
    left.start_byte() == right.start_byte() && left.end_byte() == right.end_byte()
}

fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

fn is_regex_kind(kind: &str) -> bool {
    kind.contains("regex") || kind == "regular_expression"
}

fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || matches!(kind, "char_literal" | "byte_literal")
}

fn is_keyword(language: LanguageId, kind: &str) -> bool {
    match language {
        LanguageId::Rust => is_rust_keyword(kind),
        LanguageId::Go => is_go_keyword(kind),
        LanguageId::Python => is_python_keyword(kind),
        LanguageId::C => is_c_keyword(kind),
        LanguageId::Cpp => is_cpp_keyword(kind),
        LanguageId::Java => is_java_keyword(kind),
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Jsx | LanguageId::Tsx => {
            is_js_ts_keyword(kind)
        }
        LanguageId::Json => is_json_keyword(kind),
        LanguageId::Yaml => is_yaml_keyword(kind),
        LanguageId::Toml => is_toml_keyword(kind),
        LanguageId::Sql => is_sql_keyword(kind),
        LanguageId::Html | LanguageId::Xml => false,
        LanguageId::Css => false,
        LanguageId::Bash => is_bash_keyword(kind),
        LanguageId::Markdown => false,
    }
}

fn is_c_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "_Bool"
            | "_Complex"
            | "_Imaginary"
    )
}

fn is_cpp_keyword(kind: &str) -> bool {
    is_c_keyword(kind)
        || matches!(
            kind,
            "alignas"
                | "alignof"
                | "and"
                | "and_eq"
                | "asm"
                | "bitand"
                | "bitor"
                | "bool"
                | "catch"
                | "class"
                | "compl"
                | "concept"
                | "constexpr"
                | "consteval"
                | "constinit"
                | "delete"
                | "dynamic_cast"
                | "explicit"
                | "export"
                | "false"
                | "friend"
                | "mutable"
                | "namespace"
                | "new"
                | "noexcept"
                | "not"
                | "not_eq"
                | "nullptr"
                | "operator"
                | "or"
                | "or_eq"
                | "private"
                | "protected"
                | "public"
                | "reinterpret_cast"
                | "requires"
                | "static_assert"
                | "template"
                | "this"
                | "thread_local"
                | "throw"
                | "true"
                | "try"
                | "typeid"
                | "typename"
                | "using"
                | "virtual"
                | "wchar_t"
                | "xor"
                | "xor_eq"
        )
}

fn is_java_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
            | "true"
            | "false"
            | "null"
            | "record"
            | "sealed"
            | "permits"
            | "var"
            | "yield"
    )
}

fn is_rust_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

fn is_go_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "break"
            | "case"
            | "chan"
            | "const"
            | "continue"
            | "default"
            | "defer"
            | "else"
            | "fallthrough"
            | "for"
            | "func"
            | "go"
            | "goto"
            | "if"
            | "import"
            | "interface"
            | "map"
            | "package"
            | "range"
            | "return"
            | "select"
            | "struct"
            | "switch"
            | "type"
            | "true"
            | "false"
            | "nil"
            | "iota"
            | "var"
    )
}

fn is_python_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn is_js_ts_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "async"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "export"
            | "extends"
            | "finally"
            | "for"
            | "from"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "of"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "try"
            | "type"
            | "typeof"
            | "undefined"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "true"
            | "false"
    )
}

fn is_json_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false" | "null")
}

fn is_yaml_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false" | "null")
}

fn is_toml_keyword(kind: &str) -> bool {
    matches!(kind, "true" | "false")
}

fn is_bash_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "case"
            | "esac"
            | "for"
            | "while"
            | "until"
            | "do"
            | "done"
            | "in"
            | "function"
            | "select"
            | "return"
            | "exit"
            | "local"
            | "declare"
            | "export"
            | "readonly"
            | "unset"
    )
}

fn is_sql_keyword(kind: &str) -> bool {
    if kind.starts_with("keyword_") {
        return true;
    }
    if matches!(kind, "ERROR" | "MISSING") {
        return false;
    }
    if is_sql_reserved_word(kind) {
        return true;
    }
    if is_sql_type_name(kind) {
        return true;
    }

    let mut has_alpha = false;
    for b in kind.bytes() {
        if b.is_ascii_alphabetic() {
            has_alpha = true;
            if !b.is_ascii_uppercase() {
                return false;
            }
            continue;
        }
        if b != b'_' {
            return false;
        }
    }
    has_alpha
}

fn is_sql_reserved_word(upper: &str) -> bool {
    matches!(
        upper,
        "ALL"
            | "ALTER"
            | "AND"
            | "AS"
            | "ASC"
            | "BY"
            | "CASE"
            | "CASCADE"
            | "CHECK"
            | "CONSTRAINT"
            | "CREATE"
            | "CROSS"
            | "CURRENT_DATE"
            | "CURRENT_TIME"
            | "CURRENT_TIMESTAMP"
            | "DEFAULT"
            | "DELETE"
            | "DESC"
            | "DISTINCT"
            | "DROP"
            | "ELSE"
            | "END"
            | "EXISTS"
            | "FALSE"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "IF"
            | "IN"
            | "INDEX"
            | "INNER"
            | "INSERT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "KEY"
            | "LEFT"
            | "LIKE"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "PRIMARY"
            | "REFERENCES"
            | "REPLACE"
            | "RESTRICT"
            | "RETURNING"
            | "RIGHT"
            | "SELECT"
            | "SET"
            | "TABLE"
            | "TEMP"
            | "TEMPORARY"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "UNIQUE"
            | "UPDATE"
            | "VALUES"
            | "VIEW"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
}

fn is_sql_type_name(upper: &str) -> bool {
    matches!(
        upper,
        "ARRAY"
            | "BIGINT"
            | "BIGSERIAL"
            | "BLOB"
            | "BOOL"
            | "BOOLEAN"
            | "BYTEA"
            | "CHAR"
            | "CHARACTER"
            | "DATE"
            | "DATETIME"
            | "DECIMAL"
            | "DOUBLE"
            | "FLOAT"
            | "INT"
            | "INTEGER"
            | "JSON"
            | "JSONB"
            | "NUMERIC"
            | "REAL"
            | "SERIAL"
            | "SMALLINT"
            | "STRUCT"
            | "TEXT"
            | "TIME"
            | "TIMESTAMP"
            | "TIMESTAMPTZ"
            | "UUID"
            | "VARCHAR"
    )
}

fn classify_markup_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "tag_name" => Some(HighlightKind::Keyword),
        "attribute_name" => Some(HighlightKind::Attribute),
        "attribute_value" | "quoted_attribute_value" | "AttValue" | "PseudoAttValue" => {
            Some(HighlightKind::String)
        }
        "Name" => classify_xml_name_node(node),
        _ => None,
    }
}

fn classify_xml_name_node(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        // XML tag names inside start/end/empty tags.
        "STag" | "ETag" | "EmptyElemTag" => Some(HighlightKind::Keyword),
        // XML attribute names.
        "Attribute" | "AttDef" | "PseudoAtt" => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn classify_css_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "tag_name"
        | "class_name"
        | "id_name"
        | "pseudo_class_selector"
        | "pseudo_element_selector" => Some(HighlightKind::Type),
        "property_name" | "feature_name" => Some(HighlightKind::Variable),
        "color_value" | "integer_value" | "float_value" => Some(HighlightKind::Number),
        "at_keyword" | "important" => Some(HighlightKind::Keyword),
        "function_name" => Some(HighlightKind::Function),
        _ => None,
    }
}

fn classify_bash_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "command_name" => Some(HighlightKind::Function),
        "variable_name" => Some(HighlightKind::Variable),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/syntax.rs"]
mod tests;
