use lsp_server::{Message, Notification, Request, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StubPositionEncoding {
    Utf8,
    Utf16,
    Utf32,
}

impl StubPositionEncoding {
    fn from_env() -> Self {
        let raw = std::env::var("ZCODE_LSP_STUB_POSITION_ENCODING").unwrap_or_default();
        match raw.trim().to_ascii_lowercase().as_str() {
            "utf-8" | "utf8" => Self::Utf8,
            "utf-32" | "utf32" => Self::Utf32,
            _ => Self::Utf16,
        }
    }

    fn to_lsp_kind(self) -> lsp_types::PositionEncodingKind {
        match self {
            Self::Utf8 => lsp_types::PositionEncodingKind::UTF8,
            Self::Utf16 => lsp_types::PositionEncodingKind::UTF16,
            Self::Utf32 => lsp_types::PositionEncodingKind::UTF32,
        }
    }

    fn col_units_for_str(self, s: &str) -> u32 {
        match self {
            Self::Utf8 => s.chars().map(|ch| ch.len_utf8() as u32).sum(),
            Self::Utf16 => s.chars().map(|ch| ch.len_utf16() as u32).sum(),
            Self::Utf32 => s.chars().count() as u32,
        }
    }
}

struct Trace {
    file: Option<std::fs::File>,
}

impl Trace {
    fn from_env() -> Self {
        let path = std::env::var_os("ZCODE_LSP_STUB_TRACE_PATH")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        let file = path.and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        Self { file }
    }

    fn log(&mut self, line: &str) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut trace = Trace::from_env();
    let mut workspace_root: Option<PathBuf> = None;
    let mut next_server_request_id: i32 = 1;

    loop {
        let msg = match Message::read(&mut reader) {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(_) => break,
        };

        match msg {
            Message::Request(req) => {
                trace.log(&format!("request {}", req.method));
                let (resp, new_root, extra_msgs) =
                    handle_request(req, workspace_root.as_ref(), &mut next_server_request_id);
                if let Some(root) = new_root {
                    workspace_root = Some(root);
                }
                for msg in extra_msgs {
                    if send(&mut writer, msg).is_err() {
                        return;
                    }
                }
                if send(&mut writer, Message::Response(resp)).is_err() {
                    break;
                }
            }
            Message::Notification(not) => {
                trace.log(&format!("notification {}", not.method));
                if let Some(uri) = notification_uri(&not) {
                    let marker = match not.method.as_str() {
                        m if m == lsp_types::notification::DidOpenTextDocument::METHOD => {
                            Some("didOpen")
                        }
                        m if m == lsp_types::notification::DidChangeTextDocument::METHOD => {
                            Some("didChange")
                        }
                        m if m == lsp_types::notification::DidSaveTextDocument::METHOD => {
                            Some("didSave")
                        }
                        _ => None,
                    };

                    if let Some(marker) = marker {
                        trace.log(marker);
                        let params = publish_diagnostics(uri, marker);
                        let msg = Message::Notification(Notification::new(
                            lsp_types::notification::PublishDiagnostics::METHOD.to_string(),
                            params,
                        ));
                        if send(&mut writer, msg).is_err() {
                            break;
                        }
                    }
                }

                if not.method == lsp_types::notification::Exit::METHOD {
                    break;
                }
            }
            Message::Response(_) => {}
        }
    }
}

fn send(writer: &mut BufWriter<std::io::StdoutLock<'_>>, msg: Message) -> std::io::Result<()> {
    msg.write(writer)?;
    writer.flush()
}

fn handle_request(
    req: Request,
    root: Option<&PathBuf>,
    next_server_request_id: &mut i32,
) -> (Response, Option<PathBuf>, Vec<Message>) {
    let position_encoding = StubPositionEncoding::from_env();
    match req.method.as_str() {
        m if m == lsp_types::request::Initialize::METHOD => {
            let params = serde_json::from_value::<lsp_types::InitializeParams>(req.params)
                .unwrap_or_default();
            let root = workspace_root_from_initialize(&params);

            let capabilities = lsp_types::ServerCapabilities {
                position_encoding: Some(position_encoding.to_lsp_kind()),
                text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
                    lsp_types::TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
                definition_provider: Some(lsp_types::OneOf::Left(true)),
                references_provider: Some(lsp_types::OneOf::Left(true)),
                document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
                workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
                code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
                    commands: vec!["stub.insert_cmd".to_string()],
                    ..Default::default()
                }),
                completion_provider: Some(lsp_types::CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(lsp_types::SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
                }),
                folding_range_provider: Some(lsp_types::FoldingRangeProviderCapability::Simple(
                    true,
                )),
                semantic_tokens_provider: Some(
                    lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        lsp_types::SemanticTokensOptions {
                            work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(
                            ),
                            legend: lsp_types::SemanticTokensLegend {
                                token_types: vec![
                                    lsp_types::SemanticTokenType::KEYWORD,
                                    lsp_types::SemanticTokenType::FUNCTION,
                                ],
                                token_modifiers: Vec::new(),
                            },
                            range: Some(true),
                            full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                inlay_hint_provider: Some(lsp_types::OneOf::Left(true)),
                document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
                document_range_formatting_provider: Some(lsp_types::OneOf::Left(true)),
                rename_provider: Some(lsp_types::OneOf::Left(true)),
                ..Default::default()
            };

            let result = lsp_types::InitializeResult {
                capabilities,
                server_info: Some(lsp_types::ServerInfo {
                    name: "zcode-lsp-stub".to_string(),
                    version: Some("0.1".to_string()),
                }),
            };

            (Response::new_ok(req.id, result), root, Vec::new())
        }
        m if m == lsp_types::request::HoverRequest::METHOD => {
            let params = serde_json::from_value::<lsp_types::HoverParams>(req.params)
                .unwrap_or_else(|_| lsp_types::HoverParams {
                    text_document_position_params: lsp_types::TextDocumentPositionParams {
                        text_document: lsp_types::TextDocumentIdentifier {
                            uri: lsp_types::Url::parse("file:///").unwrap(),
                        },
                        position: lsp_types::Position::new(0, 0),
                    },
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                });

            if let Ok(delay) = std::env::var("ZCODE_LSP_STUB_HOVER_DELAY_MS") {
                if let Ok(delay) = delay.trim().parse::<u64>() {
                    if delay > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                }
            }

            let pos = params.text_document_position_params.position;
            let text = format!("stub hover @{}:{}", pos.line, pos.character);
            let hover = lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: text,
                }),
                range: None,
            };

            (Response::new_ok(req.id, Some(hover)), None, Vec::new())
        }
        m if m == lsp_types::request::GotoDefinition::METHOD => {
            let target = root
                .cloned()
                .map(|root| root.join("definition_target.rs"))
                .and_then(|path| lsp_types::Url::from_file_path(path).ok());

            let resp = target.map(|uri| {
                lsp_types::GotoDefinitionResponse::Scalar(lsp_types::Location {
                    uri,
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                })
            });

            (Response::new_ok(req.id, resp), None, Vec::new())
        }
        m if m == lsp_types::request::Completion::METHOD => {
            let is_incomplete = std::env::var("ZCODE_LSP_STUB_COMPLETION_INCOMPLETE")
                .ok()
                .is_some_and(|v| {
                    let v = v.trim();
                    v == "1" || v.eq_ignore_ascii_case("true")
                });

            let items = vec![
                lsp_types::CompletionItem {
                    label: "stubItem".to_string(),
                    detail: Some("from stub".to_string()),
                    insert_text: Some("stubItem".to_string()),
                    sort_text: Some("0".to_string()),
                    data: Some(json!({ "id": 1 })),
                    insert_text_format: Some(lsp_types::InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                },
                lsp_types::CompletionItem {
                    label: "stubSnippet".to_string(),
                    detail: Some("snippet".to_string()),
                    insert_text: Some("stubFn(${1:arg})$0".to_string()),
                    sort_text: Some("1".to_string()),
                    data: Some(json!({ "id": 2 })),
                    insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
                    ..Default::default()
                },
                lsp_types::CompletionItem {
                    label: "stubItem2".to_string(),
                    detail: Some("from stub".to_string()),
                    insert_text: Some("stubItem2".to_string()),
                    sort_text: Some("2".to_string()),
                    data: Some(json!({ "id": 3 })),
                    insert_text_format: Some(lsp_types::InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                },
            ];

            let list = lsp_types::CompletionList {
                is_incomplete,
                items,
            };

            (
                Response::new_ok(req.id, Some(lsp_types::CompletionResponse::List(list))),
                None,
                Vec::new(),
            )
        }
        m if m == lsp_types::request::ResolveCompletionItem::METHOD => {
            let item =
                serde_json::from_value::<lsp_types::CompletionItem>(req.params).unwrap_or_default();
            let id = item
                .data
                .as_ref()
                .and_then(|data| data.get("id"))
                .and_then(|id| id.as_i64())
                .unwrap_or(0);

            let mut out = item;
            out.detail = Some(format!("resolved:{id}"));
            out.documentation = Some(lsp_types::Documentation::String(format!(
                "stub resolved docs for {id}"
            )));
            if id == 2 {
                out.additional_text_edits = Some(vec![lsp_types::TextEdit {
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                    new_text: "use auto_import;\n".to_string(),
                }]);
            }

            (Response::new_ok(req.id, out), None, Vec::new())
        }
        m if m == lsp_types::request::SignatureHelpRequest::METHOD => {
            let params = serde_json::from_value::<lsp_types::SignatureHelpParams>(req.params)
                .unwrap_or_else(|_| lsp_types::SignatureHelpParams {
                    context: None,
                    text_document_position_params: lsp_types::TextDocumentPositionParams {
                        text_document: lsp_types::TextDocumentIdentifier {
                            uri: lsp_types::Url::parse("file:///").unwrap(),
                        },
                        position: lsp_types::Position::new(0, 0),
                    },
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                });

            if let Ok(delay) = std::env::var("ZCODE_LSP_STUB_SIGNATURE_HELP_DELAY_MS") {
                if let Ok(delay) = delay.trim().parse::<u64>() {
                    if delay > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                }
            }

            let signature = lsp_types::SignatureInformation {
                label: "fn from(value: T) -> String".to_string(),
                documentation: Some(lsp_types::Documentation::String(
                    "Converts to this type from the input type.".to_string(),
                )),
                parameters: Some(vec![lsp_types::ParameterInformation {
                    label: lsp_types::ParameterLabel::Simple("value: T".to_string()),
                    documentation: None,
                }]),
                active_parameter: Some(0),
            };

            let help = lsp_types::SignatureHelp {
                signatures: vec![signature],
                active_signature: Some(0),
                active_parameter: Some(0),
            };

            (
                Response::new_ok(req.id, Some(help)),
                params
                    .text_document_position_params
                    .text_document
                    .uri
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.parent().map(|dir| dir.to_path_buf())),
                Vec::new(),
            )
        }
        m if m == lsp_types::request::FoldingRangeRequest::METHOD => {
            let _params = serde_json::from_value::<lsp_types::FoldingRangeParams>(req.params)
                .unwrap_or_else(|_| lsp_types::FoldingRangeParams {
                    text_document: lsp_types::TextDocumentIdentifier {
                        uri: lsp_types::Url::parse("file:///").unwrap(),
                    },
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                    partial_result_params: lsp_types::PartialResultParams::default(),
                });

            let ranges = vec![lsp_types::FoldingRange {
                start_line: 0,
                start_character: None,
                end_line: 2,
                end_character: None,
                kind: None,
                collapsed_text: None,
            }];

            (Response::new_ok(req.id, Some(ranges)), None, Vec::new())
        }
        m if m == lsp_types::request::SemanticTokensFullRequest::METHOD => {
            let params = serde_json::from_value::<lsp_types::SemanticTokensParams>(req.params)
                .unwrap_or_else(|_| lsp_types::SemanticTokensParams {
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                    partial_result_params: lsp_types::PartialResultParams::default(),
                    text_document: lsp_types::TextDocumentIdentifier {
                        uri: lsp_types::Url::parse("file:///").unwrap(),
                    },
                });

            let keyword_len = position_encoding.col_units_for_str("fn");
            let fn_name_start = position_encoding.col_units_for_str("fn ");
            let fn_name_len = position_encoding.col_units_for_str("main");

            let data = vec![
                lsp_types::SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: keyword_len,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                lsp_types::SemanticToken {
                    delta_line: 0,
                    delta_start: fn_name_start,
                    length: fn_name_len,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
            ];

            let tokens = lsp_types::SemanticTokens {
                result_id: None,
                data,
            };

            (
                Response::new_ok(
                    req.id,
                    Some(lsp_types::SemanticTokensResult::Tokens(tokens)),
                ),
                params
                    .text_document
                    .uri
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.parent().map(|dir| dir.to_path_buf())),
                Vec::new(),
            )
        }
        m if m == lsp_types::request::SemanticTokensRangeRequest::METHOD => {
            let params = serde_json::from_value::<lsp_types::SemanticTokensRangeParams>(req.params)
                .unwrap_or_else(|_| lsp_types::SemanticTokensRangeParams {
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                    partial_result_params: lsp_types::PartialResultParams::default(),
                    text_document: lsp_types::TextDocumentIdentifier {
                        uri: lsp_types::Url::parse("file:///").unwrap(),
                    },
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(1, 0),
                    ),
                });

            let target_line = params.range.start.line;
            let keyword_len = position_encoding.col_units_for_str("fn");
            let fn_name_start = position_encoding.col_units_for_str("fn ");
            let fn_name_len = position_encoding.col_units_for_str("main");

            let data = vec![
                lsp_types::SemanticToken {
                    delta_line: target_line,
                    delta_start: 0,
                    length: keyword_len,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                lsp_types::SemanticToken {
                    delta_line: 0,
                    delta_start: fn_name_start,
                    length: fn_name_len,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
            ];

            let tokens = lsp_types::SemanticTokens {
                result_id: None,
                data,
            };

            (
                Response::new_ok(
                    req.id,
                    Some(lsp_types::SemanticTokensRangeResult::Tokens(tokens)),
                ),
                params
                    .text_document
                    .uri
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.parent().map(|dir| dir.to_path_buf())),
                Vec::new(),
            )
        }
        m if m == lsp_types::request::InlayHintRequest::METHOD => {
            let params = serde_json::from_value::<lsp_types::InlayHintParams>(req.params)
                .unwrap_or_else(|_| lsp_types::InlayHintParams {
                    work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
                    text_document: lsp_types::TextDocumentIdentifier {
                        uri: lsp_types::Url::parse("file:///").unwrap(),
                    },
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(1, 0),
                    ),
                });

            let mut out = Vec::new();
            for line in params.range.start.line..params.range.end.line {
                out.push(lsp_types::InlayHint {
                    position: lsp_types::Position::new(line, 0),
                    label: lsp_types::InlayHintLabel::String(format!(": hint{line}")),
                    kind: None,
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(false),
                    padding_right: Some(false),
                    data: None,
                });
            }

            (Response::new_ok(req.id, Some(out)), None, Vec::new())
        }
        m if m == lsp_types::request::Shutdown::METHOD => {
            (Response::new_ok(req.id, ()), None, Vec::new())
        }
        m if m == lsp_types::request::References::METHOD => {
            let params = match serde_json::from_value::<lsp_types::ReferenceParams>(req.params) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        Response::new_err(
                            req.id,
                            lsp_server::ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        ),
                        None,
                        Vec::new(),
                    );
                }
            };

            let uri = params.text_document_position.text_document.uri;
            let mut locations = Vec::with_capacity(2);
            locations.push(lsp_types::Location {
                uri: uri.clone(),
                range: lsp_types::Range::new(
                    lsp_types::Position::new(0, 0),
                    lsp_types::Position::new(0, 0),
                ),
            });

            let b_uri = uri
                .to_file_path()
                .ok()
                .and_then(|path| path.parent().map(|dir| dir.join("b.rs")))
                .and_then(|path| lsp_types::Url::from_file_path(path).ok())
                .or_else(|| {
                    root.and_then(|root| lsp_types::Url::from_file_path(root.join("b.rs")).ok())
                });
            if let Some(uri) = b_uri {
                locations.push(lsp_types::Location {
                    uri,
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                });
            }

            (Response::new_ok(req.id, Some(locations)), None, Vec::new())
        }
        m if m == lsp_types::request::DocumentSymbolRequest::METHOD => {
            let _params =
                match serde_json::from_value::<lsp_types::DocumentSymbolParams>(req.params) {
                    Ok(params) => params,
                    Err(err) => {
                        return (
                            Response::new_err(
                                req.id,
                                lsp_server::ErrorCode::InvalidParams as i32,
                                format!("invalid params: {err}"),
                            ),
                            None,
                            Vec::new(),
                        );
                    }
                };

            #[allow(deprecated)]
            let child = lsp_types::DocumentSymbol {
                name: "stub_child".to_string(),
                detail: Some("()".to_string()),
                kind: lsp_types::SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range: lsp_types::Range::new(
                    lsp_types::Position::new(1, 0),
                    lsp_types::Position::new(1, 0),
                ),
                selection_range: lsp_types::Range::new(
                    lsp_types::Position::new(1, 0),
                    lsp_types::Position::new(1, 0),
                ),
                children: None,
            };

            #[allow(deprecated)]
            let parent = lsp_types::DocumentSymbol {
                name: "stub_mod".to_string(),
                detail: None,
                kind: lsp_types::SymbolKind::MODULE,
                tags: None,
                deprecated: None,
                range: lsp_types::Range::new(
                    lsp_types::Position::new(0, 0),
                    lsp_types::Position::new(0, 0),
                ),
                selection_range: lsp_types::Range::new(
                    lsp_types::Position::new(0, 0),
                    lsp_types::Position::new(0, 0),
                ),
                children: Some(vec![child]),
            };

            #[allow(deprecated)]
            let second = lsp_types::DocumentSymbol {
                name: "stub_second".to_string(),
                detail: None,
                kind: lsp_types::SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range: lsp_types::Range::new(
                    lsp_types::Position::new(2, 0),
                    lsp_types::Position::new(2, 0),
                ),
                selection_range: lsp_types::Range::new(
                    lsp_types::Position::new(2, 0),
                    lsp_types::Position::new(2, 0),
                ),
                children: None,
            };

            let resp = lsp_types::DocumentSymbolResponse::Nested(vec![parent, second]);
            (Response::new_ok(req.id, Some(resp)), None, Vec::new())
        }
        m if m == lsp_types::request::WorkspaceSymbolRequest::METHOD => {
            let _params =
                match serde_json::from_value::<lsp_types::WorkspaceSymbolParams>(req.params) {
                    Ok(params) => params,
                    Err(err) => {
                        return (
                            Response::new_err(
                                req.id,
                                lsp_server::ErrorCode::InvalidParams as i32,
                                format!("invalid params: {err}"),
                            ),
                            None,
                            Vec::new(),
                        );
                    }
                };

            let Some(root) = root.cloned() else {
                return (
                    Response::new_ok(req.id, Option::<lsp_types::WorkspaceSymbolResponse>::None),
                    None,
                    Vec::new(),
                );
            };

            let a_uri = lsp_types::Url::from_file_path(root.join("a.rs")).ok();
            let b_uri = lsp_types::Url::from_file_path(root.join("b.rs")).ok();
            let Some(a_uri) = a_uri else {
                return (
                    Response::new_ok(req.id, Option::<lsp_types::WorkspaceSymbolResponse>::None),
                    None,
                    Vec::new(),
                );
            };
            let Some(b_uri) = b_uri else {
                return (
                    Response::new_ok(req.id, Option::<lsp_types::WorkspaceSymbolResponse>::None),
                    None,
                    Vec::new(),
                );
            };

            let a = lsp_types::WorkspaceSymbol {
                name: "stub_ws_a".to_string(),
                kind: lsp_types::SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: lsp_types::OneOf::Left(lsp_types::Location {
                    uri: a_uri,
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                }),
                data: None,
            };

            let b = lsp_types::WorkspaceSymbol {
                name: "stub_ws_b".to_string(),
                kind: lsp_types::SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: lsp_types::OneOf::Left(lsp_types::Location {
                    uri: b_uri,
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                }),
                data: None,
            };

            let resp = lsp_types::WorkspaceSymbolResponse::Nested(vec![a, b]);
            (Response::new_ok(req.id, Some(resp)), None, Vec::new())
        }
        m if m == lsp_types::request::CodeActionRequest::METHOD => {
            let params = match serde_json::from_value::<lsp_types::CodeActionParams>(req.params) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        Response::new_err(
                            req.id,
                            lsp_server::ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        ),
                        None,
                        Vec::new(),
                    );
                }
            };

            let uri = params.text_document.uri;
            let mut actions = Vec::new();

            if position_encoding != StubPositionEncoding::Utf16 {
                let b_uri = uri
                    .to_file_path()
                    .ok()
                    .and_then(|path| path.parent().map(|dir| dir.join("b.rs")))
                    .and_then(|path| lsp_types::Url::from_file_path(path).ok())
                    .or_else(|| {
                        root.and_then(|root| lsp_types::Url::from_file_path(root.join("b.rs")).ok())
                    });
                if let Some(b_uri) = b_uri {
                    let start_col = position_encoding.col_units_for_str("😀");
                    let end_col = start_col + position_encoding.col_units_for_str("hello");
                    let mut changes = HashMap::new();
                    changes.insert(
                        b_uri,
                        vec![lsp_types::TextEdit {
                            range: lsp_types::Range::new(
                                lsp_types::Position::new(0, start_col),
                                lsp_types::Position::new(0, end_col),
                            ),
                            new_text: "rust".to_string(),
                        }],
                    );

                    let edit = lsp_types::WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    };

                    actions.push(lsp_types::CodeActionOrCommand::CodeAction(
                        lsp_types::CodeAction {
                            title: "Stub: Edit unopened file (multibyte)".to_string(),
                            kind: None,
                            diagnostics: None,
                            edit: Some(edit),
                            command: None,
                            is_preferred: Some(true),
                            disabled: None,
                            data: None,
                        },
                    ));
                }
            }

            let mut changes = HashMap::new();
            changes.insert(
                uri.clone(),
                vec![lsp_types::TextEdit {
                    range: lsp_types::Range::new(
                        lsp_types::Position::new(0, 0),
                        lsp_types::Position::new(0, 0),
                    ),
                    new_text: "// edit\n".to_string(),
                }],
            );

            let edit = lsp_types::WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            };

            let insert_edit = lsp_types::CodeAction {
                title: "Stub: Insert edit".to_string(),
                kind: None,
                diagnostics: None,
                edit: Some(edit),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            };

            let insert_cmd = lsp_types::CodeAction {
                title: "Stub: Execute command (applyEdit)".to_string(),
                kind: None,
                diagnostics: None,
                edit: None,
                command: Some(lsp_types::Command {
                    title: "Stub: Execute command (applyEdit)".to_string(),
                    command: "stub.insert_cmd".to_string(),
                    arguments: Some(vec![Value::String(uri.to_string())]),
                }),
                is_preferred: Some(false),
                disabled: None,
                data: None,
            };

            actions.push(lsp_types::CodeActionOrCommand::CodeAction(insert_edit));
            actions.push(lsp_types::CodeActionOrCommand::CodeAction(insert_cmd));

            let base_dir = uri
                .to_file_path()
                .ok()
                .and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
                .or_else(|| root.cloned());
            if let Some(base_dir) = base_dir {
                let create_uri =
                    lsp_types::Url::from_file_path(base_dir.join("resource_created.rs")).ok();
                let old_uri = lsp_types::Url::from_file_path(base_dir.join("resource_old.rs")).ok();
                let new_uri = lsp_types::Url::from_file_path(base_dir.join("resource_new.rs")).ok();
                let delete_uri =
                    lsp_types::Url::from_file_path(base_dir.join("resource_delete.rs")).ok();
                if let (Some(create_uri), Some(old_uri), Some(new_uri), Some(delete_uri)) =
                    (create_uri, old_uri, new_uri, delete_uri)
                {
                    let ops = vec![
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Create(
                            lsp_types::CreateFile {
                                uri: create_uri,
                                options: None,
                                annotation_id: None,
                            },
                        )),
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Rename(
                            lsp_types::RenameFile {
                                old_uri,
                                new_uri,
                                options: None,
                                annotation_id: None,
                            },
                        )),
                        lsp_types::DocumentChangeOperation::Op(lsp_types::ResourceOp::Delete(
                            lsp_types::DeleteFile {
                                uri: delete_uri,
                                options: None,
                            },
                        )),
                    ];
                    let edit = lsp_types::WorkspaceEdit {
                        changes: None,
                        document_changes: Some(lsp_types::DocumentChanges::Operations(ops)),
                        change_annotations: None,
                    };
                    actions.push(lsp_types::CodeActionOrCommand::CodeAction(
                        lsp_types::CodeAction {
                            title: "Stub: Resource operations".to_string(),
                            kind: None,
                            diagnostics: None,
                            edit: Some(edit),
                            command: None,
                            is_preferred: Some(false),
                            disabled: None,
                            data: None,
                        },
                    ));
                }
            }

            (Response::new_ok(req.id, Some(actions)), None, Vec::new())
        }
        m if m == lsp_types::request::ExecuteCommand::METHOD => {
            let params = match serde_json::from_value::<lsp_types::ExecuteCommandParams>(req.params)
            {
                Ok(params) => params,
                Err(err) => {
                    return (
                        Response::new_err(
                            req.id,
                            lsp_server::ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        ),
                        None,
                        Vec::new(),
                    );
                }
            };

            let mut extra = Vec::new();
            if params.command == "stub.insert_cmd" {
                let uri = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .and_then(|s| lsp_types::Url::parse(s).ok());
                if let Some(uri) = uri {
                    let mut changes = HashMap::new();
                    changes.insert(
                        uri,
                        vec![lsp_types::TextEdit {
                            range: lsp_types::Range::new(
                                lsp_types::Position::new(0, 0),
                                lsp_types::Position::new(0, 0),
                            ),
                            new_text: "// cmd\n".to_string(),
                        }],
                    );
                    let edit = lsp_types::WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    };

                    let params = lsp_types::ApplyWorkspaceEditParams {
                        label: Some("stub insert cmd".to_string()),
                        edit,
                    };

                    let id = *next_server_request_id;
                    *next_server_request_id = next_server_request_id.saturating_add(1);

                    extra.push(Message::Request(Request::new(
                        lsp_server::RequestId::from(id),
                        lsp_types::request::ApplyWorkspaceEdit::METHOD.to_string(),
                        params,
                    )));
                }
            }

            (Response::new_ok(req.id, Value::Null), None, extra)
        }
        m if m == lsp_types::request::Formatting::METHOD => {
            let params =
                match serde_json::from_value::<lsp_types::DocumentFormattingParams>(req.params) {
                    Ok(params) => params,
                    Err(err) => {
                        return (
                            Response::new_err(
                                req.id,
                                lsp_server::ErrorCode::InvalidParams as i32,
                                format!("invalid params: {err}"),
                            ),
                            None,
                            Vec::new(),
                        );
                    }
                };

            let edits = vec![lsp_types::TextEdit {
                range: lsp_types::Range::new(
                    lsp_types::Position::new(0, 0),
                    lsp_types::Position::new(0, 0),
                ),
                new_text: format!(
                    "// formatted {}\n",
                    params.text_document.uri.path().trim_start_matches('/')
                ),
            }];

            (Response::new_ok(req.id, Some(edits)), None, Vec::new())
        }
        m if m == lsp_types::request::RangeFormatting::METHOD => {
            let params = match serde_json::from_value::<lsp_types::DocumentRangeFormattingParams>(
                req.params,
            ) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        Response::new_err(
                            req.id,
                            lsp_server::ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        ),
                        None,
                        Vec::new(),
                    );
                }
            };

            let edits = vec![lsp_types::TextEdit {
                range: params.range,
                new_text: "RANGE".to_string(),
            }];

            (Response::new_ok(req.id, Some(edits)), None, Vec::new())
        }
        m if m == lsp_types::request::Rename::METHOD => {
            let params = match serde_json::from_value::<lsp_types::RenameParams>(req.params) {
                Ok(params) => params,
                Err(err) => {
                    return (
                        Response::new_err(
                            req.id,
                            lsp_server::ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        ),
                        None,
                        Vec::new(),
                    );
                }
            };

            let uri = params.text_document_position.text_document.uri;
            let path = match uri.to_file_path() {
                Ok(path) => path,
                Err(_) => {
                    return (
                        Response::new_ok(req.id, Option::<lsp_types::WorkspaceEdit>::None),
                        None,
                        Vec::new(),
                    )
                }
            };

            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => {
                    return (
                        Response::new_ok(req.id, Option::<lsp_types::WorkspaceEdit>::None),
                        None,
                        Vec::new(),
                    )
                }
            };

            let edits = rename_edits(
                &text,
                params.text_document_position.position,
                &params.new_name,
                position_encoding,
            )
            .unwrap_or_default();

            if edits.is_empty() {
                return (
                    Response::new_ok(req.id, Option::<lsp_types::WorkspaceEdit>::None),
                    None,
                    Vec::new(),
                );
            }

            let mut changes = HashMap::new();
            changes.insert(uri, edits);
            let edit = lsp_types::WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            };

            (Response::new_ok(req.id, Some(edit)), None, Vec::new())
        }
        m if m == lsp_types::request::Shutdown::METHOD => {
            (Response::new_ok(req.id, ()), None, Vec::new())
        }
        _ => (
            Response::new_err(
                req.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                "Method not found".to_string(),
            ),
            None,
            Vec::new(),
        ),
    }
}

