use super::*;
use crate::models::{EditOp, OpId};
use ropey::Rope;

// ---------------------------------------------------------------------------
// Block classification tests
// ---------------------------------------------------------------------------

#[test]
fn classify_heading_levels() {
    let rope = Rope::from_str("# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::Heading(1));
    assert_eq!(md.block_kind(1), MdBlockKind::Heading(2));
    assert_eq!(md.block_kind(2), MdBlockKind::Heading(3));
    assert_eq!(md.block_kind(3), MdBlockKind::Heading(4));
    assert_eq!(md.block_kind(4), MdBlockKind::Heading(5));
    assert_eq!(md.block_kind(5), MdBlockKind::Heading(6));
}

#[test]
fn classify_code_fence() {
    let rope = Rope::from_str("text\n```rust\nlet x = 1;\n```\nmore text\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::Paragraph);
    assert_eq!(md.block_kind(1), MdBlockKind::CodeFence);
    assert_eq!(md.block_kind(2), MdBlockKind::CodeBlock);
    assert_eq!(md.block_kind(3), MdBlockKind::CodeFence);
    assert_eq!(md.block_kind(4), MdBlockKind::Paragraph);
}

#[test]
fn classify_lists() {
    let rope = Rope::from_str("- item1\n* item2\n+ item3\n1. ordered\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::UnorderedList);
    assert_eq!(md.block_kind(1), MdBlockKind::UnorderedList);
    assert_eq!(md.block_kind(2), MdBlockKind::UnorderedList);
    assert_eq!(md.block_kind(3), MdBlockKind::OrderedList);
}

#[test]
fn classify_blockquote() {
    let rope = Rope::from_str("> quoted text\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::BlockQuote);
}

#[test]
fn classify_horizontal_rule() {
    let rope = Rope::from_str("---\n***\n___\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::HorizontalRule);
    assert_eq!(md.block_kind(1), MdBlockKind::HorizontalRule);
    assert_eq!(md.block_kind(2), MdBlockKind::HorizontalRule);
}

#[test]
fn classify_blank_line() {
    let rope = Rope::from_str("text\n\nmore\n");
    let md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::Paragraph);
    assert_eq!(md.block_kind(1), MdBlockKind::Blank);
    assert_eq!(md.block_kind(2), MdBlockKind::Paragraph);
}

#[test]
fn classify_table_block() {
    let rope = Rope::from_str("| Name | Value |\n| :--- | ---: |\n| foo | 42 |\n| bar | 7 |\n");
    let md = MarkdownDocument::new(&rope);

    assert_eq!(md.block_kind(0), MdBlockKind::Table);
    assert_eq!(md.block_kind(1), MdBlockKind::Table);
    assert_eq!(md.block_kind(2), MdBlockKind::Table);
    assert_eq!(md.block_kind(3), MdBlockKind::Table);
}

// ---------------------------------------------------------------------------
// Inline rendering tests
// ---------------------------------------------------------------------------

#[test]
fn render_heading_strips_markers() {
    let rope = Rope::from_str("# Hello World\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);
    assert_eq!(rendered.text, "Hello World");
    assert!(!rendered.spans.is_empty());
    assert!(rendered.spans.iter().any(|span| span.start == 0
        && span.end == rendered.text.len()
        && span.kind == MdSpanKind::Heading(1)));
}

#[test]
fn render_unordered_list_replaces_marker() {
    let rope = Rope::from_str("- item text\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);
    assert!(rendered.text.starts_with("• "));
    assert!(rendered.text.contains("item text"));
}

#[test]
fn render_unordered_list_link_span_covers_full_label() {
    let rope = Rope::from_str("- [License](#license)\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "• License");
    let content_start = "• ".len();
    let content_end = content_start + "License".len();
    assert!(rendered.spans.iter().any(|span| {
        span.start == content_start && span.end == content_end && span.kind == MdSpanKind::Link
    }));
}

#[test]
fn render_unordered_list_bold_hides_markers() {
    let rope = Rope::from_str("- **LSP**\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "• LSP");
    let content_start = "• ".len();
    let content_end = content_start + "LSP".len();
    assert!(rendered.spans.iter().any(|span| {
        span.start == content_start && span.end == content_end && span.kind == MdSpanKind::Bold
    }));
}

#[test]
fn render_blockquote_replaces_marker() {
    let rope = Rope::from_str("> quoted\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);
    assert!(rendered.text.starts_with("│ "));
    assert!(rendered.text.contains("quoted"));
}

#[test]
fn render_ordered_list_bold_hides_markers() {
    let rope = Rope::from_str("1. **LSP**\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "1. LSP");
    let content_start = "1. ".len();
    let content_end = content_start + "LSP".len();
    assert!(rendered.spans.iter().any(|span| {
        span.start == content_start && span.end == content_end && span.kind == MdSpanKind::Bold
    }));
}

#[test]
fn render_blockquote_bold_hides_markers() {
    let rope = Rope::from_str("> **LSP**\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "│ LSP");
    let content_start = "│ ".len();
    let content_end = content_start + "LSP".len();
    assert!(rendered.spans.iter().any(|span| {
        span.start == content_start && span.end == content_end && span.kind == MdSpanKind::Bold
    }));
}

#[test]
fn render_hr_fills_width() {
    let rope = Rope::from_str("---\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 40);
    // Should be 40 repetitions of ─
    assert_eq!(rendered.text.chars().count(), 40);
    assert!(rendered.text.chars().all(|c| c == '─'));
}

#[test]
fn render_code_fence_uses_block_decoration() {
    let rope = Rope::from_str("```rust\nlet x = 1;\n```\n");
    let md = MarkdownDocument::new(&rope);

    let opening = md.render_line(0, &rope, 80);
    let closing = md.render_line(2, &rope, 80);

    assert!(opening.text.starts_with("─"));
    assert!(opening.text.contains("code: rust"));
    assert!(closing.text.chars().all(|c| c == '─'));
}

#[test]
fn render_code_block_uses_plain_content_without_bg() {
    let rope = Rope::from_str("```\nfn main() {}\n```\n");
    let md = MarkdownDocument::new(&rope);

    let rendered = md.render_line(1, &rope, 80);

    assert_eq!(rendered.text, "fn main() {}");
    assert!(rendered.spans.iter().any(|span| {
        span.start == 0 && span.end == rendered.text.len() && span.kind == MdSpanKind::Code
    }));
}

#[test]
fn render_table_header_and_separator() {
    let rope = Rope::from_str("| Name | Value |\n| :--- | ---: |\n| foo | 42 |\n");
    let md = MarkdownDocument::new(&rope);

    let header = md.render_line(0, &rope, 80);
    let separator = md.render_line(1, &rope, 80);
    let row = md.render_line(2, &rope, 80);

    assert!(header.text.contains(" │ "));
    assert!(header.text.contains("Name"));
    assert!(header.text.contains("Value"));
    assert!(separator.text.contains("┼"));
    assert!(separator.text.chars().all(|ch| matches!(ch, '─' | '┼')));
    assert!(row.text.contains("foo"));
    assert!(row.text.contains("42"));
}

#[test]
fn render_table_separator_aligns_with_header_dividers() {
    let rope =
        Rope::from_str("| Left | Center | Right |\n| :--- | :---: | ---: |\n| a | b | c |\n");
    let md = MarkdownDocument::new(&rope);

    let header = md.render_line(0, &rope, 100);
    let separator = md.render_line(1, &rope, 100);

    let header_dividers: Vec<usize> = header
        .text
        .chars()
        .enumerate()
        .filter_map(|(idx, ch)| (ch == '│').then_some(idx))
        .collect();
    let sep_dividers: Vec<usize> = separator
        .text
        .chars()
        .enumerate()
        .filter_map(|(idx, ch)| (ch == '┼').then_some(idx))
        .collect();

    assert_eq!(header_dividers, sep_dividers);
}

// ---------------------------------------------------------------------------
// Offset map tests
// ---------------------------------------------------------------------------

#[test]
fn offset_map_heading() {
    let rope = Rope::from_str("## Title\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);
    // Display "Title" maps to source bytes starting at 3 (after "## ")
    assert_eq!(rendered.text, "Title");
    assert!(!rendered.offset_map.is_empty());
    // First display byte 0 should map to source byte 3
    assert_eq!(rendered.offset_map[0], (0, 3));
}

#[test]
fn offset_map_bold_inline() {
    let rope = Rope::from_str("hello **world** end\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);
    // "hello world end" - ** markers hidden
    assert_eq!(rendered.text, "hello world end");
}

#[test]
fn offset_map_inline_code_end_maps_to_closing_marker() {
    let rope = Rope::from_str("`abc`\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "abc");
    assert_eq!(
        display_to_source_byte(&rendered.offset_map, rendered.text.len()),
        "`abc`".len()
    );
}

#[test]
fn inline_code_supports_multi_backtick_delimiter() {
    let rope = Rope::from_str("``code ` span``\n");
    let md = MarkdownDocument::new(&rope);
    let rendered = md.render_line(0, &rope, 80);

    assert_eq!(rendered.text, "code ` span");
    assert!(rendered.spans.iter().any(|span| {
        span.start == 0 && span.end == "code ` span".len() && span.kind == MdSpanKind::Code
    }));
}

#[test]
fn code_fence_requires_matching_delimiter_to_close() {
    let rope = Rope::from_str("```rust\nlet x = 1;\n~~~\nstill code\n```\n");
    let md = MarkdownDocument::new(&rope);

    assert_eq!(md.block_kind(0), MdBlockKind::CodeFence);
    assert_eq!(md.block_kind(1), MdBlockKind::CodeBlock);
    assert_eq!(md.block_kind(2), MdBlockKind::CodeBlock);
    assert_eq!(md.block_kind(3), MdBlockKind::CodeBlock);
    assert_eq!(md.block_kind(4), MdBlockKind::CodeFence);
}

// ---------------------------------------------------------------------------
// Reparse test
// ---------------------------------------------------------------------------

#[test]
fn reparse_updates_block_kinds() {
    let mut rope = Rope::from_str("# Heading\nparagraph\n");
    let mut md = MarkdownDocument::new(&rope);
    assert_eq!(md.block_kind(0), MdBlockKind::Heading(1));
    assert_eq!(md.block_kind(1), MdBlockKind::Paragraph);

    // Simulate editing: change paragraph to heading
    rope = Rope::from_str("# Heading\n## Sub\n");
    md.reparse(&rope, 1);
    assert_eq!(md.block_kind(0), MdBlockKind::Heading(1));
    assert_eq!(md.block_kind(1), MdBlockKind::Heading(2));
}

#[test]
fn apply_edit_updates_local_paragraph_classification() {
    let mut md = MarkdownDocument::new(&Rope::from_str("plain text\nnext\n"));

    let rope = Rope::from_str("# plain text\nnext\n");
    let op = EditOp::replace(
        OpId::root(),
        0,
        "plain text".chars().count(),
        "plain text".to_string(),
        "# plain text".to_string(),
        (0, 0),
        (0, 0),
    );

    md.apply_edit(&op, &rope, 1);

    assert_eq!(md.block_kind(0), MdBlockKind::Heading(1));
    assert_eq!(md.block_kind(1), MdBlockKind::Paragraph);
}

#[test]
fn apply_edit_falls_back_to_full_reparse_when_line_count_changes() {
    let mut md = MarkdownDocument::new(&Rope::from_str("a\nb\n"));

    let rope = Rope::from_str("a\nx\ny\nb\n");
    let op = EditOp::insert(OpId::root(), 2, "x\ny\n".to_string(), (1, 0), (1, 0));

    md.apply_edit(&op, &rope, 1);

    assert_eq!(md.block_kind(0), MdBlockKind::Paragraph);
    assert_eq!(md.block_kind(1), MdBlockKind::Paragraph);
    assert_eq!(md.block_kind(2), MdBlockKind::Paragraph);
    assert_eq!(md.block_kind(3), MdBlockKind::Paragraph);
}

// ---------------------------------------------------------------------------
// display_to_source_byte tests
// ---------------------------------------------------------------------------

#[test]
fn display_to_source_byte_identity() {
    let map: Vec<(usize, usize)> = (0..10).map(|i| (i, i)).collect();
    assert_eq!(display_to_source_byte(&map, 5), 5);
}

#[test]
fn display_to_source_byte_stripped_prefix() {
    // Display bytes 0..5 map to source bytes 3..8 (prefix stripped)
    let map: Vec<(usize, usize)> = (0..5).map(|i| (i, i + 3)).collect();
    assert_eq!(display_to_source_byte(&map, 0), 3);
    assert_eq!(display_to_source_byte(&map, 4), 7);
}

#[test]
fn display_to_source_byte_empty_map() {
    let map: Vec<(usize, usize)> = Vec::new();
    assert_eq!(display_to_source_byte(&map, 0), 0);
}
