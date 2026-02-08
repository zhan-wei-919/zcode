use crate::core::Command;
use crate::kernel::editor::EditorTabState;
use crate::kernel::language::LanguageId;

/// Strategy trait that governs completion behavior per language.
///
/// All methods have default implementations extracted from the original
/// hard-coded logic. Language-specific strategies override only the
/// methods that need different behavior.
pub(crate) trait CompletionStrategy: Send + Sync {
    /// Rule 1: character triggers debounce (workbench layer).
    fn debounce_triggered_by_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == ':'
    }

    /// Rule 2: cursor context allows completion (workbench layer).
    fn context_allows_completion(&self, tab: &EditorTabState) -> bool {
        default_context_allows_completion(tab)
    }

    /// Rule 3: inserted char keeps completion popup open.
    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.'
    }

    /// Rule 4: extract prefix bounds at cursor.
    fn prefix_bounds(&self, tab: &EditorTabState) -> (usize, usize) {
        default_prefix_bounds(tab)
    }

    /// Rule 5: inserted char triggers LSP completion request.
    fn triggered_by_insert(&self, tab: &EditorTabState, ch: char, triggers: &[char]) -> bool {
        default_triggered_by_insert(tab, ch, triggers)
    }

    /// Rule 6a: char triggers signature help.
    fn signature_help_triggered(&self, ch: char, triggers: &[char]) -> bool {
        if triggers.is_empty() {
            matches!(ch, '(' | ',')
        } else {
            triggers.contains(&ch)
        }
    }

    /// Rule 6b: char closes signature help.
    fn signature_help_closed_by(&self, ch: char) -> bool {
        matches!(ch, ')')
    }

    /// Rule 6c: signature help should keep open.
    fn signature_help_should_keep_open(&self, tab: &EditorTabState) -> bool {
        default_signature_help_should_keep_open(tab)
    }

    /// Rule 7: command closes completion.
    fn should_close_on_command(&self, cmd: &Command, _tab: Option<&EditorTabState>) -> bool {
        default_should_close_on_command(self, cmd)
    }

    /// Composite: completion popup should keep open.
    fn completion_should_keep_open(&self, tab: &EditorTabState) -> bool {
        if tab.is_in_string_or_comment_at_cursor() {
            return false;
        }

        let (start_char, end_char) = self.prefix_bounds(tab);
        if start_char != end_char {
            return true;
        }

        let rope = tab.buffer.rope();
        if start_char > 0 && rope.char(start_char - 1) == '.' {
            return true;
        }
        if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
            return true;
        }

        false
    }
}

// ── Default free functions (extracted from original hard-coded logic) ──

fn default_context_allows_completion(tab: &EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let end_char = cursor_char_offset.min(rope.len_chars());

    let mut start_char = end_char;
    while start_char > 0 {
        let ch = rope.char(start_char - 1);
        if ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch) {
            start_char = start_char.saturating_sub(1);
        } else {
            break;
        }
    }

    if start_char != end_char {
        let first = rope.char(start_char);
        if first == '_' || unicode_xid::UnicodeXID::is_xid_start(first) {
            return true;
        }
    }

    if start_char > 0 && rope.char(start_char - 1) == '.' {
        let mut token_start = start_char.saturating_sub(1);
        while token_start > 0 {
            let ch = rope.char(token_start - 1);
            if ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch) {
                token_start = token_start.saturating_sub(1);
            } else {
                break;
            }
        }

        if token_start < start_char.saturating_sub(1) {
            let first = rope.char(token_start);
            if first == '_' || unicode_xid::UnicodeXID::is_xid_start(first) {
                return true;
            }
        }
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}

fn default_should_close_on_command<S: CompletionStrategy + ?Sized>(
    strategy: &S,
    cmd: &Command,
) -> bool {
    match cmd {
        Command::LspCompletion => false,
        Command::LspSemanticTokens | Command::LspInlayHints | Command::LspFoldingRange => false,
        Command::InsertChar(ch) => !strategy.keeps_open_on_char(*ch),
        Command::DeleteBackward | Command::DeleteForward | Command::DeleteSelection => false,
        _ => true,
    }
}