fn rename_edits(
    text: &str,
    position: lsp_types::Position,
    new_name: &str,
    encoding: StubPositionEncoding,
) -> Option<Vec<lsp_types::TextEdit>> {
    let offset = position_to_byte_offset(text, position, encoding);
    let (name_start, name_end) = word_range_at(text, offset)?;
    let old_name = text.get(name_start..name_end)?;
    if old_name.is_empty() || old_name == new_name {
        return None;
    }

    let line_starts = line_starts(text);
    let mut edits = Vec::new();
    let mut search_start = 0;
    while let Some(rel_start) = text.get(search_start..)?.find(old_name) {
        let start = search_start + rel_start;
        let end = start + old_name.len();
        if is_word_boundary(text, start, end) {
            edits.push(lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: byte_offset_to_position(text, start, &line_starts, encoding),
                    end: byte_offset_to_position(text, end, &line_starts, encoding),
                },
                new_text: new_name.to_string(),
            });
        }
        search_start = end;
    }

    Some(edits)
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let prev = text[..start].chars().last();
    let next = text[end..].chars().next();
    prev.is_none_or(|ch| !is_word_char(ch)) && next.is_none_or(|ch| !is_word_char(ch))
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn position_to_byte_offset(
    text: &str,
    position: lsp_types::Position,
    encoding: StubPositionEncoding,
) -> usize {
    let line = position.line as usize;
    let character = position.character as usize;
    let line_starts = line_starts(text);
    let Some(&start) = line_starts.get(line) else {
        return text.len();
    };

    let end = line_end(text, line, &line_starts);
    let line_text = &text[start..end];

    let mut units = 0usize;
    for (byte_idx, ch) in line_text.char_indices() {
        if units >= character {
            return start + byte_idx;
        }
        units += match encoding {
            StubPositionEncoding::Utf8 => ch.len_utf8(),
            StubPositionEncoding::Utf16 => ch.len_utf16(),
            StubPositionEncoding::Utf32 => 1,
        };
    }

    start + line_text.len()
}

