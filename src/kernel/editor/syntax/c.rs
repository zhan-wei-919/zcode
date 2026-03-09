use ropey::Rope;

use super::{AbsHighlightSpan, HighlightKind};

pub(super) fn classify(kind: &str) -> Option<HighlightKind> {
    match kind {
        "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "return" | "break"
        | "continue" | "goto" => Some(HighlightKind::KeywordControl),
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