fn default_prefix_bounds(tab: &EditorTabState) -> (usize, usize) {
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let end_char = cursor_char_offset.min(rope.len_chars());

    let mut start_char = end_char;
    while start_char > 0 {
        let ch = rope.char(start_char - 1);
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start_char = start_char.saturating_sub(1);
        } else {
            break;
        }
    }

    (start_char, end_char)
}

fn default_triggered_by_insert(tab: &EditorTabState, inserted: char, triggers: &[char]) -> bool {
    if triggers.is_empty() {
        return match inserted {
            '.' => true,
            ':' => {
                let (row, col) = tab.buffer.cursor();
                let cursor_char_offset = tab.buffer.pos_to_char((row, col));
                let rope = tab.buffer.rope();
                let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
                if cursor_char_offset < 2 {
                    return false;
                }
                rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
            }
            _ => false,
        };
    }

    match inserted {
        ':' => {
            if !triggers.contains(&':') {
                return false;
            }
            let (row, col) = tab.buffer.cursor();
            let cursor_char_offset = tab.buffer.pos_to_char((row, col));
            let rope = tab.buffer.rope();
            let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
            if cursor_char_offset < 2 {
                return false;
            }
            rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
        }
        ch => triggers.contains(&ch),
    }
}

fn default_signature_help_should_keep_open(tab: &EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col)).min(rope.len_chars());
    let start = cursor_char_offset.saturating_sub(4096);

    let mut depth: usize = 0;
    let mut idx = cursor_char_offset;
    while idx > start {
        idx = idx.saturating_sub(1);
        let ch = rope.char(idx);
        if ch != '(' && ch != ')' {
            continue;
        }

        if tab.is_in_string_or_comment_at_char(idx) {
            continue;
        }

        match ch {
            ')' => depth = depth.saturating_add(1),
            '(' => {
                if depth == 0 {
                    return true;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    false
}

// ── Concrete strategies ──

pub(crate) struct DefaultCompletionStrategy;

impl CompletionStrategy for DefaultCompletionStrategy {}

pub(crate) struct CppCompletionStrategy;

impl CompletionStrategy for CppCompletionStrategy {
    fn debounce_triggered_by_char(&self, ch: char) -> bool {
        ch.is_alphanumeric()
            || ch == '_'
            || ch == '.'
            || ch == ':'
            || ch == '#'
            || ch == '<'
            || ch == '>'
            || ch == '/'
    }

    fn context_allows_completion(&self, tab: &EditorTabState) -> bool {
        if include_context_bounds(tab).is_some() {
            return true;
        }
        if is_include_directive_line(tab) {
            return false;
        }
        default_context_allows_completion(tab)
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '>' || ch == '/' || ch == '<'
    }

    fn prefix_bounds(&self, tab: &EditorTabState) -> (usize, usize) {
        if let Some(bounds) = include_context_bounds(tab) {
            return bounds;
        }
        default_prefix_bounds(tab)
    }

    fn triggered_by_insert(&self, tab: &EditorTabState, ch: char, triggers: &[char]) -> bool {
        if ch == '#' && is_hash_at_line_start(tab) {
            return true;
        }
        if ch == '>' && preceded_by_dash(tab) {
            return true;
        }
        if ch == '<' && include_context_bounds(tab).is_some() {
            return true;
        }
        if ch == '/' && include_context_bounds(tab).is_some() {
            return true;
        }
        default_triggered_by_insert(tab, ch, triggers)
    }

    fn should_close_on_command(&self, cmd: &Command, tab: Option<&EditorTabState>) -> bool {
        match cmd {
            Command::InsertChar('/') | Command::InsertChar('<') => {
                !tab.is_some_and(|t| include_context_bounds(t).is_some())
            }
            Command::InsertChar('>') => !tab.is_some_and(|t| {
                preceded_by_dash_for_insert(t) || include_context_bounds(t).is_some()
            }),
            _ => default_should_close_on_command(self, cmd),
        }
    }

    fn completion_should_keep_open(&self, tab: &EditorTabState) -> bool {
        if let Some((start_char, end_char)) = include_context_bounds(tab) {
            if start_char != end_char {
                return true;
            }
            let rope = tab.buffer.rope();
            if end_char > 0 {
                let prev = rope.char(end_char - 1);
                if prev == '<' || prev == '"' || prev == '/' {
                    return true;
                }
            }
            return false;
        }

        if is_include_directive_line(tab) {
            return false;
        }

        if tab.is_in_string_or_comment_at_cursor() {
            return false;
        }

        let (start_char, end_char) = self.prefix_bounds(tab);
        if start_char != end_char {
            return true;
        }

        let rope = tab.buffer.rope();
        if start_char > 0 && rope.char(start_char - 1) == '.' {
            return true;
        }
        if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
            return true;
        }
        if start_char >= 2 && rope.char(start_char - 1) == '>' && rope.char(start_char - 2) == '-' {
            return true;
        }

        false
    }
}

// ── C/C++ helper functions ──

fn include_context_bounds(tab: &EditorTabState) -> Option<(usize, usize)> {
    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col)).min(rope.len_chars());
    let line_start = rope.line_to_char(row);
    let line_to_cursor: String = rope.slice(line_start..cursor_char_offset).to_string();
    let chars: Vec<char> = line_to_cursor.chars().collect();

    let mut idx = include_keyword_end(&chars)?;
    while idx < chars.len() && (chars[idx] == ' ' || chars[idx] == '\t') {
        idx += 1;
    }
    if idx >= chars.len() {
        return None;
    }

    let closer = match chars[idx] {
        '<' => '>',
        '"' => '"',
        _ => return None,
    };

    if chars[idx + 1..].contains(&closer) {
        return None;
    }

    Some((line_start + idx + 1, cursor_char_offset))
}

