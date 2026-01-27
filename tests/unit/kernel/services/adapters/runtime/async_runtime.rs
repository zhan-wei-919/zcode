use super::*;
use crate::kernel::services::ports::{LspPosition, LspRange};
use tempfile::tempdir;

#[test]
fn apply_text_edits_to_path_rewrites_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.rs");
    std::fs::write(&path, "hello\nworld\n").unwrap();

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 5,
            },
        },
        new_text: "rust".to_string(),
    }];

    apply_text_edits_to_path(&path, &edits, LspPositionEncoding::Utf16).unwrap();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert_eq!(updated, "hello\nrust\n");
}

#[test]
fn apply_text_edits_to_rope_utf16_handles_emoji_columns() {
    let mut rope = Rope::from_str("a😀b\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 3,
            },
            end: LspPosition {
                line: 0,
                character: 4,
            },
        },
        new_text: "c".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "a😀c\n");
}

#[test]
fn apply_text_edits_to_rope_utf8_handles_emoji_columns() {
    let mut rope = Rope::from_str("a😀b\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 5,
            },
            end: LspPosition {
                line: 0,
                character: 6,
            },
        },
        new_text: "c".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf8);

    assert_eq!(rope.to_string(), "a😀c\n");
}

#[test]
fn apply_text_edits_to_rope_crlf_inserts_before_line_break() {
    let mut rope = Rope::from_str("a\r\nb\r\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 1,
            },
            end: LspPosition {
                line: 0,
                character: 1,
            },
        },
        new_text: "X".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "aX\r\nb\r\n");
}

#[test]
fn apply_text_edits_to_rope_sorts_edits_from_bottom_to_top() {
    let mut rope = Rope::from_str("abcdef\n");

    let edits = vec![
        LspTextEdit {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 0,
                },
                end: LspPosition {
                    line: 0,
                    character: 2,
                },
            },
            new_text: "Y".to_string(),
        },
        LspTextEdit {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 2,
                },
                end: LspPosition {
                    line: 0,
                    character: 4,
                },
            },
            new_text: "X".to_string(),
        },
    ];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "YXef\n");
}
