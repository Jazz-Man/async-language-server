use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use async_lsp::{
    ClientSocket, ErrorCode, LanguageServer,
    lsp_types::{
        ClientCapabilities, CompletionItem, CompletionTextEdit, Diagnostic, DiagnosticOptions,
        DiagnosticServerCapabilities, DidChangeConfigurationParams,
        DidChangeWorkspaceFoldersParams, DidOpenTextDocumentParams, DocumentDiagnosticParams,
        DocumentDiagnosticReport, DocumentDiagnosticReportKind, DocumentDiagnosticReportResult,
        FullDocumentDiagnosticReport, GeneralClientCapabilities, InitializeParams, OneOf,
        PartialResultParams, Position, PositionEncodingKind, PreviousResultId, Range,
        RelatedFullDocumentDiagnosticReport, ServerCapabilities, TextDocumentItem, TextEdit, Url,
        WorkDoneProgressParams, WorkspaceDiagnosticParams, WorkspaceDiagnosticReportResult,
        WorkspaceDocumentDiagnosticReport, WorkspaceFolder, WorkspaceFoldersChangeEvent,
    },
};

use crate::server::{
    DocumentMatcher, LanguageServerWithState, Server, ServerOptions, ServerResult, ServerState,
    WorkspaceDiagnostics,
};

struct TestServer;

impl Server for TestServer {
    fn server_capabilities(_: ClientCapabilities) -> Option<ServerCapabilities> {
        test_capabilities()
    }

    fn server_document_matchers() -> Vec<DocumentMatcher> {
        test_document_matchers()
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send
    {
        std::future::ready(test_document_diagnostics(&state, params))
    }

    fn completion_resolve(
        &self,
        _state: ServerState,
        item: CompletionItem,
    ) -> impl std::future::Future<Output = ServerResult<CompletionItem>> + Send {
        std::future::ready(Ok(item))
    }
}

struct DisabledServer;

impl Server for DisabledServer {
    fn server_capabilities(_: ClientCapabilities) -> Option<ServerCapabilities> {
        test_capabilities()
    }

    fn server_document_matchers() -> Vec<DocumentMatcher> {
        test_document_matchers()
    }

    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::disabled())
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send
    {
        std::future::ready(test_document_diagnostics(&state, params))
    }
}

struct ConfigurableServer;

impl Server for ConfigurableServer {
    fn server_capabilities(_: ClientCapabilities) -> Option<ServerCapabilities> {
        test_capabilities()
    }

    fn server_document_matchers() -> Vec<DocumentMatcher> {
        test_document_matchers()
    }

    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_workspace_diagnostics(
            WorkspaceDiagnostics::setting("test.workspaceDiagnostics.enabled")
                .with_default_enabled(false),
        )
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send
    {
        std::future::ready(test_document_diagnostics(&state, params))
    }
}

fn test_capabilities() -> Option<ServerCapabilities> {
    Some(ServerCapabilities {
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            inter_file_dependencies: true,
            workspace_diagnostics: false,
            ..Default::default()
        })),
        ..Default::default()
    })
}

fn test_document_matchers() -> Vec<DocumentMatcher> {
    vec![
        DocumentMatcher::new("Test")
            .with_url_globs(["**/*.test", "*.test"])
            .with_lang_strings(["test"]),
    ]
}

fn test_document_diagnostics(
    state: &ServerState,
    params: DocumentDiagnosticParams,
) -> ServerResult<DocumentDiagnosticReportResult> {
    let message = if let Some(previous) = params.previous_result_id {
        format!("{}:{previous}", params.identifier.unwrap_or_default())
    } else {
        state
            .document(&params.text_document.uri)
            .map_or_else(String::new, |doc| doc.text_contents())
    };
    let related_documents = if message == "source" {
        related_uri(&params.text_document.uri).map(|uri| {
            HashMap::from([(
                uri,
                DocumentDiagnosticReportKind::Full(FullDocumentDiagnosticReport {
                    result_id: None,
                    items: vec![diagnostic("related")],
                }),
            )])
        })
    } else {
        None
    };

    Ok(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: vec![diagnostic(message)],
            },
        }),
    ))
}