fn is_include_directive_line(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col)).min(rope.len_chars());
    let line_start = rope.line_to_char(row);
    let line_to_cursor: String = rope.slice(line_start..cursor_char_offset).to_string();
    include_keyword_end(&line_to_cursor.chars().collect::<Vec<_>>()).is_some()
}

fn include_keyword_end(chars: &[char]) -> Option<usize> {
    let mut idx = 0;
    while idx < chars.len() && (chars[idx] == ' ' || chars[idx] == '\t') {
        idx += 1;
    }
    if chars.get(idx).copied() != Some('#') {
        return None;
    }
    idx += 1;

    while idx < chars.len() && (chars[idx] == ' ' || chars[idx] == '\t') {
        idx += 1;
    }

    for ch in ['i', 'n', 'c', 'l', 'u', 'd', 'e'] {
        if chars.get(idx).copied() != Some(ch) {
            return None;
        }
        idx += 1;
    }

    Some(idx)
}

fn is_hash_at_line_start(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
    if cursor_char_offset == 0 {
        return false;
    }
    if rope.char(cursor_char_offset - 1) != '#' {
        return false;
    }
    let line_start = rope.line_to_char(row);
    for i in line_start..cursor_char_offset.saturating_sub(1) {
        let ch = rope.char(i);
        if ch != ' ' && ch != '\t' {
            return false;
        }
    }
    true
}

fn preceded_by_dash(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
    if cursor_char_offset < 2 {
        return false;
    }
    rope.char(cursor_char_offset - 1) == '>' && rope.char(cursor_char_offset - 2) == '-'
}

fn preceded_by_dash_for_insert(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
    if cursor_char_offset == 0 {
        return false;
    }
    rope.char(cursor_char_offset - 1) == '-'
}

// ── Factory ──

static DEFAULT: DefaultCompletionStrategy = DefaultCompletionStrategy;
static CPP: CppCompletionStrategy = CppCompletionStrategy;

pub(crate) fn completion_strategy_for(
    language: Option<LanguageId>,
) -> &'static dyn CompletionStrategy {
    match language {
        Some(LanguageId::C | LanguageId::Cpp) => &CPP,
        _ => &DEFAULT,
    }
}

pub(crate) fn strategy_for_tab(tab: &EditorTabState) -> &'static dyn CompletionStrategy {
    completion_strategy_for(tab.language())
}
