use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::util::node_is_field;
use super::HighlightKind;

pub(super) fn classify(node: Node<'_>, _rope: &Rope, _lang: LanguageId) -> Option<HighlightKind> {
    classify_rust_node(node)
}

pub(super) fn is_keyword(kind: &str) -> bool {
    is_rust_keyword(kind)
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