#[test]
fn initialize_enables_workspace_diagnostics() {
    let root = temp_workspace("capabilities");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let result = futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let Some(DiagnosticServerCapabilities::Options(options)) =
        result.capabilities.diagnostic_provider
    else {
        panic!("expected diagnostic options");
    };
    assert!(options.workspace_diagnostics);

    let Some(workspace) = result.capabilities.workspace else {
        panic!("expected workspace capabilities");
    };
    let Some(folders) = workspace.workspace_folders else {
        panic!("expected workspace folder capabilities");
    };
    assert_eq!(folders.supported, Some(true));
    assert_eq!(folders.change_notifications, Some(OneOf::Left(true)));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn initialize_respects_disabled_workspace_diagnostics() {
    let root = temp_workspace("disabled-capabilities");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), DisabledServer);

    let result = futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let Some(DiagnosticServerCapabilities::Options(options)) =
        result.capabilities.diagnostic_provider
    else {
        panic!("expected diagnostic options");
    };
    assert!(!options.workspace_diagnostics);
    assert!(result.capabilities.workspace.is_none());

    let error =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect_err("workspace diagnostics should be disabled");
    assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn initialize_ignores_unknown_client_encodings() {
    let root = temp_workspace("unknown-encoding");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![
            PositionEncodingKind::new("utf-7"),
            PositionEncodingKind::UTF16,
        ]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF16)
    );

    // A client that offers only unknown encodings falls back to the
    // protocol default, `Encoding::default()` (UTF-16).
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![PositionEncodingKind::new("utf-7")]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF16)
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn configurable_workspace_diagnostics_can_be_toggled() {
    let root = temp_workspace("configurable-diagnostics");
    let file = root.join("a.test");
    fs::write(&file, "disk").expect("test file can be written");
    let file = fs::canonicalize(file).expect("test file can be canonicalized");
    let uri = Url::from_file_path(file).expect("path can be converted to a URL");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), ConfigurableServer);
    let result = futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let Some(DiagnosticServerCapabilities::Options(options)) =
        result.capabilities.diagnostic_provider
    else {
        panic!("expected diagnostic options");
    };
    assert!(options.workspace_diagnostics);

    let report =
        futures::executor::block_on(server.workspace_diagnostic(WorkspaceDiagnosticParams {
            previous_result_ids: vec![PreviousResultId {
                uri: uri.clone(),
                value: "old".into(),
            }],
            ..workspace_diagnostic_params()
        }))
        .expect("workspace diagnostics can be fetched");
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one clearing report");
    };
    assert_eq!(report.uri, uri);
    assert!(report.full_document_diagnostic_report.items.is_empty());

    let _ = server.did_change_configuration(DidChangeConfigurationParams {
        settings: serde_json::json!({
            "test": {
                "workspaceDiagnostics": {
                    "enabled": true,
                },
            },
        }),
    });

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "disk"
    );

    let _ = server.did_change_configuration(DidChangeConfigurationParams {
        settings: serde_json::json!({
            "test.workspaceDiagnostics.enabled": false,
        }),
    });

    let report =
        futures::executor::block_on(server.workspace_diagnostic(WorkspaceDiagnosticParams {
            previous_result_ids: vec![PreviousResultId {
                uri,
                value: "old".into(),
            }],
            ..workspace_diagnostic_params()
        }))
        .expect("workspace diagnostics can be fetched");
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one clearing report");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn configurable_workspace_diagnostics_read_initialization_options() {
    let root = temp_workspace("configurable-diagnostics-init");
    fs::write(root.join("a.test"), "disk").expect("test file can be written");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), ConfigurableServer);
    let mut params = initialize_params(&root);
    params.initialization_options = Some(serde_json::json!({
        "test": {
            "workspaceDiagnostics": {
                "enabled": true,
            },
        },
    }));
    futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");
    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "disk"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn workspace_diagnostics_report_unopened_documents_without_versions() {
    let root = temp_workspace("workspace-diagnostics");
    let file = root.join("a.test");
    fs::write(&file, "disk").expect("test file can be written");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(report.version, None);
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "disk"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn workspace_diagnostics_use_open_document_versions() {
    let root = temp_workspace("open-workspace-diagnostics");
    let file = root.join("a.test");
    fs::write(&file, "disk").expect("test file can be written");
    let file = fs::canonicalize(file).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&file).expect("path can be converted to a URL");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");
    let _ = server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri, "test".into(), 3, "open".into()),
    });

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(report.version, Some(3));
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "open"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn workspace_diagnostics_forward_previous_result_ids() {
    let root = temp_workspace("previous-result-id");
    let file = root.join("a.test");
    fs::write(&file, "disk").expect("test file can be written");
    let file = fs::canonicalize(file).expect("test file can be canonicalized");
    let uri = Url::from_file_path(file).expect("path can be converted to a URL");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let report =
        futures::executor::block_on(server.workspace_diagnostic(WorkspaceDiagnosticParams {
            identifier: Some("test".into()),
            previous_result_ids: vec![PreviousResultId {
                uri,
                value: "cached".into(),
            }],
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }))
        .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "test:cached"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn workspace_folder_changes_are_used_by_workspace_diagnostics() {
    let first = temp_workspace("workspace-folder-change-first");
    let second = temp_workspace("workspace-folder-change-second");
    fs::write(first.join("a.test"), "first").expect("test file can be written");
    fs::write(second.join("b.test"), "second").expect("test file can be written");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    futures::executor::block_on(server.initialize(initialize_params(&first)))
        .expect("server can initialize");
    let _ = server.did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![workspace_folder(&second)],
            removed: vec![workspace_folder(&first)],
        },
    });

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let [WorkspaceDocumentDiagnosticReport::Full(report)] = report.items.as_slice() else {
        panic!("expected one full document report");
    };
    assert_eq!(
        report.full_document_diagnostic_report.items[0].message,
        "second"
    );

    fs::remove_dir_all(first).expect("temp workspace can be removed");
    fs::remove_dir_all(second).expect("temp workspace can be removed");
}

