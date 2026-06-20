//! 括号配对查找：光标贴着某个括号时，定位它和它的配对括号，供渲染层高亮。
//!
//! 只配对可嵌套的 `()` `[]` `{}`；引号一类 open==close 的定界符不参与（无法靠深度
//! 计数配对）。尖括号 `<>` 故意不纳入——多数语言里它是比较运算符，配对会误判。

use super::state::EditorTabState;

/// 可嵌套括号对（开, 闭）。
const BRACKETS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// 单次配对扫描的字符上限。未配对的括号在大文件里会一路扫到头，设上限把热路径
/// 成本压成常数；超过即放弃高亮（屏幕外的配对本来也看不到）。
const MAX_SCAN: usize = 20_000;

impl EditorTabState {
    /// 光标贴着的括号与其配对括号的 `(row, grapheme_col)` 位置。
    ///
    /// 先看光标所在格的字符，不是括号再看左邻一格——这样「导航到括号上」和「刚敲完
    /// 闭括号、光标已移到其右侧」两种情形都能高亮。两者都不是括号则返回 `None`。
    pub fn matching_bracket(&self) -> Option<[(usize, usize); 2]> {
        let rope = self.buffer.rope();
        let len = rope.len_chars();
        let cursor = self.buffer.pos_to_char(self.buffer.cursor());

        for off in [Some(cursor), cursor.checked_sub(1)].into_iter().flatten() {
            if off >= len || self.bracket_in_string_or_comment(off) {
                continue;
            }
            let ch = rope.char(off);
            if let Some(other) = self.scan_matching(off, ch) {
                return Some([
                    self.buffer.cursor_pos_from_char_offset(off),
                    self.buffer.cursor_pos_from_char_offset(other),
                ]);
            }
        }
        None
    }

    /// 从 `off` 处的括号字符 `ch` 出发，深度计数找配对的另一半，返回其 char 偏移。
    /// 开括号向后扫、闭括号向前扫；跳过字符串/注释里的同类括号。
    fn scan_matching(&self, off: usize, ch: char) -> Option<usize> {
        let rope = self.buffer.rope();
        let len = rope.len_chars();

        if let Some(&(open, close)) = BRACKETS.iter().find(|(o, _)| *o == ch) {
            // 向后扫：起点即开括号，depth 从它开始记，归零处就是配对闭括号。
            let mut depth = 0usize;
            for (steps, i) in (off..len).enumerate() {
                if steps > MAX_SCAN {
                    return None;
                }
                let c = rope.char(i);
                if c != open && c != close || self.bracket_in_string_or_comment(i) {
                    continue;
                }
                if c == open {
                    depth += 1;
                } else if let Some(next) = depth.checked_sub(1) {
                    depth = next;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
            None
        } else if let Some(&(open, close)) = BRACKETS.iter().find(|(_, c)| *c == ch) {
            // 向前扫：起点即闭括号，归零处就是配对开括号。
            let mut depth = 0usize;
            for (steps, i) in (0..=off).rev().enumerate() {
                if steps > MAX_SCAN {
                    return None;
                }
                let c = rope.char(i);
                if c != open && c != close || self.bracket_in_string_or_comment(i) {
                    continue;
                }
                if c == close {
                    depth += 1;
                } else if let Some(next) = depth.checked_sub(1) {
                    depth = next;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
            None
        } else {
            None
        }
    }

    /// 该 char 偏移是否落在字符串/注释里（无语法树时一律否）。
    fn bracket_in_string_or_comment(&self, char_off: usize) -> bool {
        match self.syntax() {
            Some(syntax) => {
                let byte = self.buffer.rope().char_to_byte(char_off);
                syntax.is_in_string_or_comment(byte)
            }
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/bracket.rs"]
mod tests;
