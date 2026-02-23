use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::util::{
    is_string_kind, node_contains, node_in_field_subtree, node_is_field, node_text_trimmed,
};
use super::HighlightKind;

pub(super) fn classify(node: Node<'_>, rope: &Rope, _lang: LanguageId) -> Option<HighlightKind> {
    if is_string_kind(node.kind()) {
        if let Some(kind) = classify_python_string(node, rope) {
            return Some(kind);
        }
    }
    classify_python_node(node, rope)
}

pub(super) fn is_keyword(kind: &str) -> bool {
    is_python_keyword(kind)
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
