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
    let mut lines: Vec<Vec<HighlightSpan>> = vec![Vec::new(); total_lines];
    let mut needs_sort = vec![false; total_lines];
    let lookup = LegendLookup::new(legend);
    let rope_len_chars = rope.len_chars();
    let mut cached_line_index = usize::MAX;
    let mut cached_line_slice: Option<ropey::RopeSlice<'_>> = None;
    let mut cached_line_start_char = 0usize;
    let mut cached_line_start_byte = 0usize;

    for token in tokens {
        let line_index = token.line as usize;
        if line_index >= total_lines {
            continue;
        }

        let Some(kind) = lookup.kind_for(token) else {
            continue;
        };

        if cached_line_index != line_index {
            cached_line_index = line_index;
            cached_line_slice = Some(rope.line(line_index));
            cached_line_start_char = rope.line_to_char(line_index);
            cached_line_start_byte = rope.line_to_byte(line_index);
        }
        let Some(line_slice) = cached_line_slice else {
            continue;
        };

        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let start_char = (cached_line_start_char + start_chars).min(rope_len_chars);
        let end_char = (cached_line_start_char + end_chars).min(rope_len_chars);

        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(cached_line_start_byte);
        let end = end_byte.saturating_sub(cached_line_start_byte);
        if end <= start {
            continue;
        }

        let line = &mut lines[line_index];
        if let Some(prev) = line.last() {
            if start < prev.start || (start == prev.start && end < prev.end) {
                needs_sort[line_index] = true;
            }
        }
        line.push(HighlightSpan { start, end, kind });
    }

    for (idx, line_spans) in lines.iter_mut().enumerate() {
        if needs_sort[idx] {
            line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        }
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

    let mut lines: Vec<Vec<HighlightSpan>> =
        vec![Vec::new(); end_line_exclusive.saturating_sub(start_line)];
    let mut needs_sort = vec![false; lines.len()];
    let lookup = LegendLookup::new(legend);
    let rope_len_chars = rope.len_chars();
    let mut cached_line_index = usize::MAX;
    let mut cached_line_slice: Option<ropey::RopeSlice<'_>> = None;
    let mut cached_line_start_char = 0usize;
    let mut cached_line_start_byte = 0usize;

    for token in tokens {
        let Some(kind) = lookup.kind_for(token) else {
            continue;
        };

        let line_index = token.line as usize;
        if line_index < start_line || line_index >= end_line_exclusive {
            continue;
        }

        if cached_line_index != line_index {
            cached_line_index = line_index;
            cached_line_slice = Some(rope.line(line_index));
            cached_line_start_char = rope.line_to_char(line_index);
            cached_line_start_byte = rope.line_to_byte(line_index);
        }
        let Some(line_slice) = cached_line_slice else {
            continue;
        };

        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let start_char = (cached_line_start_char + start_chars).min(rope_len_chars);
        let end_char = (cached_line_start_char + end_chars).min(rope_len_chars);

        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(cached_line_start_byte);
        let end = end_byte.saturating_sub(cached_line_start_byte);
        if end <= start {
            continue;
        }

        let row_idx = line_index.saturating_sub(start_line);
        let line = &mut lines[row_idx];
        if let Some(prev) = line.last() {
            if start < prev.start || (start == prev.start && end < prev.end) {
                needs_sort[row_idx] = true;
            }
        }
        line.push(HighlightSpan { start, end, kind });
    }

    for (idx, line_spans) in lines.iter_mut().enumerate() {
        if needs_sort[idx] {
            line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        }
        merge_adjacent_highlight_spans(line_spans);
    }

    lines
}

