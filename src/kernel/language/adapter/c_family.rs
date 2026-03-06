use crate::core::Command;
use crate::kernel::editor::EditorTabState;
use crate::kernel::language::adapter::syntax_bridge::{syntax_facts_for_tab, SYNTAX_BRIDGE};
use crate::kernel::language::adapter::{
    default_context_allows_completion, default_normalize_completion_item, default_prefix_bounds,
    default_should_close_on_command, default_triggered_by_insert, language_features,
    should_append_trailing_space, CompletionBehavior, CompletionContext, LanguageAdapter,
    LanguageFeatures, LineContext, MemberAccessKind, TextEditPlan,
};
use crate::kernel::language::LanguageId;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static INCLUDE_CONTEXT_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_include_context_perf_counter() {
    INCLUDE_CONTEXT_CALLS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn include_context_perf_counter() -> usize {
    INCLUDE_CONTEXT_CALLS.load(Ordering::Relaxed)
}

#[cfg_attr(not(test), allow(dead_code))]
#[cfg(not(test))]
pub(crate) fn reset_include_context_perf_counter() {}

#[cfg_attr(not(test), allow(dead_code))]
#[cfg(not(test))]
pub(crate) fn include_context_perf_counter() -> usize {
    0
}

pub(crate) struct CFamilyCompletionBehavior;

impl CompletionBehavior for CFamilyCompletionBehavior {
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
        if include_bounds(tab).is_some() {
            return true;
        }

        match syntax_facts_for_tab(tab).line_context {
            LineContext::Include(_) | LineContext::Directive => false,
            _ => {
                if is_hash_at_line_start(tab) {
                    return true;
                }
                default_context_allows_completion(tab)
            }
        }
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '>' || ch == '/' || ch == '<'
    }

    fn prefix_bounds(&self, tab: &EditorTabState) -> (usize, usize) {
        include_bounds(tab).unwrap_or_else(|| default_prefix_bounds(tab))
    }

    fn triggered_by_insert(&self, tab: &EditorTabState, ch: char, triggers: &[char]) -> bool {
        if ch == '#' && is_hash_at_line_start(tab) {
            return true;
        }
        if ch == '>' && preceded_by_dash(tab) {
            return true;
        }
        if matches!(ch, '<' | '/') && include_bounds(tab).is_some() {
            return true;
        }
        default_triggered_by_insert(tab, ch, triggers)
    }

    fn should_close_on_command(&self, cmd: &Command, tab: Option<&EditorTabState>) -> bool {
        match cmd {
            Command::InsertChar('/') | Command::InsertChar('<') => {
                tab.is_none_or(|current| include_bounds(current).is_none())
            }
            Command::InsertChar('>') => tab.is_none_or(|current| {
                !preceded_by_dash_for_insert(current) && include_bounds(current).is_none()
            }),
            _ => default_should_close_on_command(self, cmd),
        }
    }

    fn completion_should_keep_open(&self, tab: &EditorTabState) -> bool {
        if let Some((start_char, end_char)) = include_bounds(tab) {
            if start_char != end_char {
                return true;
            }
            let rope = tab.buffer.rope();
            if end_char > 0 {
                let prev = rope.char(end_char - 1);
                if matches!(prev, '<' | '"' | '/') {
                    return true;
                }
            }
            return false;
        }

        let syntax = syntax_facts_for_tab(tab);
        if matches!(
            syntax.line_context,
            LineContext::Include(_) | LineContext::Directive
        ) {
            return false;
        }
        if syntax.in_string_or_comment() {
            return false;
        }
        if syntax.identifier_bounds.is_some() {
            return true;
        }

        matches!(
            syntax.member_access_kind,
            Some(MemberAccessKind::Dot | MemberAccessKind::Scope | MemberAccessKind::Arrow)
        )
    }

    fn normalize_completion_item(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        let syntax = &context.runtime.syntax;
        let suppress_callable_fallback =
            matches!(
                syntax.member_access_kind,
                Some(MemberAccessKind::Arrow | MemberAccessKind::Scope)
            ) || matches!(syntax.line_context, LineContext::Include(_));

        if !suppress_callable_fallback {
            return default_normalize_completion_item(context);
        }

        let mut plan = match context.item.insert_text_format {
            crate::kernel::services::ports::LspInsertTextFormat::Snippet => {
                TextEditPlan::from_snippet(&context.item.insert_text)
            }
            crate::kernel::services::ports::LspInsertTextFormat::PlainText => {
                TextEditPlan::from_plain_text(context.item.insert_text.clone())
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
}

pub(crate) static C_FAMILY_COMPLETION: CFamilyCompletionBehavior = CFamilyCompletionBehavior;

pub(crate) struct CFamilyLanguageAdapter {
    language: LanguageId,
}

impl CFamilyLanguageAdapter {
    const fn new(language: LanguageId) -> Self {
        Self { language }
    }
}

impl LanguageAdapter for CFamilyLanguageAdapter {
    fn interaction(&self) -> &dyn crate::kernel::language::adapter::LanguageInteractionPolicy {
        &C_FAMILY_COMPLETION
    }

    fn completion_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::CompletionProtocolAdapter {
        &C_FAMILY_COMPLETION
    }

    fn signature_help_protocol(
        &self,
    ) -> &dyn crate::kernel::language::adapter::SignatureHelpProtocolAdapter {
        &C_FAMILY_COMPLETION
    }

    fn hover_protocol(&self) -> &dyn crate::kernel::language::adapter::HoverProtocolAdapter {
        &C_FAMILY_COMPLETION
    }

    fn syntax(&self) -> &dyn crate::kernel::language::adapter::SyntaxBehavior {
        &SYNTAX_BRIDGE
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(self.language))
    }
}

pub(crate) static C_ADAPTER: CFamilyLanguageAdapter = CFamilyLanguageAdapter::new(LanguageId::C);
pub(crate) static CPP_ADAPTER: CFamilyLanguageAdapter =
    CFamilyLanguageAdapter::new(LanguageId::Cpp);

fn include_bounds(tab: &EditorTabState) -> Option<(usize, usize)> {
    #[cfg(test)]
    {
        INCLUDE_CONTEXT_CALLS.fetch_add(1, Ordering::Relaxed);
    }

    match syntax_facts_for_tab(tab).line_context {
        LineContext::Include(include) => include.bounds,
        _ => None,
    }
}

fn is_hash_at_line_start(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let cursor = crate::kernel::language::adapter::cursor_char_offset(tab);
    if cursor == 0 || rope.char(cursor - 1) != '#' {
        return false;
    }
    let row = rope.char_to_line(cursor);
    let line_start = rope.line_to_char(row);
    for idx in line_start..cursor.saturating_sub(1) {
        let ch = rope.char(idx);
        if ch != ' ' && ch != '\t' {
            return false;
        }
    }
    true
}

fn preceded_by_dash(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let cursor = crate::kernel::language::adapter::cursor_char_offset(tab);
    cursor >= 2 && rope.char(cursor - 2) == '-' && rope.char(cursor - 1) == '>'
}

fn preceded_by_dash_for_insert(tab: &EditorTabState) -> bool {
    let rope = tab.buffer.rope();
    let cursor = crate::kernel::language::adapter::cursor_char_offset(tab);
    cursor > 0 && rope.char(cursor - 1) == '-'
}
