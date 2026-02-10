use crate::kernel::editor::{HighlightKind, HighlightSpan};
use crate::kernel::services::ports::{
    LspPositionEncoding, LspSemanticToken, LspSemanticTokensLegend,
};

use super::lsp::lsp_col_to_char_offset_in_line;

pub(super) fn semantic_highlight_lines_from_tokens(
    rope: &ropey::Rope,
    tokens: &[LspSemanticToken],
    legend: &LspSemanticTokensLegend,
    encoding: LspPositionEncoding,
) -> Vec<Vec<HighlightSpan>> {
    let total_lines = rope.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];

    for token in tokens {
        let Some(token_type) = legend.token_types.get(token.token_type as usize) else {
            continue;
        };
        let Some(kind) =
            highlight_kind_for_semantic_token(token_type.as_str(), token.modifiers, legend)
        else {
            continue;
        };

        let line_index = token.line as usize;
        if line_index >= total_lines {
            continue;
        }

        let line_slice = rope.line(line_index);
        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let line_start_char = rope.line_to_char(line_index);
        let start_char = (line_start_char + start_chars).min(rope.len_chars());
        let end_char = (line_start_char + end_chars).min(rope.len_chars());

        let line_start_byte = rope.line_to_byte(line_index);
        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(line_start_byte);
        let end = end_byte.saturating_sub(line_start_byte);
        if end <= start {
            continue;
        }

        lines[line_index].push(HighlightSpan { start, end, kind });
    }

    for line_spans in &mut lines {
        line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(line_spans);
    }

    lines
}

pub(super) fn semantic_highlight_lines_from_tokens_range(
    rope: &ropey::Rope,
    tokens: &[LspSemanticToken],
    legend: &LspSemanticTokensLegend,
    encoding: LspPositionEncoding,
    start_line: usize,
    end_line_exclusive: usize,
) -> Vec<Vec<HighlightSpan>> {
    if start_line >= end_line_exclusive {
        return Vec::new();
    }

    let total_lines = rope.len_lines().max(1);
    let start_line = start_line.min(total_lines.saturating_sub(1));
    let end_line_exclusive = end_line_exclusive.min(total_lines);
    if end_line_exclusive <= start_line {
        return Vec::new();
    }

    let mut lines = vec![Vec::new(); end_line_exclusive.saturating_sub(start_line)];

    for token in tokens {
        let Some(token_type) = legend.token_types.get(token.token_type as usize) else {
            continue;
        };
        let Some(kind) =
            highlight_kind_for_semantic_token(token_type.as_str(), token.modifiers, legend)
        else {
            continue;
        };

        let line_index = token.line as usize;
        if line_index < start_line || line_index >= end_line_exclusive {
            continue;
        }

        let line_slice = rope.line(line_index);
        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let line_start_char = rope.line_to_char(line_index);
        let start_char = (line_start_char + start_chars).min(rope.len_chars());
        let end_char = (line_start_char + end_chars).min(rope.len_chars());

        let line_start_byte = rope.line_to_byte(line_index);
        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(line_start_byte);
        let end = end_byte.saturating_sub(line_start_byte);
        if end <= start {
            continue;
        }

        lines[line_index.saturating_sub(start_line)].push(HighlightSpan { start, end, kind });
    }

    for line_spans in &mut lines {
        line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(line_spans);
    }

    lines
}

fn highlight_kind_for_semantic_token(
    token_type: &str,
    modifiers: u32,
    legend: &LspSemanticTokensLegend,
) -> Option<HighlightKind> {
    match token_type {
        "comment" => Some(HighlightKind::Comment),
        "string" => Some(HighlightKind::String),
        "regexp" => Some(HighlightKind::Regex),
        "keyword" | "modifier" | "operator" => Some(HighlightKind::Keyword),
        "number" => Some(HighlightKind::Number),
        "type" | "struct" | "enum" | "interface" | "trait" | "typeParameter" | "class" => {
            Some(HighlightKind::Type)
        }
        "function" | "method" | "member" => Some(HighlightKind::Function),
        "macro" => Some(HighlightKind::Macro),
        "enumMember" => Some(HighlightKind::Constant),
        "variable" | "parameter" | "property" | "event" => {
            if has_semantic_modifier(modifiers, legend, "readonly") {
                Some(HighlightKind::Constant)
            } else {
                Some(HighlightKind::Variable)
            }
        }
        "namespace" | "module" => Some(HighlightKind::Namespace),
        "label" | "decorator" => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn has_semantic_modifier(modifiers: u32, legend: &LspSemanticTokensLegend, name: &str) -> bool {
    let Some(index) = legend.token_modifiers.iter().position(|m| m == name) else {
        return false;
    };
    modifiers & (1u32 << index) != 0
}

fn merge_adjacent_highlight_spans(spans: &mut Vec<HighlightSpan>) {
    if spans.len() < 2 {
        return;
    }

    let mut write = 1usize;
    for read in 1..spans.len() {
        let span = spans[read];
        let prev = &mut spans[write - 1];
        if prev.kind == span.kind && prev.end >= span.start {
            prev.end = prev.end.max(span.end);
        } else {
            spans[write] = span;
            write += 1;
        }
    }
    spans.truncate(write);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_token_class_maps_to_type() {
        assert_eq!(
            highlight_kind_for_semantic_token("class", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Type)
        );
    }

    #[test]
    fn semantic_token_member_maps_to_function() {
        assert_eq!(
            highlight_kind_for_semantic_token("member", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Function)
        );
    }

    #[test]
    fn semantic_token_label_maps_to_attribute() {
        assert_eq!(
            highlight_kind_for_semantic_token("label", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Attribute)
        );
    }

    #[test]
    fn semantic_token_operator_maps_to_keyword() {
        assert_eq!(
            highlight_kind_for_semantic_token("operator", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Keyword)
        );
    }

    #[test]
    fn semantic_token_regexp_maps_to_regex() {
        assert_eq!(
            highlight_kind_for_semantic_token("regexp", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Regex)
        );
    }

    #[test]
    fn semantic_token_enum_member_maps_to_constant() {
        assert_eq!(
            highlight_kind_for_semantic_token("enumMember", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Constant)
        );
    }

    #[test]
    fn semantic_token_readonly_variable_maps_to_constant() {
        let legend = LspSemanticTokensLegend {
            token_types: Vec::new(),
            token_modifiers: vec!["readonly".to_string()],
        };

        assert_eq!(
            highlight_kind_for_semantic_token("variable", 1, &legend),
            Some(HighlightKind::Constant)
        );
    }

    #[test]
    fn semantic_token_namespace_maps_to_namespace() {
        assert_eq!(
            highlight_kind_for_semantic_token("namespace", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Namespace)
        );
    }

    #[test]
    fn semantic_token_module_maps_to_namespace() {
        assert_eq!(
            highlight_kind_for_semantic_token("module", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Namespace)
        );
    }

    #[test]
    fn semantic_token_decorator_maps_to_attribute() {
        assert_eq!(
            highlight_kind_for_semantic_token("decorator", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Attribute)
        );
    }
}
