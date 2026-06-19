use super::*;
use ropey::Rope;

// "a中😀b": chars a/中/😀/b = 4。utf8 字节 1/3/4/1，utf16 单元 1/1/2/1，utf32 各 1。

#[test]
fn line_len_chars_excludes_line_ending() {
    let lf = Rope::from_str("a中😀b\nrest");
    assert_eq!(line_len_chars(lf.line(0)), 4);

    let crlf = Rope::from_str("a中😀b\r\nrest");
    assert_eq!(line_len_chars(crlf.line(0)), 4);

    let no_newline = Rope::from_str("abc");
    assert_eq!(line_len_chars(no_newline.line(0)), 3);

    let empty_first_line = Rope::from_str("\nx");
    assert_eq!(line_len_chars(empty_first_line.line(0)), 0);
}

#[test]
fn col_to_char_offset_counts_encoding_units() {
    let rope = Rope::from_str("a中😀b\n");
    let line = rope.line(0);

    // 前 3 字符 (a 中 😀) = utf8 8 / utf16 4 / utf32 3 个单元。
    assert_eq!(
        lsp_col_to_char_offset_in_line(line, 8, LspPositionEncoding::Utf8),
        3
    );
    assert_eq!(
        lsp_col_to_char_offset_in_line(line, 4, LspPositionEncoding::Utf16),
        3
    );
    assert_eq!(
        lsp_col_to_char_offset_in_line(line, 3, LspPositionEncoding::Utf32),
        3
    );

    // 越界列钳到行内 4 字符。
    assert_eq!(
        lsp_col_to_char_offset_in_line(line, 999, LspPositionEncoding::Utf16),
        4
    );

    // 落在字符中间的列不跨过该字符：utf8 col 7 = a(1)+中(3)=4，😀 需再 4 → 8>7，停在 2。
    assert_eq!(
        lsp_col_to_char_offset_in_line(line, 7, LspPositionEncoding::Utf8),
        2
    );
}

#[test]
fn column_for_chars_matches_encoding_units() {
    let rope = Rope::from_str("a中😀b\n");
    let line = rope.line(0);

    assert_eq!(column_for_chars(line, 3, LspPositionEncoding::Utf8), 8);
    assert_eq!(column_for_chars(line, 3, LspPositionEncoding::Utf16), 4);
    assert_eq!(column_for_chars(line, 3, LspPositionEncoding::Utf32), 3);
    assert_eq!(column_for_chars(line, 0, LspPositionEncoding::Utf16), 0);
}

#[test]
fn col_and_char_offset_roundtrip() {
    let rope = Rope::from_str("fn 名前() {}\n");
    let line = rope.line(0);
    let len = line_len_chars(line);
    for encoding in [
        LspPositionEncoding::Utf8,
        LspPositionEncoding::Utf16,
        LspPositionEncoding::Utf32,
    ] {
        for chars in 0..=len {
            let col = column_for_chars(line, chars, encoding);
            assert_eq!(
                lsp_col_to_char_offset_in_line(line, col, encoding),
                chars,
                "roundtrip {:?} chars={}",
                encoding,
                chars
            );
        }
    }
}
