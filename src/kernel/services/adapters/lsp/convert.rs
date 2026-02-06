use crate::kernel::problems::{ProblemItem, ProblemRange, ProblemSeverity};
use crate::kernel::services::ports::{
    LspCodeAction, LspCommand, LspCompletionItem, LspInlayHint, LspInsertTextFormat, LspPosition,
    LspPositionEncoding, LspRange, LspResourceOp, LspSemanticToken, LspSemanticTokensLegend,
    LspServerCapabilities, LspTextChange, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::symbols::SymbolItem;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::kernel::lsp_registry::LspLanguage;

pub(super) fn server_capabilities_from_lsp(
    caps: &lsp_types::ServerCapabilities,
) -> LspServerCapabilities {
    fn one_of_bool<T>(v: &Option<lsp_types::OneOf<bool, T>>) -> bool {
        match v {
            Some(lsp_types::OneOf::Left(enabled)) => *enabled,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }

    fn hover(v: &Option<lsp_types::HoverProviderCapability>) -> bool {
        match v {
            Some(lsp_types::HoverProviderCapability::Simple(enabled)) => *enabled,
            Some(lsp_types::HoverProviderCapability::Options(_)) => true,
            None => false,
        }
    }

    fn code_action(v: &Option<lsp_types::CodeActionProviderCapability>) -> bool {
        match v {
            Some(lsp_types::CodeActionProviderCapability::Simple(enabled)) => *enabled,
            Some(lsp_types::CodeActionProviderCapability::Options(_)) => true,
            None => false,
        }
    }

    fn folding(v: &Option<lsp_types::FoldingRangeProviderCapability>) -> bool {
        match v {
            Some(lsp_types::FoldingRangeProviderCapability::Simple(enabled)) => *enabled,
            Some(_) => true,
            None => false,
        }
    }

    fn triggers(v: &Option<Vec<String>>) -> Vec<char> {
        let mut out = Vec::new();
        let Some(v) = v else {
            return out;
        };
        for s in v {
            let mut it = s.chars();
            let Some(ch) = it.next() else {
                continue;
            };
            if it.next().is_some() {
                continue;
            }
            out.push(ch);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    let encoding = match caps
        .position_encoding
        .clone()
        .unwrap_or(lsp_types::PositionEncodingKind::UTF16)
    {
        kind if kind == lsp_types::PositionEncodingKind::UTF8 => LspPositionEncoding::Utf8,
        kind if kind == lsp_types::PositionEncodingKind::UTF32 => LspPositionEncoding::Utf32,
        _ => LspPositionEncoding::Utf16,
    };

    let completion_triggers = triggers(
        &caps
            .completion_provider
            .as_ref()
            .and_then(|p| p.trigger_characters.clone()),
    );
    let completion_resolve = caps
        .completion_provider
        .as_ref()
        .and_then(|p| p.resolve_provider)
        .unwrap_or(false);
    let signature_help_triggers = triggers(
        &caps
            .signature_help_provider
            .as_ref()
            .and_then(|p| p.trigger_characters.clone()),
    );

    let semantic_tokens_legend = caps.semantic_tokens_provider.as_ref().map(|provider| {
        let options = match provider {
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options) => options,
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                options,
            ) => &options.semantic_tokens_options,
        };

        let token_types = options
            .legend
            .token_types
            .iter()
            .map(|t| t.as_str().to_string())
            .collect::<Vec<_>>();
        let token_modifiers = options
            .legend
            .token_modifiers
            .iter()
            .map(|t| t.as_str().to_string())
            .collect::<Vec<_>>();

        LspSemanticTokensLegend {
            token_types,
            token_modifiers,
        }
    });

    let (semantic_tokens_range, semantic_tokens_full) = caps
        .semantic_tokens_provider
        .as_ref()
        .map(|provider| {
            let options = match provider {
                lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options) => {
                    options
                }
                lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                    options,
                ) => &options.semantic_tokens_options,
            };

            let range = options.range.unwrap_or(false);
            let full = options.full.as_ref().is_some_and(|full| match full {
                lsp_types::SemanticTokensFullOptions::Bool(enabled) => *enabled,
                lsp_types::SemanticTokensFullOptions::Delta { .. } => true,
            });

            (range, full)
        })
        .unwrap_or((false, false));

    LspServerCapabilities {
        position_encoding: encoding,
        hover: hover(&caps.hover_provider),
        definition: one_of_bool(&caps.definition_provider),
        references: one_of_bool(&caps.references_provider),
        document_symbols: one_of_bool(&caps.document_symbol_provider),
        workspace_symbols: one_of_bool(&caps.workspace_symbol_provider),
        code_action: code_action(&caps.code_action_provider),
        completion: caps.completion_provider.is_some(),
        signature_help: caps.signature_help_provider.is_some(),
        rename: one_of_bool(&caps.rename_provider),
        format: one_of_bool(&caps.document_formatting_provider),
        range_format: one_of_bool(&caps.document_range_formatting_provider),
        semantic_tokens: caps.semantic_tokens_provider.is_some(),
        semantic_tokens_range,
        semantic_tokens_full,
        semantic_tokens_legend,
        inlay_hints: one_of_bool(&caps.inlay_hint_provider),
        folding_range: folding(&caps.folding_range_provider),
        completion_resolve,
        completion_triggers,
        signature_help_triggers,
    }
}

