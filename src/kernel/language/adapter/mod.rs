mod c_family;
mod default;
mod go;
mod js;
mod python;
mod rust;
mod syntax_bridge;

use std::path::Path;

use crate::core::Command;
use crate::kernel::editor::EditorTabState;
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::{LspCompletionItem, LspInsertTextFormat, LspServerKind};

#[cfg(test)]
pub(crate) use c_family::{include_context_perf_counter, reset_include_context_perf_counter};
use c_family::{CPP_ADAPTER, C_ADAPTER};
use default::{
    BASH_ADAPTER, CSS_ADAPTER, DEFAULT_ADAPTER, HTML_ADAPTER, JAVA_ADAPTER, JSON_ADAPTER,
    MARKDOWN_ADAPTER, SQL_ADAPTER, TOML_ADAPTER, XML_ADAPTER, YAML_ADAPTER,
};
use go::GO_ADAPTER;
use js::{JSX_ADAPTER, JS_ADAPTER, TSX_ADAPTER, TS_ADAPTER};
use python::PYTHON_ADAPTER;
use rust::RUST_ADAPTER;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LanguageFeatures {
    pub lsp_server: Option<LspServerKind>,
    pub has_syntax: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberAccessKind {
    Dot,
    Scope,
    Arrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeDelimiter {
    Angle,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IncludeContext {
    pub bounds: Option<(usize, usize)>,
    pub delimiter: Option<IncludeDelimiter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineContext {
    #[default]
    None,
    Directive,
    Import,
    Include(IncludeContext),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyntaxFacts {
    pub in_string: bool,
    pub in_comment: bool,
    pub identifier_bounds: Option<(usize, usize)>,
    pub member_access_kind: Option<MemberAccessKind>,
    pub line_context: LineContext,
}

impl SyntaxFacts {
    pub fn in_string_or_comment(&self) -> bool {
        self.in_string || self.in_comment
    }
}

#[derive(Debug, Clone)]
pub struct LanguageBehaviorContext<'a> {
    pub language: Option<LanguageId>,
    pub tab: &'a EditorTabState,
    pub syntax: SyntaxFacts,
}

#[derive(Debug, Clone)]
pub struct CompletionContext<'a> {
    pub behavior: LanguageBehaviorContext<'a>,
    pub item: &'a LspCompletionItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionFallbackStrategy {
    NativeSnippet,
    CallableFallback,
    CursorOnly,
    PlainText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTabstop {
    pub index: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionFallbackPlan {
    pub text: String,
    pub cursor: Option<usize>,
    pub selection: Option<(usize, usize)>,
    pub tabstops: Vec<CompletionTabstop>,
    pub strategy: CompletionFallbackStrategy,
}

impl CompletionFallbackPlan {
    pub fn from_plain_text(text: String) -> Self {
        let cursor = text
            .strip_suffix("()")
            .map(|prefix| prefix.chars().count().saturating_add(1));
        let strategy = if cursor.is_some() {
            CompletionFallbackStrategy::CursorOnly
        } else {
            CompletionFallbackStrategy::PlainText
        };
        Self {
            text,
            cursor,
            selection: None,
            tabstops: Vec::new(),
            strategy,
        }
    }

    pub fn from_snippet(snippet: &str) -> Self {
        let expanded = expand_snippet(snippet);
        Self {
            text: expanded.text,
            cursor: expanded.cursor,
            selection: expanded.selection,
            tabstops: expanded.tabstops,
            strategy: CompletionFallbackStrategy::NativeSnippet,
        }
    }

    pub fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

pub trait SyntaxBehavior: Send + Sync {
    fn syntax_facts(&self, tab: &EditorTabState) -> SyntaxFacts;
}

pub trait CompletionBehavior: Send + Sync {
    fn debounce_triggered_by_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == ':'
    }

    fn context_allows_completion(&self, tab: &EditorTabState) -> bool {
        default_context_allows_completion(tab)
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.'
    }

    fn prefix_bounds(&self, tab: &EditorTabState) -> (usize, usize) {
        default_prefix_bounds(tab)
    }

    fn triggered_by_insert(&self, tab: &EditorTabState, ch: char, triggers: &[char]) -> bool {
        default_triggered_by_insert(tab, ch, triggers)
    }

    fn signature_help_triggered(&self, ch: char, triggers: &[char]) -> bool {
        if triggers.is_empty() {
            matches!(ch, '(' | ',')
        } else {
            triggers.contains(&ch)
        }
    }

    fn signature_help_closed_by(&self, ch: char) -> bool {
        matches!(ch, ')')
    }

    fn signature_help_should_keep_open(&self, tab: &EditorTabState) -> bool {
        default_signature_help_should_keep_open(tab)
    }

    fn should_close_on_command(&self, cmd: &Command, _tab: Option<&EditorTabState>) -> bool {
        default_should_close_on_command(self, cmd)
    }

    fn completion_should_keep_open(&self, tab: &EditorTabState) -> bool {
        default_completion_should_keep_open(tab)
    }

    fn normalize_completion_item(&self, context: &CompletionContext<'_>) -> CompletionFallbackPlan {
        default_normalize_completion_item(context)
    }
}

pub trait LanguageAdapter: Send + Sync {
    fn completion(&self) -> &dyn CompletionBehavior;
    fn syntax(&self) -> &dyn SyntaxBehavior;
    fn features(&self) -> LanguageFeatures;
}

pub fn adapter_for(language: Option<LanguageId>) -> &'static dyn LanguageAdapter {
    match language {
        Some(LanguageId::Rust) => &RUST_ADAPTER,
        Some(LanguageId::Go) => &GO_ADAPTER,
        Some(LanguageId::Python) => &PYTHON_ADAPTER,
        Some(LanguageId::JavaScript) => &JS_ADAPTER,
        Some(LanguageId::TypeScript) => &TS_ADAPTER,
        Some(LanguageId::Jsx) => &JSX_ADAPTER,
        Some(LanguageId::Tsx) => &TSX_ADAPTER,
        Some(LanguageId::C) => &C_ADAPTER,
        Some(LanguageId::Cpp) => &CPP_ADAPTER,
        Some(LanguageId::Java) => &JAVA_ADAPTER,
        Some(LanguageId::Json) => &JSON_ADAPTER,
        Some(LanguageId::Yaml) => &YAML_ADAPTER,
        Some(LanguageId::Html) => &HTML_ADAPTER,
        Some(LanguageId::Xml) => &XML_ADAPTER,
        Some(LanguageId::Css) => &CSS_ADAPTER,
        Some(LanguageId::Toml) => &TOML_ADAPTER,
        Some(LanguageId::Sql) => &SQL_ADAPTER,
        Some(LanguageId::Bash) => &BASH_ADAPTER,
        Some(LanguageId::Markdown) => &MARKDOWN_ADAPTER,
        None => &DEFAULT_ADAPTER,
    }
}

pub fn adapter_for_path(path: &Path) -> &'static dyn LanguageAdapter {
    adapter_for(LanguageId::from_path(path))
}

pub(crate) fn adapter_for_tab(tab: &EditorTabState) -> &'static dyn LanguageAdapter {
    adapter_for(tab.language())
}

pub(crate) fn language_features(language: Option<LanguageId>) -> LanguageFeatures {
    LanguageFeatures {
        lsp_server: language.and_then(LanguageId::server_kind),
        has_syntax: language.is_some_and(|lang| lang != LanguageId::Markdown),
    }
}

pub(crate) fn cursor_char_offset(tab: &EditorTabState) -> usize {
    let (row, col) = tab.buffer.cursor();
    let rope = tab.buffer.rope();
    tab.buffer.pos_to_char((row, col)).min(rope.len_chars())
}

pub(crate) fn default_context_allows_completion(tab: &EditorTabState) -> bool {
    let syntax = syntax_bridge::syntax_facts_for_tab(tab);
    if syntax.in_string_or_comment() {
        return false;
    }

    if syntax.identifier_bounds.is_some() {
        return true;
    }

    matches!(
        syntax.member_access_kind,
        Some(MemberAccessKind::Dot | MemberAccessKind::Scope)
    )
}

pub(crate) fn default_should_close_on_command<S: CompletionBehavior + ?Sized>(
    behavior: &S,
    cmd: &Command,
) -> bool {
    match cmd {
        Command::LspCompletion => false,
        Command::LspSemanticTokens | Command::LspInlayHints | Command::LspFoldingRange => false,
        Command::InsertChar(ch) => !behavior.keeps_open_on_char(*ch),
        Command::DeleteBackward | Command::DeleteForward | Command::DeleteSelection => false,
        _ => true,
    }
}

pub(crate) fn default_prefix_bounds(tab: &EditorTabState) -> (usize, usize) {
    let cursor = cursor_char_offset(tab);
    let syntax = syntax_bridge::syntax_facts_for_tab(tab);
    syntax
        .identifier_bounds
        .map(|(start, _end)| (start, cursor))
        .unwrap_or((cursor, cursor))
}

pub(crate) fn default_triggered_by_insert(
    tab: &EditorTabState,
    inserted: char,
    triggers: &[char],
) -> bool {
    let syntax = syntax_bridge::syntax_facts_for_tab(tab);
    if triggers.is_empty() {
        return match inserted {
            '.' => true,
            ':' => syntax.member_access_kind == Some(MemberAccessKind::Scope),
            _ => false,
        };
    }

    match inserted {
        ':' => {
            triggers.contains(&':') && syntax.member_access_kind == Some(MemberAccessKind::Scope)
        }
        ch => triggers.contains(&ch),
    }
}

pub(crate) fn default_signature_help_should_keep_open(tab: &EditorTabState) -> bool {
    if syntax_bridge::syntax_facts_for_tab(tab).in_string_or_comment() {
        return false;
    }

    let rope = tab.buffer.rope();
    let cursor = cursor_char_offset(tab);
    let start = cursor.saturating_sub(4096);

    let mut depth: usize = 0;
    let mut idx = cursor;
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

pub(crate) fn default_completion_should_keep_open(tab: &EditorTabState) -> bool {
    let syntax = syntax_bridge::syntax_facts_for_tab(tab);
    if syntax.in_string_or_comment() {
        return false;
    }

    if syntax.identifier_bounds.is_some() {
        return true;
    }

    matches!(
        syntax.member_access_kind,
        Some(MemberAccessKind::Dot | MemberAccessKind::Scope)
    )
}

pub(crate) fn default_normalize_completion_item(
    context: &CompletionContext<'_>,
) -> CompletionFallbackPlan {
    let mut plan = match context.item.insert_text_format {
        LspInsertTextFormat::PlainText => {
            let mut plan =
                CompletionFallbackPlan::from_plain_text(context.item.insert_text.clone());
            if !plan.has_cursor_or_selection() && should_append_callable_parentheses(context.item) {
                plan.text.push('(');
                plan.text.push(')');
                plan.cursor = Some(plan.text.chars().count().saturating_sub(1));
                plan.strategy = CompletionFallbackStrategy::CallableFallback;
            }
            plan
        }
        LspInsertTextFormat::Snippet => {
            CompletionFallbackPlan::from_snippet(&context.item.insert_text)
        }
    };

    if !plan.has_cursor_or_selection()
        && should_append_trailing_space(context.item)
        && !plan.text.ends_with(' ')
    {
        plan.text.push(' ');
    }

    plan
}

pub(crate) fn should_append_trailing_space(item: &LspCompletionItem) -> bool {
    if !matches!(item.kind, Some(14)) {
        return false;
    }

    let text = item.insert_text.as_str();
    if text.is_empty() || text.ends_with(' ') {
        return false;
    }

    text.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
}

fn should_append_callable_parentheses(item: &LspCompletionItem) -> bool {
    if !completion_kind_is_callable(item.kind) {
        return false;
    }

    let text = item.insert_text.as_str();
    if text.is_empty()
        || text.contains('(')
        || text.contains('!')
        || text.chars().any(|ch| ch.is_whitespace())
    {
        return false;
    }

    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || unicode_xid::UnicodeXID::is_xid_start(first)) {
        return false;
    }

    chars.all(|ch| ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch))
}

pub(crate) fn completion_kind_is_callable(kind: Option<u32>) -> bool {
    matches!(kind, Some(2..=4))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnippetExpansion {
    pub(crate) text: String,
    pub(crate) cursor: Option<usize>,
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) tabstops: Vec<CompletionTabstop>,
}

pub(crate) fn expand_snippet(snippet: &str) -> SnippetExpansion {
    let mut out = String::with_capacity(snippet.len());
    let mut out_chars = 0usize;
    let cursor;
    let mut selection = None;
    let mut tabstops = Vec::<CompletionTabstop>::new();
    let mut best_placeholder: Option<(u32, usize, usize)> = None;
    let mut best_tabstop: Option<(u32, usize)> = None;
    let mut final_cursor = None;

    let mut it = snippet.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '\\' => {
                if let Some(escaped) = it.next() {
                    out.push(escaped);
                    out_chars = out_chars.saturating_add(1);
                }
            }
            '$' => match it.peek().copied() {
                Some('{') => {
                    let _ = it.next();
                    let mut content = String::new();
                    let mut depth = 0usize;
                    for c in it.by_ref() {
                        match c {
                            '{' => {
                                depth = depth.saturating_add(1);
                                content.push(c);
                            }
                            '}' => {
                                if depth == 0 {
                                    break;
                                }
                                depth = depth.saturating_sub(1);
                                content.push(c);
                            }
                            _ => content.push(c),
                        }
                    }

                    let digits = content
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    let index: Option<u32> = if digits.is_empty() {
                        None
                    } else {
                        digits.parse().ok()
                    };

                    if let Some(index) = index {
                        let rest = content.get(digits.len()..).unwrap_or_default();
                        let (inserted, inserted_is_placeholder) = if let Some((_, text)) =
                            rest.split_once(':')
                        {
                            (text.to_string(), true)
                        } else if let (Some(start), Some(end)) = (rest.find('|'), rest.rfind('|')) {
                            if end > start.saturating_add(1) {
                                let opts = &rest[start + 1..end];
                                let first = opts.split(',').next().unwrap_or_default().to_string();
                                (first, true)
                            } else {
                                (String::new(), false)
                            }
                        } else {
                            (String::new(), false)
                        };

                        if !inserted.is_empty() {
                            let start = out_chars;
                            out.push_str(&inserted);
                            let inserted_chars = inserted.chars().count();
                            out_chars = out_chars.saturating_add(inserted_chars);
                            let end = out_chars;

                            if inserted_is_placeholder && index > 0 {
                                let replace = best_placeholder
                                    .as_ref()
                                    .is_none_or(|(best_idx, _, _)| index < *best_idx);
                                if replace {
                                    best_placeholder = Some((index, start, end));
                                }
                            }

                            tabstops.push(CompletionTabstop { index, start, end });
                        } else if index == 0 {
                            final_cursor = Some(out_chars);
                            tabstops.push(CompletionTabstop {
                                index,
                                start: out_chars,
                                end: out_chars,
                            });
                        } else if index > 0 {
                            let replace = best_tabstop
                                .as_ref()
                                .is_none_or(|(best_idx, _)| index < *best_idx);
                            if replace {
                                best_tabstop = Some((index, out_chars));
                            }
                            tabstops.push(CompletionTabstop {
                                index,
                                start: out_chars,
                                end: out_chars,
                            });
                        }

                        continue;
                    }
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num: u32 = 0;
                    while it.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let digit = it.next().unwrap_or_default();
                        num = num
                            .saturating_mul(10)
                            .saturating_add((digit as u32).saturating_sub('0' as u32));
                    }
                    if num == 0 {
                        final_cursor = Some(out_chars);
                        tabstops.push(CompletionTabstop {
                            index: 0,
                            start: out_chars,
                            end: out_chars,
                        });
                    } else {
                        let replace = best_tabstop
                            .as_ref()
                            .is_none_or(|(best_idx, _)| num < *best_idx);
                        if replace {
                            best_tabstop = Some((num, out_chars));
                        }
                        tabstops.push(CompletionTabstop {
                            index: num,
                            start: out_chars,
                            end: out_chars,
                        });
                    }
                    continue;
                }
                _ => {
                    out.push('$');
                    out_chars = out_chars.saturating_add(1);
                }
            },
            _ => {
                out.push(ch);
                out_chars = out_chars.saturating_add(1);
            }
        }
    }

    if let Some((_idx, start, end)) = best_placeholder {
        selection = Some((start, end));
        cursor = Some(end);
    } else if let Some((_idx, pos)) = best_tabstop {
        cursor = Some(pos);
    } else {
        cursor = final_cursor;
    }

    SnippetExpansion {
        text: out,
        cursor,
        selection,
        tabstops,
    }
}
