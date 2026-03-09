use crate::kernel::language::LanguageId;
use ropey::Rope;
use tree_sitter::Node;

use super::util::{node_contains, node_in_field_subtree, node_is_field, same_node};
use super::{AbsHighlightSpan, HighlightKind};

pub(super) fn classify(
    node: Node<'_>,
    _rope: &Rope,
    language: LanguageId,
) -> Option<HighlightKind> {
    match node.kind() {
        "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "return" | "break"
        | "continue" | "goto" | "catch" | "throw" | "try" => Some(HighlightKind::KeywordControl),
        "identifier" => classify_identifier(node),
        "field_identifier" => classify_field_identifier(node),
        "namespace_identifier" if language == LanguageId::Cpp => Some(HighlightKind::Namespace),
        _ => None,
    }
}

pub(super) fn classify_preprocessor(kind: &str) -> Option<HighlightKind> {
    match kind {
        "preproc_call"
        | "preproc_def"
        | "preproc_directive"
        | "preproc_elif"
        | "preproc_elifdef"
        | "preproc_else"
        | "preproc_function_def"
        | "preproc_if"
        | "preproc_ifdef"
        | "preproc_include"
        | "preproc_defined" => Some(HighlightKind::Macro),
        _ => None,
    }
}

pub(super) fn collect_fallback_spans(
    rope: &Rope,
    start_byte: usize,
    end_byte: usize,
    existing: &[AbsHighlightSpan],
) -> Vec<AbsHighlightSpan> {
    if start_byte >= end_byte {
        return Vec::new();
    }

    let start_char = rope.byte_to_char(start_byte);
    let end_char = rope.byte_to_char(end_byte);
    let text = rope.slice(start_char..end_char).to_string();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut existing_idx = 0usize;
    let mut line_start = true;

    while i < bytes.len() {
        let abs = start_byte + i;
        while existing_idx < existing.len() && existing[existing_idx].end <= abs {
            existing_idx += 1;
        }
        if let Some(active) = existing
            .get(existing_idx)
            .filter(|span| span.start <= abs && abs < span.end)
        {
            i = active.end.saturating_sub(start_byte).min(bytes.len());
            line_start = i == 0 || bytes.get(i.saturating_sub(1)) == Some(&b'\n');
            continue;
        }

        if line_start {
            let mut scan = i;
            while scan < bytes.len() && matches!(bytes[scan], b' ' | b'\t') {
                scan += 1;
            }
            if scan < bytes.len() && bytes[scan] == b'#' {
                let mut end = scan + 1;
                let mut word = end;
                while word < bytes.len() && matches!(bytes[word], b' ' | b'\t') {
                    word += 1;
                }
                let mut word_end = word;
                while word_end < bytes.len()
                    && (bytes[word_end].is_ascii_alphabetic() || bytes[word_end] == b'_')
                {
                    word_end += 1;
                }
                if word_end > word {
                    end = word_end;
                }
                out.push(AbsHighlightSpan {
                    start: start_byte + scan,
                    end: start_byte + end,
                    kind: HighlightKind::Macro,
                    depth: 0,
                });
                i = end;
                line_start = false;
                continue;
            }
        }

        if is_ident_start(bytes[i]) && (i == 0 || !is_ident_continue(bytes[i - 1])) {
            let mut end = i + 1;
            while end < bytes.len() && is_ident_continue(bytes[end]) {
                end += 1;
            }
            if is_control_keyword(&bytes[i..end]) {
                out.push(AbsHighlightSpan {
                    start: start_byte + i,
                    end: start_byte + end,
                    kind: HighlightKind::KeywordControl,
                    depth: 0,
                });
                i = end;
                line_start = false;
                continue;
            }
            let mut lookahead = end;
            while lookahead < bytes.len() && matches!(bytes[lookahead], b' ' | b'\t') {
                lookahead += 1;
            }
            if bytes.get(lookahead) == Some(&b'[') {
                out.push(AbsHighlightSpan {
                    start: start_byte + i,
                    end: start_byte + end,
                    kind: HighlightKind::Variable,
                    depth: 0,
                });
                i = end;
                line_start = false;
                continue;
            }
        }

        line_start = bytes[i] == b'\n';
        i += 1;
    }

    out
}

pub(super) fn is_c_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "_Bool"
            | "_Complex"
            | "_Imaginary"
    )
}

pub(super) fn is_cpp_keyword(kind: &str) -> bool {
    is_c_keyword(kind)
        || matches!(
            kind,
            "alignas"
                | "alignof"
                | "and"
                | "and_eq"
                | "asm"
                | "bitand"
                | "bitor"
                | "bool"
                | "catch"
                | "class"
                | "compl"
                | "concept"
                | "constexpr"
                | "consteval"
                | "constinit"
                | "delete"
                | "dynamic_cast"
                | "explicit"
                | "export"
                | "false"
                | "friend"
                | "mutable"
                | "namespace"
                | "new"
                | "noexcept"
                | "not"
                | "not_eq"
                | "nullptr"
                | "operator"
                | "or"
                | "or_eq"
                | "private"
                | "protected"
                | "public"
                | "reinterpret_cast"
                | "requires"
                | "static_assert"
                | "template"
                | "this"
                | "thread_local"
                | "throw"
                | "true"
                | "try"
                | "typeid"
                | "typename"
                | "using"
                | "virtual"
                | "wchar_t"
                | "xor"
                | "xor_eq"
        )
}