pub(super) fn decode_semantic_tokens(
    tokens: Vec<lsp_types::SemanticToken>,
) -> Vec<LspSemanticToken> {
    let mut out = Vec::with_capacity(tokens.len().min(2048));
    let mut line = 0u32;
    let mut start = 0u32;

    for token in tokens {
        line = line.saturating_add(token.delta_line);
        if token.delta_line == 0 {
            start = start.saturating_add(token.delta_start);
        } else {
            start = token.delta_start;
        }

        out.push(LspSemanticToken {
            line,
            start,
            length: token.length,
            token_type: token.token_type,
            modifiers: token.token_modifiers_bitset,
        });
    }

    out
}

pub(super) fn inlay_hints_from_lsp(hints: Vec<lsp_types::InlayHint>) -> Vec<LspInlayHint> {
    let mut out = Vec::with_capacity(hints.len().min(128));

    for hint in hints.into_iter().take(512) {
        let label = match hint.label {
            lsp_types::InlayHintLabel::String(s) => s,
            lsp_types::InlayHintLabel::LabelParts(parts) => {
                parts.into_iter().map(|p| p.value).collect::<String>()
            }
        };

        let label = label.trim().to_string();
        if label.is_empty() {
            continue;
        }

        out.push(LspInlayHint {
            position: LspPosition {
                line: hint.position.line,
                character: hint.position.character,
            },
            label,
            padding_left: hint.padding_left.unwrap_or(false),
            padding_right: hint.padding_right.unwrap_or(false),
        });
    }

    out
}

pub(super) fn push_document_symbols(
    path: &PathBuf,
    symbols: &[lsp_types::DocumentSymbol],
    level: usize,
    out: &mut Vec<SymbolItem>,
) {
    for sym in symbols {
        out.push(SymbolItem {
            name: sym.name.clone(),
            detail: sym.detail.clone(),
            kind: symbol_kind_u32(sym.kind),
            level,
            path: path.clone(),
            line: sym.selection_range.start.line,
            column: sym.selection_range.start.character,
        });

        if let Some(children) = sym.children.as_ref() {
            push_document_symbols(path, children, level.saturating_add(1), out);
        }
    }
}

pub(super) fn symbol_item_from_symbol_information(
    fallback_path: Option<&PathBuf>,
    sym: lsp_types::SymbolInformation,
) -> Option<SymbolItem> {
    let path = sym
        .location
        .uri
        .to_file_path()
        .ok()
        .or_else(|| fallback_path.cloned())?;

    Some(SymbolItem {
        name: sym.name,
        detail: sym.container_name,
        kind: symbol_kind_u32(sym.kind),
        level: 0,
        path,
        line: sym.location.range.start.line,
        column: sym.location.range.start.character,
    })
}

