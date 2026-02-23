use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::HighlightKind;

pub(super) fn classify_markup(
    node: Node<'_>,
    _rope: &Rope,
    _lang: LanguageId,
) -> Option<HighlightKind> {
    classify_markup_node(node)
}

pub(super) fn classify_css(
    node: Node<'_>,
    _rope: &Rope,
    _lang: LanguageId,
) -> Option<HighlightKind> {
    classify_css_node(node)
}

pub(super) fn classify_bash(
    node: Node<'_>,
    _rope: &Rope,
    _lang: LanguageId,
) -> Option<HighlightKind> {
    classify_bash_node(node)
}

pub(super) fn is_bash_keyword(kind: &str) -> bool {
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