fn word_range_at(text: &str, mut offset: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }

    if offset >= text.len() {
        offset = text.len().saturating_sub(1);
    }

    while offset > 0 && !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }

    if !is_word_char(text[offset..].chars().next()?) && offset > 0 {
        let prev = text[..offset].chars().last()?;
        if is_word_char(prev) {
            offset = text[..offset]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
        }
    }

    if !is_word_char(text[offset..].chars().next()?) {
        return None;
    }

    let mut start = offset;
    while start > 0 {
        let (prev_idx, prev_ch) = text[..start].char_indices().last()?;
        if is_word_char(prev_ch) {
            start = prev_idx;
        } else {
            break;
        }
    }

    let mut end = offset;
    if let Some(ch) = text[end..].chars().next() {
        end += ch.len_utf8();
    }
    while end < text.len() {
        let ch = text[end..].chars().next()?;
        if is_word_char(ch) {
            end += ch.len_utf8();
        } else {
            break;
        }
    }

    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, byte) in text.as_bytes().iter().enumerate() {
        if *byte == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

fn line_end(text: &str, line: usize, starts: &[usize]) -> usize {
    let start = starts.get(line).copied().unwrap_or(0);
    let mut end = starts.get(line + 1).copied().unwrap_or_else(|| text.len());
    if end > 0 && end <= text.len() && text.as_bytes().get(end - 1) == Some(&b'\n') {
        end = end.saturating_sub(1);
    }
    if end > start && text.as_bytes().get(end - 1) == Some(&b'\r') {
        end = end.saturating_sub(1);
    }
    end.min(text.len())
}

fn byte_offset_to_position(
    text: &str,
    offset: usize,
    starts: &[usize],
    encoding: StubPositionEncoding,
) -> lsp_types::Position {
    let offset = offset.min(text.len());
    let idx = starts.partition_point(|&start| start <= offset);
    let line = idx.saturating_sub(1);
    let line_start = starts.get(line).copied().unwrap_or(0);
    let character = match encoding {
        StubPositionEncoding::Utf8 => offset.saturating_sub(line_start),
        StubPositionEncoding::Utf16 => text[line_start..offset].encode_utf16().count(),
        StubPositionEncoding::Utf32 => text[line_start..offset].chars().count(),
    };
    lsp_types::Position::new(line as u32, character as u32)
}

fn workspace_root_from_initialize(params: &lsp_types::InitializeParams) -> Option<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .and_then(|folder| folder.uri.to_file_path().ok())
        .or_else(|| {
            #[allow(deprecated)]
            {
                params
                    .root_uri
                    .as_ref()
                    .and_then(|uri| uri.to_file_path().ok())
            }
        })
}