pub(super) fn symbol_item_from_workspace_symbol(
    sym: lsp_types::WorkspaceSymbol,
) -> Option<SymbolItem> {
    match sym.location {
        lsp_types::OneOf::Left(loc) => {
            let path = loc.uri.to_file_path().ok()?;
            Some(SymbolItem {
                name: sym.name,
                detail: sym.container_name,
                kind: symbol_kind_u32(sym.kind),
                level: 0,
                path,
                line: loc.range.start.line,
                column: loc.range.start.character,
            })
        }
        lsp_types::OneOf::Right(loc) => {
            let path = loc.uri.to_file_path().ok()?;
            Some(SymbolItem {
                name: sym.name,
                detail: sym.container_name,
                kind: symbol_kind_u32(sym.kind),
                level: 0,
                path,
                line: 0,
                column: 0,
            })
        }
    }
}

pub(super) fn symbol_kind_u32(kind: lsp_types::SymbolKind) -> u32 {
    if kind == lsp_types::SymbolKind::FILE {
        1
    } else if kind == lsp_types::SymbolKind::MODULE {
        2
    } else if kind == lsp_types::SymbolKind::NAMESPACE {
        3
    } else if kind == lsp_types::SymbolKind::PACKAGE {
        4
    } else if kind == lsp_types::SymbolKind::CLASS {
        5
    } else if kind == lsp_types::SymbolKind::METHOD {
        6
    } else if kind == lsp_types::SymbolKind::PROPERTY {
        7
    } else if kind == lsp_types::SymbolKind::FIELD {
        8
    } else if kind == lsp_types::SymbolKind::CONSTRUCTOR {
        9
    } else if kind == lsp_types::SymbolKind::ENUM {
        10
    } else if kind == lsp_types::SymbolKind::INTERFACE {
        11
    } else if kind == lsp_types::SymbolKind::FUNCTION {
        12
    } else if kind == lsp_types::SymbolKind::VARIABLE {
        13
    } else if kind == lsp_types::SymbolKind::CONSTANT {
        14
    } else if kind == lsp_types::SymbolKind::STRING {
        15
    } else if kind == lsp_types::SymbolKind::NUMBER {
        16
    } else if kind == lsp_types::SymbolKind::BOOLEAN {
        17
    } else if kind == lsp_types::SymbolKind::ARRAY {
        18
    } else if kind == lsp_types::SymbolKind::OBJECT {
        19
    } else if kind == lsp_types::SymbolKind::KEY {
        20
    } else if kind == lsp_types::SymbolKind::NULL {
        21
    } else if kind == lsp_types::SymbolKind::ENUM_MEMBER {
        22
    } else if kind == lsp_types::SymbolKind::STRUCT {
        23
    } else if kind == lsp_types::SymbolKind::EVENT {
        24
    } else if kind == lsp_types::SymbolKind::OPERATOR {
        25
    } else if kind == lsp_types::SymbolKind::TYPE_PARAMETER {
        26
    } else {
        0
    }
}