fn map_semantic_token_type(token_type: &str) -> (Option<HighlightKind>, bool) {
    match token_type {
        "comment" => (Some(HighlightKind::Comment), false),
        "string" => (Some(HighlightKind::String), false),
        "regexp" => (Some(HighlightKind::Regex), false),
        "keyword" | "modifier" => (Some(HighlightKind::Keyword), false),
        // Operators like `==`, `:=`, `&&` are often returned by LSP semantic tokens.
        // Coloring them adds visual noise, so keep them unstyled.
        "operator" => (None, false),
        "number" => (Some(HighlightKind::Number), false),
        "type" | "struct" | "enum" | "interface" | "trait" | "typeParameter" | "class" => {
            (Some(HighlightKind::Type), false)
        }
        "function" => (Some(HighlightKind::Function), false),
        "method" => (Some(HighlightKind::Method), false),
        // Non-standard token type (not in the LSP spec): some servers use "member" for object
        // members without distinguishing fields vs methods. Keep it mapped to `Function` to
        // preserve legacy visuals; prefer "property"/"method" when provided.
        "member" => (Some(HighlightKind::Function), false),
        "macro" => (Some(HighlightKind::Macro), false),
        "enumMember" => (Some(HighlightKind::EnumMember), false),
        "variable" => (Some(HighlightKind::Variable), true),
        "parameter" => (Some(HighlightKind::Parameter), true),
        "property" => (Some(HighlightKind::Property), true),
        "event" => (Some(HighlightKind::Variable), true),
        "namespace" | "module" => (Some(HighlightKind::Namespace), false),
        "label" | "decorator" => (Some(HighlightKind::Attribute), false),
        _ => (None, false),
    }
}

#[derive(Debug)]
struct LegendLookup {
    kind_by_type_idx: Vec<Option<HighlightKind>>,
    readonly_sensitive: Vec<bool>,
    readonly_modifier_bit: Option<u32>,
}

impl LegendLookup {
    fn new(legend: &LspSemanticTokensLegend) -> Self {
        let mut kind_by_type_idx = Vec::with_capacity(legend.token_types.len());
        let mut readonly_sensitive = Vec::with_capacity(legend.token_types.len());

        for token_type in &legend.token_types {
            let (kind, sensitive) = map_semantic_token_type(token_type.as_str());
            kind_by_type_idx.push(kind);
            readonly_sensitive.push(sensitive);
        }

        let readonly_modifier_bit = legend
            .token_modifiers
            .iter()
            .position(|m| m == "readonly")
            .and_then(|idx| (1u32).checked_shl(idx as u32));

        Self {
            kind_by_type_idx,
            readonly_sensitive,
            readonly_modifier_bit,
        }
    }

    fn kind_for(&self, token: &LspSemanticToken) -> Option<HighlightKind> {
        let idx = token.token_type as usize;
        let mut kind = *self.kind_by_type_idx.get(idx)?;
        if self.readonly_sensitive.get(idx).copied().unwrap_or(false)
            && self
                .readonly_modifier_bit
                .is_some_and(|bit| token.modifiers & bit != 0)
        {
            kind = Some(HighlightKind::Constant);
        }
        kind
    }
}

#[cfg(test)]
fn highlight_kind_for_semantic_token(
    token_type: &str,
    modifiers: u32,
    legend: &LspSemanticTokensLegend,
) -> Option<HighlightKind> {
    let (mut kind, readonly_sensitive) = map_semantic_token_type(token_type);
    if readonly_sensitive && has_semantic_modifier(modifiers, legend, "readonly") {
        kind = Some(HighlightKind::Constant);
    }
    kind
}

#[cfg(test)]
fn has_semantic_modifier(modifiers: u32, legend: &LspSemanticTokensLegend, name: &str) -> bool {
    let Some(index) = legend.token_modifiers.iter().position(|m| m == name) else {
        return false;
    };
    let Some(mask) = (1u32).checked_shl(index as u32) else {
        return false;
    };
    modifiers & mask != 0
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
    fn semantic_token_method_maps_to_method() {
        assert_eq!(
            highlight_kind_for_semantic_token("method", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Method)
        );
    }

    #[test]
    fn semantic_token_parameter_maps_to_parameter() {
        assert_eq!(
            highlight_kind_for_semantic_token("parameter", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Parameter)
        );
    }

    #[test]
    fn semantic_token_property_maps_to_property() {
        assert_eq!(
            highlight_kind_for_semantic_token("property", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::Property)
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
    fn semantic_token_operator_is_unstyled() {
        assert_eq!(
            highlight_kind_for_semantic_token("operator", 0, &LspSemanticTokensLegend::default()),
            None
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
    fn semantic_token_enum_member_maps_to_enum_member() {
        assert_eq!(
            highlight_kind_for_semantic_token("enumMember", 0, &LspSemanticTokensLegend::default()),
            Some(HighlightKind::EnumMember)
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
