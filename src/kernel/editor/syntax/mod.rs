//! Syntax support (in-process): parsing + highlighting helpers.

mod c;
mod data;
mod go;
mod js;
mod markup;
mod python;
mod rust;
mod sql;
mod util;

use self::util::{is_comment_kind, is_regex_kind, is_string_kind};
use crate::kernel::language::LanguageId;
use crate::kernel::services::adapters::perf;
use crate::models::EditOp;
use ropey::Rope;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::Path;
use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SyntaxColorGroup {
    Comment = 0,
    String = 1,
    Regex = 2,
    Keyword = 3,
    KeywordControl = 4,
    Type = 5,
    Number = 6,
    Function = 7,
    Macro = 8,
    Namespace = 9,
    Variable = 10,
    Constant = 11,
    Attribute = 12,
    Operator = 13,
    Tag = 14,
}

impl SyntaxColorGroup {
    pub const COUNT: usize = 15;

    pub const CONFIGURABLE: [Self; 13] = [
        Self::Comment,
        Self::Keyword,
        Self::KeywordControl,
        Self::String,
        Self::Number,
        Self::Type,
        Self::Attribute,
        Self::Namespace,
        Self::Macro,
        Self::Function,
        Self::Variable,
        Self::Constant,
        Self::Regex,
    ];
}

pub const DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX: [u32; SyntaxColorGroup::CONFIGURABLE.len()] = [
    0x6A9955, // Comment
    0x569CD6, // Keyword
    0xC586C0, // KeywordControl
    0xCE9178, // String
    0xB5CEA8, // Number
    0x4EC9B0, // Type
    0x4EC9B0, // Attribute
    0x4EC9B0, // Namespace
    0x569CD6, // Macro
    0xDCDCAA, // Function
    0x9CDCFE, // Variable
    0x4FC1FF, // Constant
    0xD16969, // Regex
];

const _: () = assert!(SyntaxColorGroup::COUNT == 15);
const _: () = assert!(SyntaxColorGroup::Tag as usize == SyntaxColorGroup::COUNT - 1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HighlightKind {
    Comment = 0,
    String = 1,
    Regex = 2,
    Keyword = 3,
    KeywordControl = 4,
    KeywordOperator = 5,
    Type = 6,
    TypeBuiltin = 7,
    Number = 8,
    Boolean = 9,
    Function = 10,
    Method = 11,
    Macro = 12,
    Namespace = 13,
    Variable = 14,
    Parameter = 15,
    Property = 16,
    Constant = 17,
    EnumMember = 18,
    Attribute = 19,
    Lifetime = 20,
    Operator = 21,
    Tag = 22,
    TagAttribute = 23,
}

impl HighlightKind {
    pub const COUNT: usize = 24;

    /// Tree traversal should skip the node's children when a highlight kind is a "leaf".
    pub const fn is_leaf(self) -> bool {
        matches!(
            self,
            Self::Comment | Self::String | Self::Regex | Self::Attribute
        )
    }

    /// Rendering merge: avoid semantic token overrides when a highlight kind is "opaque".
    ///
    /// Tree-sitter is treated as authoritative for comments/strings/regex, which tend to be more
    /// reliable than LSP semantic tokens for these categories.
    pub const fn is_opaque(self) -> bool {
        matches!(self, Self::Comment | Self::String | Self::Regex)
    }

    pub const fn color_group(self) -> SyntaxColorGroup {
        match self {
            Self::Comment => SyntaxColorGroup::Comment,
            Self::String => SyntaxColorGroup::String,
            Self::Regex => SyntaxColorGroup::Regex,
            Self::Keyword => SyntaxColorGroup::Keyword,
            Self::KeywordControl => SyntaxColorGroup::KeywordControl,
            Self::KeywordOperator => SyntaxColorGroup::Keyword,
            Self::Type => SyntaxColorGroup::Type,
            Self::TypeBuiltin => SyntaxColorGroup::Type,
            Self::Number => SyntaxColorGroup::Number,
            Self::Boolean => SyntaxColorGroup::Keyword,
            Self::Attribute => SyntaxColorGroup::Attribute,
            Self::Lifetime => SyntaxColorGroup::Keyword,
            Self::Function => SyntaxColorGroup::Function,
            Self::Method => SyntaxColorGroup::Function,
            Self::Macro => SyntaxColorGroup::Macro,
            Self::Namespace => SyntaxColorGroup::Namespace,
            Self::Variable => SyntaxColorGroup::Variable,
            Self::Parameter => SyntaxColorGroup::Variable,
            Self::Property => SyntaxColorGroup::Variable,
            Self::Constant => SyntaxColorGroup::Constant,
            Self::EnumMember => SyntaxColorGroup::Constant,
            Self::Operator => SyntaxColorGroup::Operator,
            Self::Tag => SyntaxColorGroup::Tag,
            Self::TagAttribute => SyntaxColorGroup::Attribute,
        }
    }
}

const _: () = assert!(HighlightKind::COUNT == 24);
const _: () = assert!(HighlightKind::TagAttribute as usize == HighlightKind::COUNT - 1);

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
pub(crate) struct SyntaxEditDelta {
    pub(crate) input_edit: Option<InputEdit>,
    pub(crate) changed_ranges: Vec<tree_sitter::Range>,
    pub(crate) reparsed: bool,
}

#[derive(Debug, Clone)]
pub struct SyntaxHighlightPatch {
    pub start_line: usize,
    pub lines: Vec<Vec<HighlightSpan>>,
}

pub struct SyntaxDocument {
    language: LanguageId,
    parser: Parser,
    tree: Tree,
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
        })
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn reparse(&mut self, rope: &Rope) {
        if let Some(tree) = parse_rope(&mut self.parser, rope, None) {
            self.tree = tree;
        }
    }

    pub fn apply_edit(&mut self, rope: &Rope, op: &EditOp) -> SyntaxEditDelta {
        let Some(edit) = build_input_edit(rope, op) else {
            self.reparse(rope);
            return SyntaxEditDelta {
                input_edit: None,
                changed_ranges: Vec::new(),
                reparsed: true,
            };
        };

        self.tree.edit(&edit);

        let Some(new_tree) = parse_rope(&mut self.parser, rope, Some(&self.tree)) else {
            self.reparse(rope);
            return SyntaxEditDelta {
                input_edit: None,
                changed_ranges: Vec::new(),
                reparsed: true,
            };
        };

        let changed_ranges: Vec<_> = self.tree.changed_ranges(&new_tree).collect();
        self.tree = new_tree;

        SyntaxEditDelta {
            input_edit: Some(edit),
            changed_ranges,
            reparsed: false,
        }
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
}

