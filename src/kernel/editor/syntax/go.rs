use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::util::node_is_field;
use super::HighlightKind;

pub(super) fn classify(node: Node<'_>, _rope: &Rope, _lang: LanguageId) -> Option<HighlightKind> {
    classify_go_node(node)
}

pub(super) fn is_keyword(kind: &str) -> bool {
    is_go_keyword(kind)
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