pub(super) fn hover_text(hover: &lsp_types::Hover) -> Option<String> {
    let mut parts = Vec::new();
    match &hover.contents {
        lsp_types::HoverContents::Scalar(s) => push_marked_string(s, &mut parts),
        lsp_types::HoverContents::Array(items) => {
            for s in items {
                push_marked_string(s, &mut parts);
            }
        }
        lsp_types::HoverContents::Markup(m) => parts.push(m.value.clone()),
    }

    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub(super) fn signature_help_text(help: &lsp_types::SignatureHelp) -> Option<String> {
    let active_sig = help.active_signature.unwrap_or(0) as usize;
    let sig = help.signatures.get(active_sig)?;

    let active_param = sig.active_parameter.or(help.active_parameter).unwrap_or(0) as usize;

    let mut label = sig.label.clone();
    if let Some(params) = sig.parameters.as_ref() {
        if let Some(param) = params.get(active_param) {
            if let Some((start, end)) = parameter_label_range(&label, &param.label) {
                if start < end && end <= label.len() {
                    label = format!(
                        "{}[{}]{}",
                        &label[..start],
                        &label[start..end],
                        &label[end..]
                    );
                }
            }
        }
    }

    let mut lines = Vec::new();
    if help.signatures.len() > 1 {
        lines.push(format!(
            "{label} ({}/{})",
            active_sig + 1,
            help.signatures.len()
        ));
    } else {
        lines.push(label);
    }

    if let Some(doc) = sig.documentation.as_ref().and_then(documentation_text) {
        let doc = doc.trim();
        if !doc.is_empty() {
            let mut it = doc.lines();
            let first = it.next().unwrap_or_default();
            if !first.trim().is_empty() {
                lines.push(first.trim().to_string());
            }
        }
    }

    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub(super) fn documentation_text(doc: &lsp_types::Documentation) -> Option<String> {
    match doc {
        lsp_types::Documentation::String(s) => Some(s.clone()),
        lsp_types::Documentation::MarkupContent(m) => Some(m.value.clone()),
    }
}

pub(super) fn parameter_label_range(
    label: &str,
    param: &lsp_types::ParameterLabel,
) -> Option<(usize, usize)> {
    match param {
        lsp_types::ParameterLabel::Simple(s) => {
            let start = label.find(s)?;
            Some((start, start.saturating_add(s.len())))
        }
        lsp_types::ParameterLabel::LabelOffsets([start, end]) => {
            let start = utf16_offset_to_byte(label, *start);
            let end = utf16_offset_to_byte(label, *end);
            Some((start, end.max(start)))
        }
    }
}

pub(super) fn utf16_offset_to_byte(s: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in s.char_indices() {
        let next = units.saturating_add(ch.len_utf16() as u32);
        if next > offset {
            return byte;
        }
        units = next;
    }
    s.len()
}

pub(super) fn push_marked_string(s: &lsp_types::MarkedString, out: &mut Vec<String>) {
    match s {
        lsp_types::MarkedString::String(s) => out.push(s.clone()),
        lsp_types::MarkedString::LanguageString(ls) => out.push(ls.value.clone()),
    }
}

pub(super) fn definition_location(
    resp: lsp_types::GotoDefinitionResponse,
) -> Option<(PathBuf, u32, u32)> {
    match resp {
        lsp_types::GotoDefinitionResponse::Scalar(loc) => location_from_location(&loc),
        lsp_types::GotoDefinitionResponse::Array(locs) => {
            locs.first().and_then(location_from_location)
        }
        lsp_types::GotoDefinitionResponse::Link(links) => {
            links.first().and_then(location_from_link)
        }
    }
}

pub(super) fn location_from_location(loc: &lsp_types::Location) -> Option<(PathBuf, u32, u32)> {
    let path = loc.uri.to_file_path().ok()?;
    Some((path, loc.range.start.line, loc.range.start.character))
}

pub(super) fn location_from_link(link: &lsp_types::LocationLink) -> Option<(PathBuf, u32, u32)> {
    let path = link.target_uri.to_file_path().ok()?;
    let range = &link.target_selection_range;
    Some((path, range.start.line, range.start.character))
}

pub(super) fn completion_item_kind_u32(kind: lsp_types::CompletionItemKind) -> u32 {
    use lsp_types::CompletionItemKind as Kind;

    if kind == Kind::TEXT {
        1
    } else if kind == Kind::METHOD {
        2
    } else if kind == Kind::FUNCTION {
        3
    } else if kind == Kind::CONSTRUCTOR {
        4
    } else if kind == Kind::FIELD {
        5
    } else if kind == Kind::VARIABLE {
        6
    } else if kind == Kind::CLASS {
        7
    } else if kind == Kind::INTERFACE {
        8
    } else if kind == Kind::MODULE {
        9
    } else if kind == Kind::PROPERTY {
        10
    } else if kind == Kind::UNIT {
        11
    } else if kind == Kind::VALUE {
        12
    } else if kind == Kind::ENUM {
        13
    } else if kind == Kind::KEYWORD {
        14
    } else if kind == Kind::SNIPPET {
        15
    } else if kind == Kind::COLOR {
        16
    } else if kind == Kind::FILE {
        17
    } else if kind == Kind::REFERENCE {
        18
    } else if kind == Kind::FOLDER {
        19
    } else if kind == Kind::ENUM_MEMBER {
        20
    } else if kind == Kind::CONSTANT {
        21
    } else if kind == Kind::STRUCT {
        22
    } else if kind == Kind::EVENT {
        23
    } else if kind == Kind::OPERATOR {
        24
    } else if kind == Kind::TYPE_PARAMETER {
        25
    } else {
        0
    }
}

pub(super) fn insert_text_format(fmt: Option<lsp_types::InsertTextFormat>) -> LspInsertTextFormat {
    match fmt {
        Some(fmt) if fmt == lsp_types::InsertTextFormat::SNIPPET => LspInsertTextFormat::Snippet,
        _ => LspInsertTextFormat::PlainText,
    }
}

pub(super) fn completion_items(
    resp: lsp_types::CompletionResponse,
) -> (Vec<LspCompletionItem>, bool) {
    let (items, mut is_incomplete) = match resp {
        lsp_types::CompletionResponse::Array(items) => (items, false),
        lsp_types::CompletionResponse::List(list) => (list.items, list.is_incomplete),
    };

    let mut out = Vec::with_capacity(items.len().min(64));
    let mut next_id = 1u64;
    let truncated = items.len() > 200;

    for item in items.into_iter().take(200) {
        let label = item.label.trim().to_string();
        if label.is_empty() {
            continue;
        }

        let detail = item.detail;
        let kind = item.kind.map(completion_item_kind_u32).filter(|k| *k != 0);
        let documentation = item
            .documentation
            .as_ref()
            .and_then(documentation_text)
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        let sort_text = item.sort_text;
        let filter_text = item.filter_text;
        let insert_text_format = insert_text_format(item.insert_text_format);
        let additional_text_edits = item
            .additional_text_edits
            .unwrap_or_default()
            .into_iter()
            .map(|edit| LspTextEdit {
                range: range_from_lsp(edit.range),
                new_text: edit.new_text,
            })
            .collect::<Vec<_>>();
        let command = item.command.map(command_from_lsp);
        let data = item.data;

        let mut insert_range = None;
        let mut replace_range = None;
        let mut insert_text = None;

        if let Some(text_edit) = item.text_edit {
            match text_edit {
                lsp_types::CompletionTextEdit::Edit(edit) => {
                    let range = range_from_lsp(edit.range);
                    insert_range = Some(range);
                    replace_range = Some(range);
                    insert_text = Some(edit.new_text);
                }
                lsp_types::CompletionTextEdit::InsertAndReplace(edit) => {
                    insert_range = Some(range_from_lsp(edit.insert));
                    replace_range = Some(range_from_lsp(edit.replace));
                    insert_text = Some(edit.new_text);
                }
            }
        }

        if insert_text.is_none() {
            insert_text = item.insert_text;
        }

        let insert_text = insert_text.unwrap_or_else(|| label.clone());

        let id = next_id;
        next_id = next_id.saturating_add(1);

        out.push(LspCompletionItem {
            id,
            label,
            detail,
            kind,
            documentation,
            insert_text,
            insert_text_format,
            insert_range,
            replace_range,
            sort_text,
            filter_text,
            additional_text_edits,
            command,
            data,
        });
    }

    if truncated {
        is_incomplete = true;
    }

    (out, is_incomplete)
}

pub(super) fn completion_item_kind_from_u32(kind: u32) -> Option<lsp_types::CompletionItemKind> {
    serde_json::from_value(Value::from(kind as i64)).ok()
}

pub(super) fn completion_item_to_lsp(item: &LspCompletionItem) -> lsp_types::CompletionItem {
    let mut out = lsp_types::CompletionItem {
        label: item.label.clone(),
        detail: item.detail.clone(),
        kind: item.kind.and_then(completion_item_kind_from_u32),
        sort_text: item.sort_text.clone(),
        filter_text: item.filter_text.clone(),
        insert_text: Some(item.insert_text.clone()),
        insert_text_format: Some(match item.insert_text_format {
            LspInsertTextFormat::PlainText => lsp_types::InsertTextFormat::PLAIN_TEXT,
            LspInsertTextFormat::Snippet => lsp_types::InsertTextFormat::SNIPPET,
        }),
        data: item.data.clone(),
        ..Default::default()
    };

    if let Some(range) = item.replace_range {
        out.text_edit = Some(lsp_types::CompletionTextEdit::Edit(lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: range.start.line,
                    character: range.start.character,
                },
                end: lsp_types::Position {
                    line: range.end.line,
                    character: range.end.character,
                },
            },
            new_text: item.insert_text.clone(),
        }));
    }

    out
}

