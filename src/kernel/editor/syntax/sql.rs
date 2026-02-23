use ropey::Rope;

use super::{AbsHighlightSpan, HighlightKind};

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
            continue;
        }

        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            let token_start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i,
                kind: HighlightKind::Comment,
                depth: 0,
            });
            continue;
        }

        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let token_start = i;
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i.min(bytes.len()),
                kind: HighlightKind::Comment,
                depth: 0,
            });
            continue;
        }

        if b == b'\'' {
            let token_start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i.min(bytes.len()),
                kind: HighlightKind::String,
                depth: 0,
            });
            continue;
        }

        if b.is_ascii_digit() {
            let token_start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(AbsHighlightSpan {
                start: start_byte + token_start,
                end: start_byte + i,
                kind: HighlightKind::Number,
                depth: 0,
            });
            continue;
        }

        if is_sql_identifier_start(b) {
            let token_start = i;
            i += 1;
            while i < bytes.len() && is_sql_identifier_continue(bytes[i]) {
                i += 1;
            }
            let token = &text[token_start..i];
            if let Some(kind) = classify_sql_word(token) {
                out.push(AbsHighlightSpan {
                    start: start_byte + token_start,
                    end: start_byte + i,
                    kind,
                    depth: 0,
                });
            }
            continue;
        }

        i += 1;
    }

    out
}

pub(super) fn is_keyword(kind: &str) -> bool {
    is_sql_keyword(kind)
}

fn is_sql_identifier_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_sql_identifier_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn classify_sql_word(word: &str) -> Option<HighlightKind> {
    let upper = word.to_ascii_uppercase();
    if is_sql_type_name(upper.as_str()) {
        return Some(HighlightKind::Type);
    }
    if is_sql_reserved_word(upper.as_str()) {
        return Some(HighlightKind::Keyword);
    }
    None
}

fn is_sql_keyword(kind: &str) -> bool {
    if kind.starts_with("keyword_") {
        return true;
    }
    if matches!(kind, "ERROR" | "MISSING") {
        return false;
    }
    if is_sql_reserved_word(kind) {
        return true;
    }
    if is_sql_type_name(kind) {
        return true;
    }

    let mut has_alpha = false;
    for b in kind.bytes() {
        if b.is_ascii_alphabetic() {
            has_alpha = true;
            if !b.is_ascii_uppercase() {
                return false;
            }
            continue;
        }
        if b != b'_' {
            return false;
        }
    }
    has_alpha
}

fn is_sql_reserved_word(upper: &str) -> bool {
    matches!(
        upper,
        "ALL"
            | "ALTER"
            | "AND"
            | "AS"
            | "ASC"
            | "BY"
            | "CASE"
            | "CASCADE"
            | "CHECK"
            | "CONSTRAINT"
            | "CREATE"
            | "CROSS"
            | "CURRENT_DATE"
            | "CURRENT_TIME"
            | "CURRENT_TIMESTAMP"
            | "DEFAULT"
            | "DELETE"
            | "DESC"
            | "DISTINCT"
            | "DROP"
            | "ELSE"
            | "END"
            | "EXISTS"
            | "FALSE"
            | "FROM"
            | "FULL"
            | "GROUP"
            | "HAVING"
            | "IF"
            | "IN"
            | "INDEX"
            | "INNER"
            | "INSERT"
            | "INTO"
            | "IS"
            | "JOIN"
            | "KEY"
            | "LEFT"
            | "LIKE"
            | "LIMIT"
            | "NOT"
            | "NULL"
            | "OFFSET"
            | "ON"
            | "OR"
            | "ORDER"
            | "OUTER"
            | "PRIMARY"
            | "REFERENCES"
            | "REPLACE"
            | "RESTRICT"
            | "RETURNING"
            | "RIGHT"
            | "SELECT"
            | "SET"
            | "TABLE"
            | "TEMP"
            | "TEMPORARY"
            | "THEN"
            | "TRUE"
            | "UNION"
            | "UNIQUE"
            | "UPDATE"
            | "VALUES"
            | "VIEW"
            | "WHEN"
            | "WHERE"
            | "WITH"
    )
}

fn is_sql_type_name(upper: &str) -> bool {
    matches!(
        upper,
        "ARRAY"
            | "BIGINT"
            | "BIGSERIAL"
            | "BLOB"
            | "BOOL"
            | "BOOLEAN"
            | "BYTEA"
            | "CHAR"
            | "CHARACTER"
            | "DATE"
            | "DATETIME"
            | "DECIMAL"
            | "DOUBLE"
            | "FLOAT"
            | "INT"
            | "INTEGER"
            | "JSON"
            | "JSONB"
            | "NUMERIC"
            | "REAL"
            | "SERIAL"
            | "SMALLINT"
            | "STRUCT"
            | "TEXT"
            | "TIME"
            | "TIMESTAMP"
            | "TIMESTAMPTZ"
            | "UUID"
            | "VARCHAR"
    )
}