pub(super) fn is_java_keyword(kind: &str) -> bool {
    matches!(
        kind,
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
            | "true"
            | "false"
            | "null"
            | "record"
            | "sealed"
            | "permits"
            | "var"
            | "yield"
    )
}

fn classify_identifier(node: Node<'_>) -> Option<HighlightKind> {
    classify_identifier_in_expression(node).or_else(|| classify_declarator_name(node))
}

fn classify_field_identifier(node: Node<'_>) -> Option<HighlightKind> {
    classify_field_identifier_in_expression(node)
        .or_else(|| classify_field_identifier_in_declaration(node))
}

fn classify_identifier_in_expression(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "call_expression" if node_is_field(parent, "function", node) => {
            Some(HighlightKind::Function)
        }
        "subscript_expression" if node_is_field(parent, "argument", node) => {
            Some(HighlightKind::Variable)
        }
        "template_function" if node_is_field(parent, "name", node) => {
            classify_name_container(parent)
        }
        "qualified_identifier" if is_qualified_identifier_scope(parent, node) => {
            Some(HighlightKind::Namespace)
        }
        "qualified_identifier" if is_qualified_identifier_name(parent, node) => {
            classify_name_container(parent)
        }
        _ => None,
    }
}

fn classify_field_identifier_in_expression(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "field_expression" if node_is_field(parent, "field", node) => {
            Some(classify_field_access(parent))
        }
        "qualified_identifier" if is_qualified_identifier_name(parent, node) => {
            classify_name_container(parent)
        }
        _ => None,
    }
}

fn classify_field_identifier_in_declaration(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    if parent.kind() == "field_declaration" {
        return Some(HighlightKind::Property);
    }
    None
}

fn classify_name_container(node: Node<'_>) -> Option<HighlightKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "call_expression" if node_is_field(parent, "function", node) => {
            Some(HighlightKind::Function)
        }
        "field_expression" if node_in_field_subtree(parent, "field", node) => {
            Some(classify_field_access(parent))
        }
        "qualified_identifier" if is_qualified_identifier_name(parent, node) => {
            classify_name_container(parent)
        }
        _ => None,
    }
}

fn classify_field_access(node: Node<'_>) -> HighlightKind {
    if node.parent().is_some_and(|parent| {
        parent.kind() == "call_expression" && node_is_field(parent, "function", node)
    }) {
        HighlightKind::Method
    } else {
        HighlightKind::Property
    }
}

fn classify_declarator_name(node: Node<'_>) -> Option<HighlightKind> {
    let mut current = node;
    let mut saw_function_declarator = false;

    loop {
        let parent = current.parent()?;
        match parent.kind() {
            "function_declarator" => {
                if !node_in_field_subtree(parent, "declarator", node) {
                    return None;
                }
                saw_function_declarator = true;
                current = parent;
            }
            "array_declarator"
            | "attributed_declarator"
            | "parenthesized_declarator"
            | "pointer_declarator"
            | "reference_declarator" => {
                if !nested_declarator_contains(parent, node) {
                    return None;
                }
                current = parent;
            }
            "init_declarator" => {
                if !node_in_field_subtree(parent, "declarator", node) {
                    return None;
                }
                current = parent;
            }
            "function_definition" if node_in_field_subtree(parent, "declarator", node) => {
                return Some(HighlightKind::Function);
            }
            "parameter_declaration" if node_in_field_subtree(parent, "declarator", node) => {
                return Some(HighlightKind::Parameter);
            }
            "field_declaration" if node_in_field_subtree(parent, "declarator", node) => {
                return Some(HighlightKind::Property);
            }
            "type_definition" if node_in_field_subtree(parent, "declarator", node) => {
                return Some(HighlightKind::Type);
            }
            "declaration" if node_in_field_subtree(parent, "declarator", node) => {
                return Some(if saw_function_declarator {
                    HighlightKind::Function
                } else {
                    HighlightKind::Variable
                });
            }
            _ => current = parent,
        }
    }
}

fn is_qualified_identifier_scope(parent: Node<'_>, node: Node<'_>) -> bool {
    parent
        .child_by_field_name("scope")
        .is_some_and(|scope| same_node(scope, node))
}

fn is_qualified_identifier_name(parent: Node<'_>, node: Node<'_>) -> bool {
    parent
        .named_child(parent.named_child_count().saturating_sub(1))
        .is_some_and(|name| same_node(name, node))
}

fn nested_declarator_contains(parent: Node<'_>, node: Node<'_>) -> bool {
    parent
        .child_by_field_name("declarator")
        .is_some_and(|declarator| node_contains(declarator, node))
        || parent
            .named_child(0)
            .is_some_and(|declarator| node_contains(declarator, node))
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_control_keyword(token: &[u8]) -> bool {
    matches!(
        token,
        b"if"
            | b"else"
            | b"for"
            | b"while"
            | b"do"
            | b"switch"
            | b"case"
            | b"return"
            | b"break"
            | b"continue"
            | b"goto"
            | b"catch"
            | b"throw"
            | b"try"
    )
}