pub(super) fn code_actions_from_lsp(
    items: Vec<lsp_types::CodeActionOrCommand>,
) -> Vec<LspCodeAction> {
    let mut out = Vec::with_capacity(items.len().min(64));

    for item in items.into_iter().take(200) {
        match item {
            lsp_types::CodeActionOrCommand::Command(cmd) => {
                let title = cmd.title.clone();
                out.push(LspCodeAction {
                    title,
                    kind: None,
                    is_preferred: false,
                    edit: None,
                    command: Some(command_from_lsp(cmd)),
                });
            }
            lsp_types::CodeActionOrCommand::CodeAction(action) => {
                if action.disabled.is_some() {
                    continue;
                }

                let edit = action.edit.map(workspace_edit_from_lsp);
                let command = action.command.map(command_from_lsp);

                out.push(LspCodeAction {
                    title: action.title,
                    kind: action.kind.map(|k| k.as_str().to_string()),
                    is_preferred: action.is_preferred.unwrap_or(false),
                    edit,
                    command,
                });
            }
        }
    }

    out
}

pub(super) fn command_from_lsp(command: lsp_types::Command) -> LspCommand {
    LspCommand {
        command: command.command,
        arguments: command.arguments.unwrap_or_default(),
    }
}

pub(super) fn workspace_edit_from_lsp(edit: lsp_types::WorkspaceEdit) -> LspWorkspaceEdit {
    let mut by_path: FxHashMap<PathBuf, Vec<LspTextEdit>> = FxHashMap::default();
    let mut resource_ops: Vec<LspResourceOp> = Vec::new();

    if let Some(changes) = edit.changes {
        for (uri, edits) in changes {
            let Ok(path) = uri.to_file_path() else {
                continue;
            };
            let out = by_path.entry(path).or_default();
            for edit in edits {
                out.push(LspTextEdit {
                    range: range_from_lsp(edit.range),
                    new_text: edit.new_text,
                });
            }
        }
    }

    if let Some(doc_changes) = edit.document_changes {
        match doc_changes {
            lsp_types::DocumentChanges::Edits(edits) => {
                for doc in edits {
                    merge_text_document_edits(&mut by_path, doc);
                }
            }
            lsp_types::DocumentChanges::Operations(ops) => {
                for op in ops {
                    match op {
                        lsp_types::DocumentChangeOperation::Edit(doc) => {
                            merge_text_document_edits(&mut by_path, doc);
                        }
                        lsp_types::DocumentChangeOperation::Op(op) => match op {
                            lsp_types::ResourceOp::Create(create) => {
                                if let Ok(path) = create.uri.to_file_path() {
                                    let overwrite = create
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.overwrite)
                                        .unwrap_or(false);
                                    let ignore_if_exists = create
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.ignore_if_exists)
                                        .unwrap_or(false);
                                    resource_ops.push(LspResourceOp::CreateFile {
                                        path,
                                        overwrite,
                                        ignore_if_exists,
                                    });
                                }
                            }
                            lsp_types::ResourceOp::Rename(rename) => {
                                let Ok(old_path) = rename.old_uri.to_file_path() else {
                                    continue;
                                };
                                let Ok(new_path) = rename.new_uri.to_file_path() else {
                                    continue;
                                };
                                let overwrite = rename
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.overwrite)
                                    .unwrap_or(false);
                                let ignore_if_exists = rename
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.ignore_if_exists)
                                    .unwrap_or(false);
                                resource_ops.push(LspResourceOp::RenameFile {
                                    old_path,
                                    new_path,
                                    overwrite,
                                    ignore_if_exists,
                                });
                            }
                            lsp_types::ResourceOp::Delete(delete) => {
                                if let Ok(path) = delete.uri.to_file_path() {
                                    let recursive = delete
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.recursive)
                                        .unwrap_or(false);
                                    let ignore_if_not_exists = delete
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.ignore_if_not_exists)
                                        .unwrap_or(false);
                                    resource_ops.push(LspResourceOp::DeleteFile {
                                        path,
                                        recursive,
                                        ignore_if_not_exists,
                                    });
                                }
                            }
                        },
                    }
                }
            }
        }
    }

    let changes = by_path
        .into_iter()
        .filter_map(|(path, edits)| {
            if edits.is_empty() {
                None
            } else {
                Some(LspWorkspaceFileEdit { path, edits })
            }
        })
        .collect();

    LspWorkspaceEdit {
        changes,
        resource_ops,
    }
}

