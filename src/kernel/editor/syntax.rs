//! Syntax support (in-process): parsing + highlighting helpers.

use crate::models::EditOp;
use ropey::Rope;
use std::path::Path;
use tree_sitter::{InputEdit, Parser, Point, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageId {
    Rust,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Comment,
    String,
    Keyword,
    Type,
    Number,
    Attribute,
    Lifetime,
    Function,
    Macro,
    Variable,
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
}

pub struct SyntaxDocument {
    language: LanguageId,
    parser: Parser,
    tree: Tree,
}

impl SyntaxDocument {
    pub fn for_path(path: &Path, rope: &Rope) -> Option<Self> {
        match path.extension().and_then(|s| s.to_str()) {
            Some("rs") => Self::new(LanguageId::Rust, rope),
            Some("go") => Self::new(LanguageId::Go, rope),
            Some("py" | "pyi") => Self::new(LanguageId::Python, rope),
            Some("js" | "mjs" | "cjs" | "jsx") => Self::new(LanguageId::JavaScript, rope),
            Some("ts" | "mts" | "cts") => Self::new(LanguageId::TypeScript, rope),
            Some("tsx") => Self::new(LanguageId::Tsx, rope),
            _ => None,
        }
    }

    fn new(language: LanguageId, rope: &Rope) -> Option<Self> {
        let mut parser = Parser::new();
        match language {
            LanguageId::Rust => parser.set_language(tree_sitter_rust::language()).ok()?,
            LanguageId::Go => parser.set_language(tree_sitter_go::language()).ok()?,
            LanguageId::Python => parser.set_language(tree_sitter_python::language()).ok()?,
            LanguageId::JavaScript => parser
                .set_language(tree_sitter_javascript::language())
                .ok()?,
            LanguageId::TypeScript => parser
                .set_language(tree_sitter_typescript::language_typescript())
                .ok()?,
            LanguageId::Tsx => parser
                .set_language(tree_sitter_typescript::language_tsx())
                .ok()?,
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

    pub fn reparse(&mut self, rope: &Rope) {
        if let Some(tree) = parse_rope(&mut self.parser, rope, None) {
            self.tree = tree;
        }
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

    pub fn highlight_lines(
        &self,
        rope: &Rope,
        start_line: usize,
        end_line_exclusive: usize,
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

        let spans = collect_highlights(self.language, &self.tree, range_start, range_end);

        let mut per_line = vec![Vec::new(); end_line_exclusive - start_line];

        for span in spans {
            let span_start = span.start.max(range_start);
            let span_end = span.end.min(range_end);
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
            line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
            merge_adjacent_spans(line_spans);
        }

        per_line
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
        LanguageId::JavaScript => parser
            .set_language(tree_sitter_javascript::language())
            .is_ok(),
        LanguageId::TypeScript => parser
            .set_language(tree_sitter_typescript::language_typescript())
            .is_ok(),
        LanguageId::Tsx => parser
            .set_language(tree_sitter_typescript::language_tsx())
            .is_ok(),
    };
    if !language_set {
        return vec![Vec::new(); total_lines];
    }

    let Some(tree) = parse_rope(&mut parser, &rope, None) else {
        return vec![Vec::new(); total_lines];
    };

    let start_byte = 0;
    let end_byte = rope.len_bytes();
    let spans = collect_highlights(language, &tree, start_byte, end_byte);

    let mut per_line = vec![Vec::new(); total_lines];

    for span in spans {
        let span_start = span.start.min(end_byte);
        let span_end = span.end.min(end_byte);
        if span_start >= span_end {
            continue;
        }

        let first_line = rope.byte_to_line(span_start);
        let last_line = rope.byte_to_line(span_end.saturating_sub(1));

        let last_line = last_line.min(total_lines.saturating_sub(1));
        for (line, line_spans) in per_line
            .iter_mut()
            .enumerate()
            .take(last_line.saturating_add(1))
            .skip(first_line)
        {
            let line_start = rope.line_to_byte(line);
            let line_end = rope.line_to_byte((line + 1).min(total_lines));

            let s = span_start.max(line_start);
            let e = span_end.min(line_end);
            if s >= e {
                continue;
            }

            line_spans.push(HighlightSpan {
                start: s - line_start,
                end: e - line_start,
                kind: span.kind,
            });
        }
    }

    for line_spans in &mut per_line {
        line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_spans(line_spans);
    }

    per_line
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

fn collect_highlights(
    language: LanguageId,
    tree: &Tree,
    start_byte: usize,
    end_byte: usize,
) -> Vec<AbsHighlightSpan> {
    let root = tree.root_node();
    let mut stack = vec![root];
    let mut spans = Vec::new();

    while let Some(node) = stack.pop() {
        let node_start = node.start_byte();
        let node_end = node.end_byte();

        if node_end <= start_byte || node_start >= end_byte {
            continue;
        }

        if let Some(kind) = classify_node(language, node.kind()) {
            spans.push(AbsHighlightSpan {
                start: node_start,
                end: node_end,
                kind,
            });

            if matches!(
                kind,
                HighlightKind::Comment | HighlightKind::String | HighlightKind::Attribute
            ) {
                continue;
            }
        }

        let child_count = node.child_count();
        for i in (0..child_count).rev() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }

    spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    spans
}

fn classify_node(language: LanguageId, kind: &str) -> Option<HighlightKind> {
    if is_comment_kind(kind) {
        return Some(HighlightKind::Comment);
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
    if is_keyword(language, kind) {
        return Some(HighlightKind::Keyword);
    }
    None
}

fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || matches!(kind, "char_literal" | "byte_literal")
}

fn is_keyword(language: LanguageId, kind: &str) -> bool {
    match language {
        LanguageId::Rust => is_rust_keyword(kind),
        LanguageId::Go => is_go_keyword(kind),
        LanguageId::Python => is_python_keyword(kind),
        LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::Tsx => is_js_ts_keyword(kind),
    }
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

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/syntax.rs"]
mod tests;