#[test]
fn workspace_diagnostics_prefer_direct_reports_over_related_reports() {
    let root = temp_workspace("related-reports");
    fs::write(root.join("a.test"), "source").expect("test file can be written");
    fs::write(root.join("b.test"), "direct").expect("test file can be written");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    futures::executor::block_on(server.initialize(initialize_params(&root)))
        .expect("server can initialize");

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    let messages: Vec<_> = report.items.iter().map(workspace_report_message).collect();
    assert_eq!(messages, ["source", "direct"]);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn initialize_without_workspace_folders_reports_no_items() {
    let root = temp_workspace("no-folders");
    fs::write(root.join("a.test"), "disk").expect("test file can be written");

    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    let mut params = initialize_params(&root);
    params.workspace_folders = None; // client sends neither folders nor rootUri
    futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    let report =
        futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
            .expect("workspace diagnostics can be fetched");

    let WorkspaceDiagnosticReportResult::Report(report) = report else {
        panic!("expected full workspace diagnostic report");
    };
    assert!(report.items.is_empty());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn resolve_converts_with_sole_document_and_passes_through_with_two() {
    let root = temp_workspace("resolve-pick");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![PositionEncodingKind::UTF16]),
        ..Default::default()
    });
    futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    let first = Url::from_file_path(root.join("a.test")).expect("path can be converted to a URL");
    let _ = server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(first, "test".into(), 1, "🙂abc".into()),
    });

    let echo = CompletionItem {
        label: "x".into(),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
            Range::new(Position::new(0, 2), Position::new(0, 2)),
            "y".into(),
        ))),
        ..Default::default()
    };

    // Sole document: the echo handler receives UTF-8 (0,4) and the wire
    // shows UTF-16 (0,2) — identity for the client. Missing either
    // converter breaks this arm (one-way conversion shifts the column).
    let resolved = futures::executor::block_on(server.completion_item_resolve(echo.clone()))
        .expect("resolve succeeds");
    let Some(CompletionTextEdit::Edit(edit)) = resolved.text_edit else {
        panic!("expected edit");
    };
    assert_eq!(edit.range.start, Position::new(0, 2));

    // Second document: no sole document, both converters pass through.
    let second = Url::from_file_path(root.join("b.test")).expect("path can be converted to a URL");
    let _ = server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(second, "test".into(), 1, "🙂def".into()),
    });
    let resolved = futures::executor::block_on(server.completion_item_resolve(echo))
        .expect("resolve succeeds");
    let Some(CompletionTextEdit::Edit(edit)) = resolved.text_edit else {
        panic!("expected edit");
    };
    assert_eq!(edit.range.start, Position::new(0, 2));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

fn temp_workspace(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after epoch")
        .as_millis();
    let root =
        std::env::temp_dir().join(format!("async-language-server-workspace-{name}-{millis}"));
    fs::create_dir_all(&root).expect("temp workspace can be created");
    root
}

fn initialize_params(root: &PathBuf) -> InitializeParams {
    InitializeParams {
        process_id: Some(std::process::id()),
        capabilities: ClientCapabilities::default(),
        workspace_folders: Some(vec![workspace_folder(root)]),
        ..Default::default()
    }
}

fn workspace_folder(path: &PathBuf) -> WorkspaceFolder {
    let uri = Url::from_file_path(path).expect("path can be converted to a URL");
    WorkspaceFolder {
        uri,
        name: "test".into(),
    }
}

fn workspace_diagnostic_params() -> WorkspaceDiagnosticParams {
    WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids: Vec::new(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn related_uri(uri: &Url) -> Option<Url> {
    let path = uri.to_file_path().ok()?.with_file_name("b.test");
    Url::from_file_path(path).ok()
}

fn workspace_report_message(report: &WorkspaceDocumentDiagnosticReport) -> &str {
    match report {
        WorkspaceDocumentDiagnosticReport::Full(report) => {
            report.full_document_diagnostic_report.items[0]
                .message
                .as_str()
        }
        WorkspaceDocumentDiagnosticReport::Unchanged(_) => panic!("expected full report"),
    }
}

fn diagnostic(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        },
        message: message.into(),
        ..Default::default()
    }
}