pub(super) fn merge_text_document_edits(
    by_path: &mut FxHashMap<PathBuf, Vec<LspTextEdit>>,
    doc: lsp_types::TextDocumentEdit,
) {
    let Ok(path) = doc.text_document.uri.to_file_path() else {
        return;
    };

    let out = by_path.entry(path).or_default();
    for edit in doc.edits {
        let edit = match edit {
            lsp_types::OneOf::Left(edit) => edit,
            lsp_types::OneOf::Right(edit) => edit.text_edit,
        };
        out.push(LspTextEdit {
            range: range_from_lsp(edit.range),
            new_text: edit.new_text,
        });
    }
}

pub(super) fn diagnostics_from_params(
    params: lsp_types::PublishDiagnosticsParams,
) -> Option<(PathBuf, Vec<ProblemItem>)> {
    let path = params.uri.to_file_path().ok()?;

    let mut items = Vec::with_capacity(params.diagnostics.len());
    for diag in params.diagnostics {
        let severity = match diag.severity {
            Some(lsp_types::DiagnosticSeverity::ERROR) => ProblemSeverity::Error,
            Some(lsp_types::DiagnosticSeverity::WARNING) => ProblemSeverity::Warning,
            Some(lsp_types::DiagnosticSeverity::INFORMATION) => ProblemSeverity::Information,
            Some(lsp_types::DiagnosticSeverity::HINT) => ProblemSeverity::Hint,
            _ => ProblemSeverity::Information,
        };

        items.push(ProblemItem {
            path: path.clone(),
            range: ProblemRange {
                start_line: diag.range.start.line,
                start_col: diag.range.start.character,
                end_line: diag.range.end.line,
                end_col: diag.range.end.character,
            },
            severity,
            message: diag.message,
            source: diag.source,
        });
    }

    Some((path, items))
}