fn notification_uri(not: &Notification) -> Option<lsp_types::Url> {
    match not.method.as_str() {
        m if m == lsp_types::notification::DidOpenTextDocument::METHOD => {
            serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(not.params.clone())
                .ok()
                .map(|p| p.text_document.uri)
        }
        m if m == lsp_types::notification::DidChangeTextDocument::METHOD => {
            serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(not.params.clone())
                .ok()
                .map(|p| p.text_document.uri)
        }
        m if m == lsp_types::notification::DidSaveTextDocument::METHOD => {
            serde_json::from_value::<lsp_types::DidSaveTextDocumentParams>(not.params.clone())
                .ok()
                .map(|p| p.text_document.uri)
        }
        _ => None,
    }
}

fn publish_diagnostics(uri: lsp_types::Url, marker: &str) -> lsp_types::PublishDiagnosticsParams {
    let diag = lsp_types::Diagnostic {
        range: lsp_types::Range::new(
            lsp_types::Position::new(0, 0),
            lsp_types::Position::new(0, 1),
        ),
        severity: Some(lsp_types::DiagnosticSeverity::WARNING),
        code: None,
        code_description: None,
        source: Some("zcode-lsp-stub".to_string()),
        message: marker.to_string(),
        related_information: None,
        tags: None,
        data: Some(Value::Null),
    };

    lsp_types::PublishDiagnosticsParams {
        uri,
        diagnostics: vec![diag],
        version: None,
    }
}
