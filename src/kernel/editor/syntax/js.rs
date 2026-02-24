use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::util::node_is_field;
use super::HighlightKind;

pub(super) fn classify(node: Node<'_>, _rope: &Rope, _lang: LanguageId) -> Option<HighlightKind> {
    classify_js_node(node)
}

pub(super) fn is_keyword(kind: &str) -> bool {
    is_js_ts_keyword(kind)
}

fn classify_js_node(node: Node<'_>) -> Option<HighlightKind> {
    match node.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => classify_js_identifier(node),
        "property_identifier" => classify_js_property_identifier(node),
        "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "return" | "break"
        | "continue" | "throw" | "try" | "catch" | "finally" | "await" => {
            Some(HighlightKind::KeywordControl)
        }
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
        "formal_parameters" => Some(HighlightKind::Parameter),
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
            return Some(HighlightKind::Method);
        }
        return Some(HighlightKind::Property);
    }
    None
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