pub(super) fn text_change_event(
    change: LspTextChange,
) -> lsp_types::TextDocumentContentChangeEvent {
    match change.range {
        Some(range) => lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: range.start.line,
                    character: range.start.character,
                },
                end: lsp_types::Position {
                    line: range.end.line,
                    character: range.end.character,
                },
            }),
            range_length: None,
            text: change.text,
        },
        None => lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: change.text,
        },
    }
}

pub(super) fn range_from_lsp(range: lsp_types::Range) -> LspRange {
    LspRange {
        start: LspPosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: LspPosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

pub(super) fn lsp_version(version: u64) -> i32 {
    i32::try_from(version).unwrap_or(i32::MAX)
}

pub(super) fn path_to_url(path: &Path) -> Option<lsp_types::Url> {
    lsp_types::Url::from_file_path(path).ok()
}

pub(super) fn workspace_folders_for_root(root: &Path) -> Option<Vec<lsp_types::WorkspaceFolder>> {
    let uri = path_to_url(root)?;
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace")
        .to_string();
    Some(vec![lsp_types::WorkspaceFolder { uri, name }])
}

pub(super) fn client_capabilities() -> lsp_types::ClientCapabilities {
    let hover = lsp_types::HoverClientCapabilities {
        dynamic_registration: Some(false),
        content_format: Some(vec![
            lsp_types::MarkupKind::Markdown,
            lsp_types::MarkupKind::PlainText,
        ]),
    };

    let completion = lsp_types::CompletionClientCapabilities {
        completion_item: Some(lsp_types::CompletionItemCapability {
            snippet_support: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let signature_help = lsp_types::SignatureHelpClientCapabilities {
        context_support: Some(true),
        ..Default::default()
    };

    let document_symbol = lsp_types::DocumentSymbolClientCapabilities {
        hierarchical_document_symbol_support: Some(true),
        ..Default::default()
    };

    let workspace_symbol = lsp_types::WorkspaceSymbolClientCapabilities::default();
    let semantic_tokens = lsp_types::SemanticTokensClientCapabilities {
        dynamic_registration: Some(false),
        requests: lsp_types::SemanticTokensClientCapabilitiesRequests {
            range: Some(true),
            full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
        },
        token_types: vec![
            lsp_types::SemanticTokenType::NAMESPACE,
            lsp_types::SemanticTokenType::TYPE,
            lsp_types::SemanticTokenType::CLASS,
            lsp_types::SemanticTokenType::ENUM,
            lsp_types::SemanticTokenType::INTERFACE,
            lsp_types::SemanticTokenType::STRUCT,
            lsp_types::SemanticTokenType::TYPE_PARAMETER,
            lsp_types::SemanticTokenType::PARAMETER,
            lsp_types::SemanticTokenType::VARIABLE,
            lsp_types::SemanticTokenType::PROPERTY,
            lsp_types::SemanticTokenType::ENUM_MEMBER,
            lsp_types::SemanticTokenType::EVENT,
            lsp_types::SemanticTokenType::FUNCTION,
            lsp_types::SemanticTokenType::METHOD,
            lsp_types::SemanticTokenType::MACRO,
            lsp_types::SemanticTokenType::KEYWORD,
            lsp_types::SemanticTokenType::MODIFIER,
            lsp_types::SemanticTokenType::COMMENT,
            lsp_types::SemanticTokenType::STRING,
            lsp_types::SemanticTokenType::NUMBER,
            lsp_types::SemanticTokenType::REGEXP,
            lsp_types::SemanticTokenType::OPERATOR,
            lsp_types::SemanticTokenType::DECORATOR,
        ],
        token_modifiers: vec![
            lsp_types::SemanticTokenModifier::DECLARATION,
            lsp_types::SemanticTokenModifier::DEFINITION,
            lsp_types::SemanticTokenModifier::READONLY,
            lsp_types::SemanticTokenModifier::STATIC,
            lsp_types::SemanticTokenModifier::DEPRECATED,
            lsp_types::SemanticTokenModifier::ABSTRACT,
            lsp_types::SemanticTokenModifier::ASYNC,
            lsp_types::SemanticTokenModifier::MODIFICATION,
            lsp_types::SemanticTokenModifier::DOCUMENTATION,
            lsp_types::SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
        formats: vec![lsp_types::TokenFormat::RELATIVE],
        overlapping_token_support: Some(false),
        multiline_token_support: Some(false),
        server_cancel_support: Some(true),
        augments_syntax_tokens: Some(true),
    };
    let inlay_hint = lsp_types::InlayHintClientCapabilities {
        dynamic_registration: Some(false),
        resolve_support: None,
    };
    let general = lsp_types::GeneralClientCapabilities {
        position_encodings: Some(vec![
            lsp_types::PositionEncodingKind::UTF16,
            lsp_types::PositionEncodingKind::UTF8,
            lsp_types::PositionEncodingKind::UTF32,
        ]),
        ..Default::default()
    };

    lsp_types::ClientCapabilities {
        workspace: Some(lsp_types::WorkspaceClientCapabilities {
            apply_edit: Some(true),
            configuration: Some(true),
            symbol: Some(workspace_symbol),
            ..Default::default()
        }),
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            hover: Some(hover),
            completion: Some(completion),
            signature_help: Some(signature_help),
            document_symbol: Some(document_symbol),
            semantic_tokens: Some(semantic_tokens),
            inlay_hint: Some(inlay_hint),
            ..Default::default()
        }),
        general: Some(general),
        ..Default::default()
    }
}

pub(super) fn language_id_for_path(path: &Path) -> &'static str {
    LspLanguage::from_path(path)
        .map(LspLanguage::language_id)
        .unwrap_or("plaintext")
}
