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
use crate::kernel::services::ports::{
    LspCommand, LspCompletionItem, LspHoverBlock, LspHoverPayload, LspInsertTextFormat, LspMarkup,
    LspRange, LspServerCapabilities, LspServerKind, LspSignatureHelpPayload,
    LspSignatureParameterLabel, LspTextEdit,
};

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
pub struct LanguageRuntimeContext<'a> {
    pub language: Option<LanguageId>,
    pub server: Option<LspServerKind>,
    pub server_caps: Option<&'a LspServerCapabilities>,
    pub tab: &'a EditorTabState,
    pub syntax: SyntaxFacts,
}

impl<'a> LanguageRuntimeContext<'a> {
    pub fn new(language: Option<LanguageId>, tab: &'a EditorTabState, syntax: SyntaxFacts) -> Self {
        Self {
            language,
            server: None,
            server_caps: None,
            tab,
            syntax,
        }
    }

    pub fn with_server(
        mut self,
        server: Option<LspServerKind>,
        server_caps: Option<&'a LspServerCapabilities>,
    ) -> Self {
        self.server = server;
        self.server_caps = server_caps;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CompletionContext<'a> {
    pub runtime: LanguageRuntimeContext<'a>,
    pub item: &'a LspCompletionItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEditStrategy {
    PlainText,
    NativeSnippet,
    SynthesizedSnippet,
    CallableTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTabstop {
    pub index: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEditPlan {
    pub text: String,
    pub cursor: Option<usize>,
    pub selection: Option<(usize, usize)>,
    pub tabstops: Vec<TextTabstop>,
    pub strategy: TextEditStrategy,
}

impl TextEditPlan {
    pub fn from_plain_text(text: String) -> Self {
        let cursor = text
            .strip_suffix("()")
            .map(|prefix| prefix.chars().count().saturating_add(1));
        let strategy = if cursor.is_some() {
            TextEditStrategy::CallableTemplate
        } else {
            TextEditStrategy::PlainText
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
            strategy: TextEditStrategy::NativeSnippet,
        }
    }

    pub fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum CompletionReplacePolicy {
    IdentifierPrefix,
    ServerRange {
        insert_range: Option<LspRange>,
        replace_range: Option<LspRange>,
        anchor_to_prefix: bool,
    },
}

#[derive(Debug, Clone)]
pub struct CompletionCommitPlan {
    pub replace: CompletionReplacePolicy,
    pub insert: TextEditPlan,
    pub additional_edits: Vec<LspTextEdit>,
    pub command: Option<LspCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionResolveState {
    Unsupported,
    Unresolved,
    Resolving,
    Resolved,
}

#[derive(Debug, Clone)]
pub struct CompletionEntry {
    pub id: u64,
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub filter_text: Option<String>,
    pub sort_text: Option<String>,
    pub kind: Option<u32>,
    pub commit: CompletionCommitPlan,
    pub resolve_state: CompletionResolveState,
}

#[derive(Debug, Clone)]
pub struct CompletionRecord {
    pub entry: CompletionEntry,
    pub raw: LspCompletionItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelpModel {
    pub label: String,
    pub documentation: Option<String>,
    pub active_parameter_range: Option<(usize, usize)>,
    pub overload_count: usize,
}

impl SignatureHelpModel {
    pub fn to_display_text(&self) -> String {
        let mut lines = Vec::with_capacity(2);
        if self.overload_count > 1 {
            lines.push(format!("{} ({})", self.label, self.overload_count));
        } else {
            lines.push(self.label.clone());
        }
        if let Some(doc) = self
            .documentation
            .as_deref()
            .filter(|doc| !doc.trim().is_empty())
        {
            lines.push(doc.trim().to_string());
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverBlock {
    Markdown(String),
    Code {
        language: Option<LanguageId>,
        code: String,
    },
    PlainText(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HoverModel {
    pub blocks: Vec<HoverBlock>,
    pub range: Option<LspRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverSectionModel {
    pub title: String,
    pub body: HoverModel,
}

impl HoverSectionModel {
    pub fn to_display_text(&self) -> String {
        let body = self.body.to_display_text();
        if body.trim().is_empty() {
            self.title.trim().to_string()
        } else if self.title.trim().is_empty() {
            body
        } else {
            format!("{}\n\n{}", self.title.trim(), body)
        }
    }
}

impl HoverModel {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn to_display_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| match block {
                HoverBlock::Markdown(text) | HoverBlock::PlainText(text) => text.trim().to_string(),
                HoverBlock::Code { language, code } => {
                    let mut out = String::new();
                    out.push_str("```");
                    if let Some(language) = language {
                        out.push_str(language_code_fence(*language));
                    }
                    out.push('\n');
                    out.push_str(code.trim_end());
                    out.push('\n');
                    out.push_str("```");
                    out
                }
            })
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl CompletionRecord {
    pub fn from_raw_unresolved(raw: LspCompletionItem) -> Self {
        let insert = match raw.insert_text_format {
            LspInsertTextFormat::PlainText => {
                TextEditPlan::from_plain_text(raw.insert_text.clone())
            }
            LspInsertTextFormat::Snippet => TextEditPlan::from_snippet(&raw.insert_text),
        };
        let resolve_state = if raw.data.is_some() {
            CompletionResolveState::Unresolved
        } else {
            CompletionResolveState::Resolved
        };
        let entry = CompletionEntry {
            id: raw.id,
            label: raw.label.clone(),
            detail: raw.detail.clone(),
            documentation: raw.documentation.clone(),
            filter_text: raw.filter_text.clone(),
            sort_text: raw.sort_text.clone(),
            kind: raw.kind,
            commit: CompletionCommitPlan {
                replace: default_completion_replace_policy(&raw),
                insert,
                additional_edits: raw.additional_text_edits.clone(),
                command: raw.command.clone(),
            },
            resolve_state,
        };
        Self { entry, raw }
    }
}

impl From<LspCompletionItem> for CompletionRecord {
    fn from(raw: LspCompletionItem) -> Self {
        Self::from_raw_unresolved(raw)
    }
}

pub trait SyntaxBehavior: Send + Sync {
    fn syntax_facts(&self, tab: &EditorTabState) -> SyntaxFacts;
}

pub trait LanguageInteractionPolicy: Send + Sync {
    fn debounce_triggered_by_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == ':'
    }

    fn context_allows_completion(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        default_context_allows_completion(ctx.tab)
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '.'
    }

    fn completion_prefix_bounds(&self, ctx: &LanguageRuntimeContext<'_>) -> (usize, usize) {
        default_prefix_bounds(ctx.tab)
    }

    fn completion_triggered_by_insert(
        &self,
        ctx: &LanguageRuntimeContext<'_>,
        ch: char,
        triggers: &[char],
    ) -> bool {
        default_triggered_by_insert(ctx.tab, ch, triggers)
    }

    fn signature_help_triggered(
        &self,
        _ctx: &LanguageRuntimeContext<'_>,
        ch: char,
        triggers: &[char],
    ) -> bool {
        if triggers.is_empty() {
            matches!(ch, '(' | ',')
        } else {
            triggers.contains(&ch)
        }
    }

    fn signature_help_closed_by(&self, ch: char) -> bool {
        matches!(ch, ')')
    }

    fn signature_help_should_keep_open(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        default_signature_help_should_keep_open(ctx.tab)
    }

    fn should_close_on_command(
        &self,
        cmd: &Command,
        _ctx: Option<&LanguageRuntimeContext<'_>>,
    ) -> bool {
        default_should_close_on_command(self, cmd)
    }

    fn completion_should_keep_open(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        default_completion_should_keep_open(ctx.tab)
    }
}

pub trait CompletionProtocolAdapter: Send + Sync {
    fn normalize_completion_text(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        default_normalize_completion_item(context)
    }

    fn completion_replace_policy(
        &self,
        context: &CompletionContext<'_>,
    ) -> CompletionReplacePolicy {
        default_completion_replace_policy(context.item)
    }

    fn completion_resolve_state(&self, context: &CompletionContext<'_>) -> CompletionResolveState {
        default_completion_resolve_state(&context.runtime, context.item)
    }

    fn normalize_completion(&self, context: &CompletionContext<'_>) -> CompletionEntry {
        default_normalize_completion(
            context,
            self.normalize_completion_text(context),
            self.completion_replace_policy(context),
            self.completion_resolve_state(context),
        )
    }
}

pub trait SignatureHelpProtocolAdapter: Send + Sync {
    fn normalize_signature_help(
        &self,
        ctx: &LanguageRuntimeContext<'_>,
        payload: &LspSignatureHelpPayload,
    ) -> Option<SignatureHelpModel> {
        default_normalize_signature_help(ctx, payload)
    }
}

pub trait HoverProtocolAdapter: Send + Sync {
    fn normalize_hover(
        &self,
        ctx: &LanguageRuntimeContext<'_>,
        payload: &LspHoverPayload,
    ) -> Option<HoverModel> {
        default_normalize_hover(ctx, payload)
    }
}

pub(crate) trait CompletionBehavior: Send + Sync {
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

    fn normalize_completion_item(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        default_normalize_completion_item(context)
    }
}

impl<T> LanguageInteractionPolicy for T
where
    T: CompletionBehavior + ?Sized,
{
    fn debounce_triggered_by_char(&self, ch: char) -> bool {
        CompletionBehavior::debounce_triggered_by_char(self, ch)
    }

    fn context_allows_completion(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        CompletionBehavior::context_allows_completion(self, ctx.tab)
    }

    fn keeps_open_on_char(&self, ch: char) -> bool {
        CompletionBehavior::keeps_open_on_char(self, ch)
    }

    fn completion_prefix_bounds(&self, ctx: &LanguageRuntimeContext<'_>) -> (usize, usize) {
        CompletionBehavior::prefix_bounds(self, ctx.tab)
    }

    fn completion_triggered_by_insert(
        &self,
        ctx: &LanguageRuntimeContext<'_>,
        ch: char,
        triggers: &[char],
    ) -> bool {
        CompletionBehavior::triggered_by_insert(self, ctx.tab, ch, triggers)
    }

    fn signature_help_triggered(
        &self,
        _ctx: &LanguageRuntimeContext<'_>,
        ch: char,
        triggers: &[char],
    ) -> bool {
        CompletionBehavior::signature_help_triggered(self, ch, triggers)
    }

    fn signature_help_closed_by(&self, ch: char) -> bool {
        CompletionBehavior::signature_help_closed_by(self, ch)
    }

    fn signature_help_should_keep_open(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        CompletionBehavior::signature_help_should_keep_open(self, ctx.tab)
    }

    fn should_close_on_command(
        &self,
        cmd: &Command,
        ctx: Option<&LanguageRuntimeContext<'_>>,
    ) -> bool {
        CompletionBehavior::should_close_on_command(self, cmd, ctx.map(|runtime| runtime.tab))
    }

    fn completion_should_keep_open(&self, ctx: &LanguageRuntimeContext<'_>) -> bool {
        CompletionBehavior::completion_should_keep_open(self, ctx.tab)
    }
}

impl<T> CompletionProtocolAdapter for T
where
    T: CompletionBehavior + ?Sized,
{
    fn normalize_completion_text(&self, context: &CompletionContext<'_>) -> TextEditPlan {
        CompletionBehavior::normalize_completion_item(self, context)
    }
}

impl<T> SignatureHelpProtocolAdapter for T where T: CompletionBehavior + ?Sized {}

impl<T> HoverProtocolAdapter for T where T: CompletionBehavior + ?Sized {}

pub trait LanguageAdapter: Send + Sync {
    fn interaction(&self) -> &dyn LanguageInteractionPolicy;
    fn completion_protocol(&self) -> &dyn CompletionProtocolAdapter;
    fn signature_help_protocol(&self) -> &dyn SignatureHelpProtocolAdapter;
    fn hover_protocol(&self) -> &dyn HoverProtocolAdapter;
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

pub(crate) fn default_should_close_on_command<S: LanguageInteractionPolicy + ?Sized>(
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

pub(crate) fn default_normalize_completion_item(context: &CompletionContext<'_>) -> TextEditPlan {
    let mut plan = match context.item.insert_text_format {
        LspInsertTextFormat::PlainText => {
            let mut plan = TextEditPlan::from_plain_text(context.item.insert_text.clone());
            if !plan.has_cursor_or_selection() && should_append_callable_parentheses(context.item) {
                plan.text.push('(');
                plan.text.push(')');
                plan.cursor = Some(plan.text.chars().count().saturating_sub(1));
                plan.strategy = TextEditStrategy::CallableTemplate;
            }
            plan
        }
        LspInsertTextFormat::Snippet => TextEditPlan::from_snippet(&context.item.insert_text),
    };

    if !plan.has_cursor_or_selection()
        && should_append_trailing_space(context.item)
        && !plan.text.ends_with(' ')
    {
        plan.text.push(' ');
    }

    plan
}

pub(crate) fn default_completion_replace_policy(
    item: &LspCompletionItem,
) -> CompletionReplacePolicy {
    if item.insert_range.is_some() || item.replace_range.is_some() {
        CompletionReplacePolicy::ServerRange {
            insert_range: item.insert_range,
            replace_range: item.replace_range,
            anchor_to_prefix: true,
        }
    } else {
        CompletionReplacePolicy::IdentifierPrefix
    }
}

pub(crate) fn default_completion_resolve_state(
    runtime: &LanguageRuntimeContext<'_>,
    item: &LspCompletionItem,
) -> CompletionResolveState {
    if runtime
        .server_caps
        .is_some_and(|caps| !caps.completion_resolve)
    {
        CompletionResolveState::Unsupported
    } else if item.data.is_some() {
        CompletionResolveState::Unresolved
    } else {
        CompletionResolveState::Resolved
    }
}

pub(crate) fn default_normalize_completion(
    context: &CompletionContext<'_>,
    insert: TextEditPlan,
    replace: CompletionReplacePolicy,
    resolve_state: CompletionResolveState,
) -> CompletionEntry {
    CompletionEntry {
        id: context.item.id,
        label: context.item.label.clone(),
        detail: context.item.detail.clone(),
        documentation: context.item.documentation.clone(),
        filter_text: context.item.filter_text.clone(),
        sort_text: context.item.sort_text.clone(),
        kind: context.item.kind,
        commit: CompletionCommitPlan {
            replace,
            insert,
            additional_edits: context.item.additional_text_edits.clone(),
            command: context.item.command.clone(),
        },
        resolve_state,
    }
}

pub(crate) fn default_normalize_signature_help(
    _ctx: &LanguageRuntimeContext<'_>,
    payload: &LspSignatureHelpPayload,
) -> Option<SignatureHelpModel> {
    default_normalize_signature_help_payload(payload)
}

pub(crate) fn default_normalize_signature_help_payload(
    payload: &LspSignatureHelpPayload,
) -> Option<SignatureHelpModel> {
    let active_signature = payload.active_signature.unwrap_or(0) as usize;
    let signature = payload.signatures.get(active_signature)?;
    let active_parameter = payload.active_parameter.unwrap_or(0) as usize;
    let active_parameter_range = signature
        .parameters
        .get(active_parameter)
        .and_then(|parameter| {
            signature_parameter_range(signature.label.as_str(), &parameter.label)
        });

    Some(SignatureHelpModel {
        label: highlight_signature_label(signature.label.as_str(), active_parameter_range),
        documentation: signature.documentation.as_ref().map(markup_to_display_text),
        active_parameter_range,
        overload_count: payload.signatures.len(),
    })
}

pub(crate) fn default_normalize_hover(
    _ctx: &LanguageRuntimeContext<'_>,
    payload: &LspHoverPayload,
) -> Option<HoverModel> {
    default_normalize_hover_payload(payload)
}

pub(crate) fn default_normalize_hover_payload(payload: &LspHoverPayload) -> Option<HoverModel> {
    let blocks = payload
        .blocks
        .iter()
        .filter_map(|block| match block {
            LspHoverBlock::Markdown(text) if !text.trim().is_empty() => {
                Some(HoverBlock::Markdown(text.clone()))
            }
            LspHoverBlock::PlainText(text) if !text.trim().is_empty() => {
                Some(HoverBlock::PlainText(text.clone()))
            }
            LspHoverBlock::Code { language, code } if !code.trim().is_empty() => {
                Some(HoverBlock::Code {
                    language: language.as_deref().and_then(language_id_from_code_fence),
                    code: code.clone(),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    if blocks.is_empty() {
        None
    } else {
        Some(HoverModel {
            blocks,
            range: payload.range,
        })
    }
}

fn markup_to_display_text(markup: &LspMarkup) -> String {
    match markup {
        LspMarkup::Markdown(text) | LspMarkup::PlainText(text) => text.trim().to_string(),
    }
}

fn signature_parameter_range(
    label: &str,
    parameter: &LspSignatureParameterLabel,
) -> Option<(usize, usize)> {
    match parameter {
        LspSignatureParameterLabel::Simple(text) => {
            let start = label.find(text)?;
            Some((start, start.saturating_add(text.len())))
        }
        LspSignatureParameterLabel::Offsets { start, end } => {
            let start = utf16_offset_to_byte(label, *start);
            let end = utf16_offset_to_byte(label, *end);
            Some((start, end.max(start)))
        }
    }
}

fn highlight_signature_label(label: &str, range: Option<(usize, usize)>) -> String {
    let Some((start, end)) = range else {
        return label.to_string();
    };
    if start >= end || end > label.len() {
        return label.to_string();
    }

    format!(
        "{}[{}]{}",
        &label[..start],
        &label[start..end],
        &label[end..]
    )
}

fn utf16_offset_to_byte(text: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in text.char_indices() {
        let next = units.saturating_add(ch.len_utf16() as u32);
        if next > offset {
            return byte;
        }
        units = next;
    }
    text.len()
}

fn language_id_from_code_fence(language: &str) -> Option<LanguageId> {
    match language.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => Some(LanguageId::Rust),
        "go" => Some(LanguageId::Go),
        "python" | "py" => Some(LanguageId::Python),
        "javascript" | "js" => Some(LanguageId::JavaScript),
        "typescript" | "ts" => Some(LanguageId::TypeScript),
        "jsx" => Some(LanguageId::Jsx),
        "tsx" => Some(LanguageId::Tsx),
        "c" => Some(LanguageId::C),
        "cpp" | "c++" | "cc" | "cxx" => Some(LanguageId::Cpp),
        "java" => Some(LanguageId::Java),
        "json" => Some(LanguageId::Json),
        "yaml" | "yml" => Some(LanguageId::Yaml),
        "html" => Some(LanguageId::Html),
        "xml" => Some(LanguageId::Xml),
        "css" => Some(LanguageId::Css),
        "toml" => Some(LanguageId::Toml),
        "sql" => Some(LanguageId::Sql),
        "bash" | "sh" | "shell" => Some(LanguageId::Bash),
        "markdown" | "md" => Some(LanguageId::Markdown),
        _ => None,
    }
}

fn language_code_fence(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "rust",
        LanguageId::Go => "go",
        LanguageId::Python => "python",
        LanguageId::JavaScript => "javascript",
        LanguageId::TypeScript => "typescript",
        LanguageId::Jsx => "jsx",
        LanguageId::Tsx => "tsx",
        LanguageId::C => "c",
        LanguageId::Cpp => "cpp",
        LanguageId::Java => "java",
        LanguageId::Json => "json",
        LanguageId::Yaml => "yaml",
        LanguageId::Html => "html",
        LanguageId::Xml => "xml",
        LanguageId::Css => "css",
        LanguageId::Toml => "toml",
        LanguageId::Sql => "sql",
        LanguageId::Bash => "bash",
        LanguageId::Markdown => "markdown",
    }
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
    pub(crate) tabstops: Vec<TextTabstop>,
}

pub(crate) fn expand_snippet(snippet: &str) -> SnippetExpansion {
    let mut out = String::with_capacity(snippet.len());
    let mut out_chars = 0usize;
    let cursor;
    let mut selection = None;
    let mut tabstops = Vec::<TextTabstop>::new();
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

                            tabstops.push(TextTabstop { index, start, end });
                        } else if index == 0 {
                            final_cursor = Some(out_chars);
                            tabstops.push(TextTabstop {
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
                            tabstops.push(TextTabstop {
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
                        tabstops.push(TextTabstop {
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
                        tabstops.push(TextTabstop {
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
