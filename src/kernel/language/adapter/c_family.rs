use crate::core::Command;
use crate::kernel::editor::EditorTabState;
use crate::kernel::language::adapter::editing::BRACE_LANGUAGE_EDITING_POLICY;
use crate::kernel::language::adapter::syntax_bridge::SYNTAX_BRIDGE;
use crate::kernel::language::adapter::{
    apply_callable_completion_fallback, default_context_allows_completion, default_prefix_bounds,
    default_should_close_on_command, default_triggered_by_insert, language_features,
    normalize_server_completion_text, CompletionBehavior, CompletionContext, IncludeContext,
    LanguageAdapter, LanguageEditingPolicy, LanguageFeatures, LanguageRuntimeContext, LineContext,
    MemberAccessKind, SyntaxFacts, TextEditPlan,
};
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::{LspCompletionItem, LspInsertTextFormat};

pub(crate) struct CFamilyCompletionBehavior;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectiveCompletionState {
    None,
    DirectiveName,
    IncludeBeforeDelimiter,
    IncludePath,
    IncludeClosed,
}

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

    fn context_allows_completion(&self, syntax: &SyntaxFacts, tab: &EditorTabState) -> bool {
        match directive_completion_state(syntax, tab) {
            DirectiveCompletionState::DirectiveName
            | DirectiveCompletionState::IncludeBeforeDelimiter
            | DirectiveCompletionState::IncludePath => true,
            DirectiveCompletionState::IncludeClosed => false,
            DirectiveCompletionState::None => default_context_allows_completion(syntax),
        }
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '>' || ch == '/' || ch == '<'
    }

    fn prefix_bounds(&self, syntax: &SyntaxFacts, tab: &EditorTabState) -> (usize, usize) {
        include_bounds(syntax).unwrap_or_else(|| default_prefix_bounds(tab, syntax))
    }

    fn triggered_by_insert(
        &self,
        syntax: &SyntaxFacts,
        tab: &EditorTabState,
        ch: char,
        triggers: &[char],
    ) -> bool {
        if ch == '#' && is_hash_at_line_start(tab) {
            return true;
        }
        if ch == '>' && preceded_by_dash(tab) {
            return true;
        }
        if matches!(ch, '<' | '/') && include_bounds(syntax).is_some() {
            return true;
        }
        default_triggered_by_insert(syntax, ch, triggers)
    }

    fn should_close_on_command(
        &self,
        cmd: &Command,
        syntax: Option<&SyntaxFacts>,
        tab: Option<&EditorTabState>,
    ) -> bool {
        match cmd {
            Command::InsertChar(' ') => syntax.zip(tab).is_none_or(|(syntax, tab)| {
                !matches!(
                    directive_completion_state(syntax, tab),
                    DirectiveCompletionState::DirectiveName
                        | DirectiveCompletionState::IncludeBeforeDelimiter
                )
            }),
            Command::InsertChar('/') | Command::InsertChar('<') => {
                syntax.is_none_or(|syntax| include_bounds(syntax).is_none())
            }
            Command::InsertChar('>') => syntax.zip(tab).is_none_or(|(syntax, tab)| {
                !preceded_by_dash_for_insert(tab) && include_bounds(syntax).is_none()
            }),
            _ => default_should_close_on_command(self, cmd),
        }
    }

    fn completion_should_keep_open(&self, syntax: &SyntaxFacts, tab: &EditorTabState) -> bool {
        match directive_completion_state(syntax, tab) {
            DirectiveCompletionState::IncludePath => {
                if let Some((start_char, end_char)) = include_bounds(syntax) {
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
                }
                false
            }
            DirectiveCompletionState::IncludeClosed => false,
            DirectiveCompletionState::DirectiveName
            | DirectiveCompletionState::IncludeBeforeDelimiter => true,
            DirectiveCompletionState::None => {
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
        }
    }

    fn normalize_completion_item(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        let syntax = context.syntax();
        let suppress_callable_fallback =
            matches!(
                syntax.member_access_kind,
                Some(MemberAccessKind::Arrow | MemberAccessKind::Scope)
            ) || matches!(syntax.line_context, LineContext::Include(_));

        let plan = normalize_server_completion_text(context);
        if suppress_callable_fallback {
            plan
        } else {
            apply_callable_completion_fallback(plan, context.item)
        }
    }

    fn fallback_completion_items(
        &self,
        ctx: &LanguageRuntimeContext<'_>,
    ) -> Vec<LspCompletionItem> {
        match directive_completion_state(&ctx.syntax, ctx.tab) {
            DirectiveCompletionState::DirectiveName => directive_keyword_completion_items(),
            DirectiveCompletionState::IncludeBeforeDelimiter => {
                include_delimiter_completion_items()
            }
            DirectiveCompletionState::IncludePath
            | DirectiveCompletionState::IncludeClosed
            | DirectiveCompletionState::None => Vec::new(),
        }
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

    fn editing(&self) -> &dyn LanguageEditingPolicy {
        &BRACE_LANGUAGE_EDITING_POLICY
    }

    fn features(&self) -> LanguageFeatures {
        language_features(Some(self.language))
    }
}

pub(crate) static C_ADAPTER: CFamilyLanguageAdapter = CFamilyLanguageAdapter::new(LanguageId::C);
pub(crate) static CPP_ADAPTER: CFamilyLanguageAdapter =
    CFamilyLanguageAdapter::new(LanguageId::Cpp);

fn include_bounds(syntax: &SyntaxFacts) -> Option<(usize, usize)> {
    match syntax.line_context {
        LineContext::Include(include) => include.bounds,
        _ => None,
    }
}

fn directive_completion_state(
    syntax: &SyntaxFacts,
    tab: &EditorTabState,
) -> DirectiveCompletionState {
    match syntax.line_context {
        LineContext::Include(IncludeContext {
            bounds: Some(_), ..
        }) => DirectiveCompletionState::IncludePath,
        LineContext::Include(IncludeContext {
            bounds: None,
            delimiter: None,
        }) => DirectiveCompletionState::IncludeBeforeDelimiter,
        LineContext::Include(_) => DirectiveCompletionState::IncludeClosed,
        LineContext::Directive | LineContext::Import | LineContext::None => {
            non_include_directive_completion_state(tab)
        }
    }
}

fn non_include_directive_completion_state(tab: &EditorTabState) -> DirectiveCompletionState {
    let rope = tab.buffer.rope();
    let cursor = crate::kernel::language::adapter::cursor_char_offset(tab).min(rope.len_chars());
    let row = rope.char_to_line(cursor);
    let line_start = rope.line_to_char(row);

    let mut idx = line_start;
    while idx < cursor && matches!(rope.char(idx), ' ' | '\t') {
        idx = idx.saturating_add(1);
    }

    if idx >= cursor || rope.char(idx) != '#' {
        return DirectiveCompletionState::None;
    }
    idx = idx.saturating_add(1);

    while idx < cursor && matches!(rope.char(idx), ' ' | '\t') {
        idx = idx.saturating_add(1);
    }

    if idx >= cursor {
        return DirectiveCompletionState::DirectiveName;
    }

    let token_start = idx;
    while idx < cursor {
        let ch = rope.char(idx);
        if ch == '_' || ch.is_ascii_alphanumeric() {
            idx = idx.saturating_add(1);
            continue;
        }
        break;
    }

    if idx == token_start {
        return DirectiveCompletionState::None;
    }

    if idx == cursor {
        DirectiveCompletionState::DirectiveName
    } else {
        DirectiveCompletionState::None
    }
}

fn directive_keyword_completion_items() -> Vec<LspCompletionItem> {
    [
        (
            "include",
            "include ",
            "preprocessor directive",
            "#include header",
        ),
        (
            "define",
            "define ",
            "preprocessor directive",
            "#define macro",
        ),
        ("ifdef", "ifdef ", "preprocessor directive", "#ifdef symbol"),
        (
            "ifndef",
            "ifndef ",
            "preprocessor directive",
            "#ifndef symbol",
        ),
        ("if", "if ", "preprocessor directive", "#if expression"),
        (
            "elif",
            "elif ",
            "preprocessor directive",
            "#elif expression",
        ),
        ("else", "else", "preprocessor directive", "#else"),
        ("endif", "endif", "preprocessor directive", "#endif"),
        (
            "pragma",
            "pragma ",
            "preprocessor directive",
            "#pragma option",
        ),
        ("undef", "undef ", "preprocessor directive", "#undef symbol"),
        (
            "error",
            "error ",
            "preprocessor directive",
            "#error message",
        ),
        ("line", "line ", "preprocessor directive", "#line number"),
    ]
    .into_iter()
    .enumerate()
    .map(|(idx, (label, insert_text, detail, documentation))| {
        synthetic_completion_item(SyntheticCompletionSpec {
            id: 0xC000_0000_0000_0000 + idx as u64,
            label: label.to_string(),
            insert_text: insert_text.to_string(),
            kind: Some(14),
            detail: Some(detail.to_string()),
            documentation: Some(documentation.to_string()),
            insert_text_format: LspInsertTextFormat::PlainText,
            sort_text: Some(format!("{idx:02}")),
            filter_text: None,
        })
    })
    .collect()
}

fn include_delimiter_completion_items() -> Vec<LspCompletionItem> {
    vec![
        synthetic_completion_item(SyntheticCompletionSpec {
            id: 0xC000_0000_0000_0100,
            label: "<...>".to_string(),
            insert_text: "<$0>".to_string(),
            kind: None,
            detail: Some("system header".to_string()),
            documentation: Some("Insert angle brackets for a system header include.".to_string()),
            insert_text_format: LspInsertTextFormat::Snippet,
            sort_text: Some("00".to_string()),
            filter_text: Some("<".to_string()),
        }),
        synthetic_completion_item(SyntheticCompletionSpec {
            id: 0xC000_0000_0000_0101,
            label: "\"...\"".to_string(),
            insert_text: "\"$0\"".to_string(),
            kind: None,
            detail: Some("local header".to_string()),
            documentation: Some("Insert quotes for a local header include.".to_string()),
            insert_text_format: LspInsertTextFormat::Snippet,
            sort_text: Some("01".to_string()),
            filter_text: Some("\"".to_string()),
        }),
    ]
}

struct SyntheticCompletionSpec {
    id: u64,
    label: String,
    insert_text: String,
    kind: Option<u32>,
    detail: Option<String>,
    documentation: Option<String>,
    insert_text_format: LspInsertTextFormat,
    sort_text: Option<String>,
    filter_text: Option<String>,
}

fn synthetic_completion_item(spec: SyntheticCompletionSpec) -> LspCompletionItem {
    LspCompletionItem {
        id: spec.id,
        label: spec.label,
        detail: spec.detail,
        kind: spec.kind,
        documentation: spec.documentation,
        insert_text: spec.insert_text,
        insert_text_format: spec.insert_text_format,
        insert_range: None,
        replace_range: None,
        sort_text: spec.sort_text,
        filter_text: spec.filter_text,
        additional_text_edits: Vec::new(),
        command: None,
        data: None,
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