pub(crate) fn compute_highlight_patches(
    language: LanguageId,
    tree: &Tree,
    rope: &Rope,
    segments: &[(usize, usize)],
) -> Vec<SyntaxHighlightPatch> {
    let _scope = perf::scope("syntax.highlight.compute");
    let total_lines = rope.len_lines().max(1);

    let mut patches = Vec::new();
    for (start_line, end_line_exclusive) in segments.iter().copied() {
        if start_line >= end_line_exclusive {
            continue;
        }

        let start_line = start_line.min(total_lines);
        let end_line_exclusive = end_line_exclusive.min(total_lines);
        if start_line >= end_line_exclusive {
            continue;
        }

        let range_start = rope.line_to_byte(start_line);
        let range_end = rope.line_to_byte(end_line_exclusive);
        let spans = collect_highlights(language, tree, rope, range_start, range_end);
        let per_line = project_abs_spans_to_lines(rope, start_line, end_line_exclusive, &spans);
        patches.push(SyntaxHighlightPatch {
            start_line,
            lines: per_line,
        });
    }
    patches
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
        crate::models::edit_op::OpKind::Batch { .. } => return None,
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

            if kind.is_leaf() {
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
        let supplemental = sql::collect_fallback_spans(rope, start_byte, end_byte, &normalized);
        if !supplemental.is_empty() {
            normalized.extend(supplemental);
            normalized = normalize_overlapping_highlight_spans(normalized, start_byte, end_byte);
        }
    }

    normalized
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
        if let Some(kind) = python::classify(node, rope, language) {
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
    if matches!(kind, "primitive_type" | "predefined_type") {
        return Some(HighlightKind::TypeBuiltin);
    }
    if kind == "type_identifier" {
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
            if let Some(kind) = rust::classify(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::Go => {
            if let Some(kind) = go::classify(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::Python => {
            if let Some(kind) = python::classify(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::C => {
            if let Some(kind) = c::classify(kind) {
                return Some(kind);
            }
        }
        LanguageId::Cpp | LanguageId::Java => {}
        LanguageId::Sql => {
            if let Some(kind) = sql::classify(kind) {
                return Some(kind);
            }
        }
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Jsx | LanguageId::Tsx => {
            if let Some(kind) = js::classify(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::Json | LanguageId::Yaml | LanguageId::Toml | LanguageId::Markdown => {}
        LanguageId::Html | LanguageId::Xml => {
            if let Some(kind) = markup::classify_markup(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::Css => {
            if let Some(kind) = markup::classify_css(node, rope, language) {
                return Some(kind);
            }
        }
        LanguageId::Bash => {
            if let Some(kind) = markup::classify_bash(node, rope, language) {
                return Some(kind);
            }
        }
    }
    if is_keyword(language, kind) {
        return Some(HighlightKind::Keyword);
    }
    None
}

fn is_keyword(language: LanguageId, kind: &str) -> bool {
    match language {
        LanguageId::Rust => rust::is_keyword(kind),
        LanguageId::Go => go::is_keyword(kind),
        LanguageId::Python => python::is_keyword(kind),
        LanguageId::C => c::is_c_keyword(kind),
        LanguageId::Cpp => c::is_cpp_keyword(kind),
        LanguageId::Java => c::is_java_keyword(kind),
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Jsx | LanguageId::Tsx => {
            js::is_keyword(kind)
        }
        LanguageId::Json => data::is_json_keyword(kind),
        LanguageId::Yaml => data::is_yaml_keyword(kind),
        LanguageId::Toml => data::is_toml_keyword(kind),
        LanguageId::Sql => sql::is_keyword(kind),
        LanguageId::Html | LanguageId::Xml => false,
        LanguageId::Css => false,
        LanguageId::Bash => markup::is_bash_keyword(kind),
        LanguageId::Markdown => false,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/editor/syntax.rs"]
mod tests;
