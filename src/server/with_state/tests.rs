use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_lsp::{
    ClientSocket, ErrorCode, LanguageServer,
    lsp_types::{
        ClientCapabilities, CodeLens, CompletionItem, CompletionTextEdit, CreateFilesParams,
        DeleteFilesParams, DiagnosticOptions, DiagnosticServerCapabilities,
        DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
        DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, DocumentDiagnosticParams, DocumentDiagnosticReport,
        DocumentDiagnosticReportKind, DocumentDiagnosticReportResult, DocumentLink, FileChangeType,
        FileCreate, FileDelete, FileEvent, FileRename, FullDocumentDiagnosticReport,
        GeneralClientCapabilities, Hover, HoverContents, HoverParams, InitializeParams, InlayHint,
        InlayHintLabel, InlayHintLabelPart, Location, MarkupContent, MarkupKind, NumberOrString,
        OneOf, PartialResultParams, Position, PositionEncodingKind, PreviousResultId, Range,
        RelatedFullDocumentDiagnosticReport, RenameFilesParams, ServerCapabilities, SymbolKind,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentPositionParams, TextDocumentSaveReason, TextEdit, Url,
        VersionedTextDocumentIdentifier, WillSaveTextDocumentParams, WorkDoneProgressCancelParams,
        WorkDoneProgressParams, WorkspaceDiagnosticParams, WorkspaceDiagnosticReportResult,
        WorkspaceDocumentDiagnosticReport, WorkspaceEdit, WorkspaceFoldersChangeEvent,
        WorkspaceLocation, WorkspaceSymbol, WorkspaceSymbolParams, WorkspaceSymbolResponse,
    },
};

use crate::server::{
    DocumentMatcher, LanguageServerWithState, Server, ServerOptions, ServerResult, ServerState,
    WorkspaceDiagnostics,
};
use crate::testing::{diagnostic, line_position, same_line, temp_workspace, url, workspace_folder};
use crate::text_utils::Encoding;

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

struct LinkCaptureServer {
    received: Arc<Mutex<Option<Range>>>,
}

impl Server for LinkCaptureServer {
    fn link_resolve(
        &self,
        _state: ServerState,
        link: DocumentLink,
    ) -> impl std::future::Future<Output = ServerResult<DocumentLink>> + Send {
        let received = Arc::clone(&self.received);
        async move {
            *received.lock().expect("capture mutex") = Some(link.range);
            Ok(link)
        }
    }
}

/// Answers `workspace/willCreateFiles` with a workspace edit keyed at
/// `url("edit.txt")`, carrying a UTF-8 byte-4 range.
struct EditServer;

impl Server for EditServer {
    fn will_create_files(
        &self,
        _state: ServerState,
        _params: CreateFilesParams,
    ) -> impl std::future::Future<Output = ServerResult<Option<WorkspaceEdit>>> + Send {
        let mut changes = HashMap::new();
        changes.insert(
            url("edit.txt"),
            vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }],
        );
        std::future::ready(Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        })))
    }
}

/// Echoes a hover whose range sits at UTF-8 byte 4, capturing the
/// position the handler received.
struct HoverCaptureServer {
    received: Arc<Mutex<Option<Position>>>,
}

impl Server for HoverCaptureServer {
    fn hover(
        &self,
        _state: ServerState,
        params: HoverParams,
    ) -> impl std::future::Future<Output = ServerResult<Option<Hover>>> + Send {
        let received = Arc::clone(&self.received);
        async move {
            *received.lock().expect("capture mutex") =
                Some(params.text_document_position_params.position);
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: "hover".into(),
                }),
                range: Some(same_line(0, 4, 4)),
            }))
        }
    }
}

/// Answers `workspace/symbol` with nested symbols located in `url("a.txt")`
/// at UTF-8 (0,4)-(0,5) and `self.0` at UTF-8 (0,5)-(0,9).
struct SymbolServer(Url);

impl Server for SymbolServer {
    fn symbol(
        &self,
        _state: ServerState,
        _params: WorkspaceSymbolParams,
    ) -> impl std::future::Future<Output = ServerResult<Option<WorkspaceSymbolResponse>>> + Send
    {
        let second_uri = self.0.clone();
        let nested = vec![
            WorkspaceSymbol {
                name: "a".into(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: OneOf::Left(Location {
                    uri: url("a.txt"),
                    range: same_line(0, 4, 5),
                }),
                data: None,
            },
            WorkspaceSymbol {
                name: "b".into(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                container_name: None,
                location: OneOf::Left(Location {
                    uri: second_uri,
                    range: same_line(0, 5, 9),
                }),
                data: None,
            },
        ];
        std::future::ready(Ok(Some(WorkspaceSymbolResponse::Nested(nested))))
    }
}

/// Echoes each resolve item back, capturing what its handler received so
/// the resolve dispatch tests can pin the handler-side columns.
struct ResolveCaptureServer {
    code_lens: Arc<Mutex<Option<CodeLens>>>,
    inlay_hint: Arc<Mutex<Option<InlayHint>>>,
    workspace_symbol: Arc<Mutex<Option<WorkspaceSymbol>>>,
}

impl ResolveCaptureServer {
    /// Stores the received item in its capture slot.
    fn store<T: Clone>(slot: &Mutex<Option<T>>, item: &T) {
        *slot.lock().expect("capture mutex") = Some(item.clone());
    }
}

impl Server for ResolveCaptureServer {
    fn code_lens_resolve(
        &self,
        _state: ServerState,
        lens: CodeLens,
    ) -> impl std::future::Future<Output = ServerResult<CodeLens>> + Send {
        let received = Arc::clone(&self.code_lens);
        async move {
            Self::store(&received, &lens);
            Ok(lens)
        }
    }

    fn inlay_hint_resolve(
        &self,
        _state: ServerState,
        hint: InlayHint,
    ) -> impl std::future::Future<Output = ServerResult<InlayHint>> + Send {
        let received = Arc::clone(&self.inlay_hint);
        async move {
            Self::store(&received, &hint);
            Ok(hint)
        }
    }

    fn workspace_symbol_resolve(
        &self,
        _state: ServerState,
        symbol: WorkspaceSymbol,
    ) -> impl std::future::Future<Output = ServerResult<WorkspaceSymbol>> + Send {
        let received = Arc::clone(&self.workspace_symbol);
        async move {
            Self::store(&received, &symbol);
            Ok(symbol)
        }
    }
}

/// Records every notification hook fired, in order. The
/// `did_change_watched_files` hook also captures the tracked document text
/// it observes, pinning the after-the-internal-handler contract.
struct HookRecordingServer {
    hooks: Arc<Mutex<Vec<&'static str>>>,
    watched_url: Url,
    watched_text: Arc<Mutex<Option<String>>>,
}

impl HookRecordingServer {
    /// Records one fired hook by name.
    fn record(&self, hook: &'static str) {
        self.hooks.lock().expect("hook record mutex").push(hook);
    }
}

impl Server for HookRecordingServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        test_document_matchers()
    }

    fn did_change_configuration(
        &self,
        _state: &ServerState,
        _params: &DidChangeConfigurationParams,
    ) {
        self.record("did_change_configuration");
    }

    fn did_change_workspace_folders(
        &self,
        _state: &ServerState,
        _params: &DidChangeWorkspaceFoldersParams,
    ) {
        self.record("did_change_workspace_folders");
    }

    fn did_open(&self, _state: &ServerState, _params: &DidOpenTextDocumentParams) {
        self.record("did_open");
    }

    fn did_close(&self, _state: &ServerState, _params: &DidCloseTextDocumentParams) {
        self.record("did_close");
    }

    fn did_change(&self, _state: &ServerState, _params: &DidChangeTextDocumentParams) {
        self.record("did_change");
    }

    fn did_save(&self, _state: &ServerState, _params: &DidSaveTextDocumentParams) {
        self.record("did_save");
    }

    fn will_save(&self, _state: &ServerState, _params: &WillSaveTextDocumentParams) {
        self.record("will_save");
    }

    fn did_change_watched_files(&self, state: &ServerState, _params: &DidChangeWatchedFilesParams) {
        *self.watched_text.lock().expect("hook record mutex") = state
            .document(&self.watched_url)
            .map(|doc| doc.text_contents());
        self.record("did_change_watched_files");
    }

    fn did_create_files(&self, _state: &ServerState, _params: &CreateFilesParams) {
        self.record("did_create_files");
    }

    fn did_rename_files(&self, _state: &ServerState, _params: &RenameFilesParams) {
        self.record("did_rename_files");
    }

    fn did_delete_files(&self, _state: &ServerState, _params: &DeleteFilesParams) {
        self.record("did_delete_files");
    }

    fn work_done_progress_cancel(
        &self,
        _state: &ServerState,
        _params: &WorkDoneProgressCancelParams,
    ) {
        self.record("work_done_progress_cancel");
    }
}

/// Drives documentLink/resolve over real dispatch: opens `documents`,
/// sends a link at UTF-16 position (0,2) with the given target, and returns
/// (what the handler received, what the client got back).
fn drive_link_resolve(documents: &[(&str, &str)], target: Option<Url>) -> (Option<Range>, Range) {
    let received = Arc::new(Mutex::new(None));
    let mut server = LanguageServerWithState::new(
        ClientSocket::new_closed(),
        LinkCaptureServer {
            received: Arc::clone(&received),
        },
    );
    server.state.set_position_encoding(Encoding::UTF16);
    for (name, text) in documents {
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: url(name),
                language_id: "plaintext".into(),
                version: 0,
                text: (*text).into(),
            },
        });
    }

    let resolved = futures::executor::block_on(server.document_link_resolve(DocumentLink {
        range: same_line(0, 2, 2),
        target,
        tooltip: None,
        data: None,
    }))
    .expect("link resolves");

    (*received.lock().expect("capture mutex"), resolved.range)
}

/// Drives workspace/willCreateFiles over real dispatch: opens `documents`,
/// answers with a UTF-8 byte-4 edit keyed at `url("edit.txt")`, and returns
/// the range the client received.
fn drive_will_create_files(documents: &[(&str, &str)]) -> Range {
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), EditServer);
    server.state.set_position_encoding(Encoding::UTF16);
    for (name, text) in documents {
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: url(name),
                language_id: "plaintext".into(),
                version: 0,
                text: (*text).into(),
            },
        });
    }

    let edit = futures::executor::block_on(server.will_create_files(CreateFilesParams {
        files: vec![FileCreate {
            uri: url("new.txt").to_string(),
        }],
    }))
    .expect("will_create_files succeeds")
    .expect("edit present");
    edit.changes
        .expect("changes present")
        .get(&url("edit.txt"))
        .expect("edit is keyed at the emoji URL")[0]
        .range
}

/// Drives workspace/symbol over real dispatch: opens `documents` and returns
/// the (uri, range) pairs the client received, in handler order. The handler
/// answers with a location in `url("a.txt")` at UTF-8 (0,4)-(0,5) and one in
/// `second_uri` at UTF-8 (0,5)-(0,9).
fn drive_workspace_symbol(documents: &[(&str, &str)], second_uri: Url) -> Vec<(Url, Range)> {
    let mut server =
        LanguageServerWithState::new(ClientSocket::new_closed(), SymbolServer(second_uri));
    server.state.set_position_encoding(Encoding::UTF16);
    for (name, text) in documents {
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: url(name),
                language_id: "plaintext".into(),
                version: 0,
                text: (*text).into(),
            },
        });
    }

    let response = futures::executor::block_on(server.symbol(WorkspaceSymbolParams {
        query: String::new(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }))
    .expect("symbol succeeds");
    let Some(WorkspaceSymbolResponse::Nested(symbols)) = response else {
        panic!("expected nested symbols");
    };
    symbols
        .into_iter()
        .map(|symbol| match symbol.location {
            OneOf::Left(location) => (location.uri, location.range),
            OneOf::Right(_) => panic!("expected a ranged location"),
        })
        .collect()
}

/// Builds a UTF-16 server tracking `documents` behind
/// [`ResolveCaptureServer`]; returns the server and the capture handles
/// its handlers write.
fn resolve_capture_server(
    documents: &[(&str, &str)],
) -> (
    LanguageServerWithState<ResolveCaptureServer>,
    ResolveCaptureServer,
) {
    let captures = ResolveCaptureServer {
        code_lens: Arc::new(Mutex::new(None)),
        inlay_hint: Arc::new(Mutex::new(None)),
        workspace_symbol: Arc::new(Mutex::new(None)),
    };
    let mut server = LanguageServerWithState::new(
        ClientSocket::new_closed(),
        ResolveCaptureServer {
            code_lens: Arc::clone(&captures.code_lens),
            inlay_hint: Arc::clone(&captures.inlay_hint),
            workspace_symbol: Arc::clone(&captures.workspace_symbol),
        },
    );
    server.state.set_position_encoding(Encoding::UTF16);
    for (name, text) in documents {
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(url(name), "plaintext".into(), 0, (*text).into()),
        });
    }
    (server, captures)
}

/// Drives one workspaceSymbol/resolve round trip over the capture server;
/// returns (the location the handler received, the location the client got
/// back).
fn drive_workspace_symbol_resolve(
    server: &mut LanguageServerWithState<ResolveCaptureServer>,
    captures: &ResolveCaptureServer,
    location: OneOf<Location, WorkspaceLocation>,
) -> (
    OneOf<Location, WorkspaceLocation>,
    OneOf<Location, WorkspaceLocation>,
) {
    let resolved = futures::executor::block_on(server.workspace_symbol_resolve(WorkspaceSymbol {
        name: "s".into(),
        kind: SymbolKind::FUNCTION,
        tags: None,
        container_name: None,
        location,
        data: None,
    }))
    .expect("symbol resolves");
    let received = captures
        .workspace_symbol
        .lock()
        .expect("capture mutex")
        .clone()
        .expect("handler ran");
    (received.location, resolved.location)
}

/// Fires all twelve notifications at `server`, in dispatch order. The
/// watched-files event assumes the disk file behind `watched_url` was
/// already mutated before the call.
fn drive_notifications(
    server: &mut LanguageServerWithState<HookRecordingServer>,
    watched_url: Url,
) {
    let doc = url("open.txt");
    let _ = server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(doc.clone(), "plaintext".into(), 1, "open".into()),
    });
    let _ = server.did_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(doc.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "edited".into(),
        }],
    });
    let _ = server.will_save(WillSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(doc.clone()),
        reason: TextDocumentSaveReason::MANUAL,
    });
    let _ = server.did_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(doc.clone()),
        text: Some("saved".into()),
    });
    let _ = server.did_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(doc.clone()),
    });
    let _ = server.did_change_watched_files(DidChangeWatchedFilesParams {
        changes: vec![FileEvent::new(watched_url, FileChangeType::CHANGED)],
    });
    let _ = server.did_create_files(CreateFilesParams {
        files: vec![FileCreate {
            uri: url("created.txt").to_string(),
        }],
    });
    let _ = server.did_rename_files(RenameFilesParams {
        files: vec![FileRename {
            old_uri: url("created.txt").to_string(),
            new_uri: url("renamed.txt").to_string(),
        }],
    });
    let _ = server.did_delete_files(DeleteFilesParams {
        files: vec![FileDelete {
            uri: url("renamed.txt").to_string(),
        }],
    });
    let _ = server.work_done_progress_cancel(WorkDoneProgressCancelParams {
        token: NumberOrString::String("progress".into()),
    });
    let _ = server.did_change_configuration(DidChangeConfigurationParams {
        settings: serde_json::json!({}),
    });
    let _ = server.did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![],
            removed: vec![],
        },
    });
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
    let root = temp_workspace("workspace", "capabilities");
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
    let root = temp_workspace("workspace", "disabled-capabilities");
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
    let root = temp_workspace("workspace", "unknown-encoding");
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
fn initialize_prefers_utf8_when_the_client_offers_it() {
    let root = temp_workspace("workspace", "prefer-utf8");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF8,
        ]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF8)
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn initialize_prefers_utf32_over_utf16() {
    let root = temp_workspace("workspace", "prefer-utf32");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF32,
        ]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF32)
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn configurable_workspace_diagnostics_can_be_toggled() {
    let root = temp_workspace("workspace", "configurable-diagnostics");
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
    let root = temp_workspace("workspace", "configurable-diagnostics-init");
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
    let root = temp_workspace("workspace", "workspace-diagnostics");
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
    let root = temp_workspace("workspace", "open-workspace-diagnostics");
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
    let root = temp_workspace("workspace", "previous-result-id");
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
    let first = temp_workspace("workspace", "workspace-folder-change-first");
    let second = temp_workspace("workspace", "workspace-folder-change-second");
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
    let root = temp_workspace("workspace", "related-reports");
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
    let root = temp_workspace("workspace", "no-folders");
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
    let root = temp_workspace("workspace", "resolve-pick");
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

#[test]
fn link_resolve_converts_against_the_sole_tracked_document() {
    // One tracked document ("🙂abc": byte 4 == UTF-16 unit 2); the link's
    // target points at an untracked URL. Resolve-side conversion keys on
    // the sole document, never the target.
    let (received, returned) =
        drive_link_resolve(&[("only.txt", "🙂abc")], Some(url("untracked.md")));

    assert_eq!(received, Some(same_line(0, 4, 4)));
    assert_eq!(returned, same_line(0, 2, 2));
}

#[test]
fn link_resolve_skips_conversion_without_a_sole_document() {
    // Two tracked documents: the params cannot name the source document,
    // and the target is the OTHER tracked document. Conversion must be
    // skipped — the handler sees the client's UTF-16 positions verbatim,
    // not positions converted against the target's text.
    let (received, returned) =
        drive_link_resolve(&[("a.txt", "🙂abc"), ("b.txt", "🙂🙂")], Some(url("b.txt")));

    assert_eq!(received, Some(same_line(0, 2, 2)));
    assert_eq!(returned, same_line(0, 2, 2));
}

#[test]
fn code_lens_resolve_round_trips_through_the_sole_document() {
    // One tracked document ("🙂abc": byte 4 == UTF-16 unit 2): the handler
    // sees UTF-8, the client its UTF-16 columns back.
    let (mut server, captures) = resolve_capture_server(&[("only.txt", "🙂abc")]);

    let resolved = futures::executor::block_on(server.code_lens_resolve(CodeLens {
        range: same_line(0, 2, 3),
        command: None,
        data: None,
    }))
    .expect("code lens resolves");

    let received = captures
        .code_lens
        .lock()
        .expect("capture mutex")
        .clone()
        .expect("handler ran");
    assert_eq!(received.range, same_line(0, 4, 5));
    assert_eq!(resolved.range, same_line(0, 2, 3));
}

#[test]
fn inlay_hint_resolve_round_trips_through_the_sole_document() {
    // One tracked document ("🙂abc"): position, text edits, and the
    // label-part location (keyed at that same document) all convert — the
    // handler sees UTF-8 everywhere, the client its UTF-16 columns.
    let (mut server, captures) = resolve_capture_server(&[("only.txt", "🙂abc")]);

    let resolved = futures::executor::block_on(server.inlay_hint_resolve(InlayHint {
        position: line_position(0, 2),
        label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
            value: "x".into(),
            tooltip: None,
            location: Some(Location::new(url("only.txt"), same_line(0, 2, 3))),
            command: None,
        }]),
        kind: None,
        text_edits: Some(vec![TextEdit {
            range: same_line(0, 2, 3),
            new_text: "x".into(),
        }]),
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    }))
    .expect("inlay hint resolves");

    let received = captures
        .inlay_hint
        .lock()
        .expect("capture mutex")
        .clone()
        .expect("handler ran");
    assert_eq!(received.position, line_position(0, 4));
    let edits = received.text_edits.expect("edits present");
    assert_eq!(edits[0].range, same_line(0, 4, 5));
    let InlayHintLabel::LabelParts(parts) = &received.label else {
        panic!("label parts present");
    };
    let location = parts[0].location.as_ref().expect("location present");
    assert_eq!(location.range, same_line(0, 4, 5));

    assert_eq!(resolved.position, line_position(0, 2));
}

#[test]
fn workspace_symbol_resolve_converts_per_url_and_passes_right_through() {
    // Sole tracked document "🙂abc"; the symbol's location resolves against
    // ITS OWN document — the tracked snapshot when the URL is tracked, a
    // disk read when it only exists on disk ("x🙂🙂": byte 1 == UTF-16 unit
    // 1, byte 9 == unit 5; the sole anchor would move these columns
    // differently) — and the range-less Right variant passes through.
    let root = temp_workspace("with_state", "symbol-resolve");
    let on_disk = root.join("sym.txt");
    fs::write(&on_disk, "x🙂🙂").expect("test file can be written");
    let on_disk = fs::canonicalize(on_disk).expect("test file can be canonicalized");
    let disk_url = Url::from_file_path(on_disk).expect("path can be converted to a URL");

    let (mut server, captures) = resolve_capture_server(&[("only.txt", "🙂abc")]);

    // Tracked URL: converts against the tracked snapshot, both directions.
    let (received, returned) = drive_workspace_symbol_resolve(
        &mut server,
        &captures,
        OneOf::Left(Location {
            uri: url("only.txt"),
            range: same_line(0, 2, 3),
        }),
    );
    let OneOf::Left(received_location) = received else {
        panic!("expected a ranged location");
    };
    let OneOf::Left(returned_location) = returned else {
        panic!("expected a ranged location");
    };
    assert_eq!(received_location.range, same_line(0, 4, 5));
    assert_eq!(returned_location.range, same_line(0, 2, 3));

    // Untracked but on disk: the disk fallback reads the file, both
    // directions.
    let (received, returned) = drive_workspace_symbol_resolve(
        &mut server,
        &captures,
        OneOf::Left(Location {
            uri: disk_url,
            range: same_line(0, 1, 5),
        }),
    );
    let OneOf::Left(received_location) = received else {
        panic!("expected a ranged location");
    };
    let OneOf::Left(returned_location) = returned else {
        panic!("expected a ranged location");
    };
    assert_eq!(received_location.range, same_line(0, 1, 9));
    assert_eq!(returned_location.range, same_line(0, 1, 5));

    // Right variant: carries no range, passes through unchanged in both
    // directions.
    let expected_right = OneOf::Right(WorkspaceLocation {
        uri: url("only.txt"),
    });
    let (received, returned) = drive_workspace_symbol_resolve(
        &mut server,
        &captures,
        OneOf::Right(WorkspaceLocation {
            uri: url("only.txt"),
        }),
    );
    assert_eq!(received, expected_right);
    assert_eq!(returned, expected_right);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn url_less_response_converts_against_sole_document() {
    // One tracked document ("🙂abc": byte 4 == UTF-16 unit 2); the edit keys
    // at that document's URL. A URL-less file-ops request must still run its
    // outgoing hook, converting against the sole tracked document.
    assert_eq!(
        drive_will_create_files(&[("edit.txt", "🙂abc")]),
        same_line(0, 2, 2)
    );
}

#[test]
fn url_less_passes_through_without_sole_document() {
    // Zero tracked documents: nothing to convert against.
    assert_eq!(drive_will_create_files(&[]), same_line(0, 4, 4));

    // Two tracked documents: no sole document, so the response is
    // returned in the handler's UTF-8 columns, unconverted.
    assert_eq!(
        drive_will_create_files(&[("a.txt", "🙂abc"), ("b.txt", "🙂🙂")]),
        same_line(0, 4, 4)
    );
}

#[test]
fn workspace_symbol_converts_in_sole_and_multi_document_states() {
    // Sole document: the engine resolves a sole conversion document, so
    // dispatch goes through `modify_response` — whose trait default
    // delegates to the standalone hook. The tracked location must still
    // convert; without the delegation this state passed through raw.
    // The second location points inside this test's own temp workspace at
    // a file that was never created — hermetically missing, so the disk
    // fallback provably passes it through unchanged.
    let missing =
        Url::from_file_path(temp_workspace("with_state", "symbol-missing").join("missing.txt"))
            .expect("path converts to a URL");
    let locations = drive_workspace_symbol(&[("a.txt", "🙂abc")], missing.clone());

    assert_eq!(locations[0], (url("a.txt"), same_line(0, 2, 3)));
    assert_eq!(locations[1], (missing, same_line(0, 5, 9)));

    // Two tracked documents with different multibyte layouts: each location
    // must convert against its OWN document ("🙂abc": byte 4 == UTF-16
    // unit 2, byte 5 == unit 3; "x🙂🙂": byte 5 == unit 3, byte 9 ==
    // unit 5) — converting either location against the other document
    // moves it to different columns in both directions. This is the exact
    // state where URL-less responses used to pass through raw.
    let locations = drive_workspace_symbol(&[("a.txt", "🙂abc"), ("b.txt", "x🙂🙂")], url("b.txt"));

    assert_eq!(locations[0], (url("a.txt"), same_line(0, 2, 3)));
    assert_eq!(locations[1], (url("b.txt"), same_line(0, 3, 5)));
}

#[test]
fn untracked_url_converts_against_disk() {
    // "🙂abc" on disk, never opened: byte 4 == UTF-16 unit 2. A second,
    // unrelated document is tracked (with ASCII text, so converting against
    // it would NOT move column 2 to byte 4) — the conversion must read the
    // disk text, not fall back to the sole tracked document.
    let root = temp_workspace("workspace", "untracked-disk");
    let file = root.join("emoji.txt");
    fs::write(&file, "🙂abc").expect("test file can be written");
    let file = fs::canonicalize(file).expect("test file can be canonicalized");
    let disk_url = Url::from_file_path(file).expect("path can be converted to a URL");

    let received = Arc::new(Mutex::new(None));
    let mut server = LanguageServerWithState::new(
        ClientSocket::new_closed(),
        HoverCaptureServer {
            received: Arc::clone(&received),
        },
    );
    server.state.set_position_encoding(Encoding::UTF16);
    let _ = server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(
            url("other.txt"),
            "plaintext".into(),
            0,
            "abcdef".into(),
        ),
    });

    let hover = futures::executor::block_on(server.hover(HoverParams {
        text_document_position_params: TextDocumentPositionParams::new(
            TextDocumentIdentifier::new(disk_url),
            line_position(0, 2),
        ),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }))
    .expect("hover succeeds");

    // Params side: the handler saw the disk text's UTF-8 byte column...
    assert_eq!(
        *received.lock().expect("capture mutex"),
        Some(line_position(0, 4))
    );
    // ...and the response came back in the client's UTF-16 columns.
    let hover = hover.expect("hover present");
    assert_eq!(hover.range.expect("range present"), same_line(0, 2, 2));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn notification_hooks_run_after_the_internal_handlers() {
    // The watched file loads as a Workspace document through the real
    // workspace path, so the did_change_watched_files hook can observe the
    // already-refreshed snapshot.
    let root = temp_workspace("with_state", "hooks");
    let watched = root.join("watched.test");
    fs::write(&watched, "before").expect("test file can be written");
    let watched = fs::canonicalize(watched).expect("test file can be canonicalized");
    let watched_url = Url::from_file_path(&watched).expect("path can be converted to a URL");

    let hooks = Arc::new(Mutex::new(Vec::new()));
    let watched_text = Arc::new(Mutex::new(None));
    let mut server = LanguageServerWithState::new(
        ClientSocket::new_closed(),
        HookRecordingServer {
            hooks: Arc::clone(&hooks),
            watched_url: watched_url.clone(),
            watched_text: Arc::clone(&watched_text),
        },
    );
    server
        .state
        .set_workspace_folders([workspace_folder(&root)]);
    server
        .state
        .refresh_workspace_documents()
        .expect("workspace documents can be refreshed");
    assert_eq!(
        server
            .state
            .document(&watched_url)
            .expect("watched document is tracked")
            .text_contents(),
        "before"
    );

    // Mutate between the snapshot and the event: the hook must see the
    // text the internal handler already refreshed, not the stale one.
    fs::write(&watched, "after").expect("test file can be written");
    drive_notifications(&mut server, watched_url);

    assert_eq!(
        *hooks.lock().expect("hook record mutex"),
        [
            "did_open",
            "did_change",
            "will_save",
            "did_save",
            "did_close",
            "did_change_watched_files",
            "did_create_files",
            "did_rename_files",
            "did_delete_files",
            "work_done_progress_cancel",
            "did_change_configuration",
            "did_change_workspace_folders",
        ]
    );
    assert_eq!(
        *watched_text.lock().expect("hook record mutex"),
        Some("after".into()),
        "the watched-files hook observes the already-refreshed document"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

fn initialize_params(root: &PathBuf) -> InitializeParams {
    InitializeParams {
        process_id: Some(std::process::id()),
        capabilities: ClientCapabilities::default(),
        workspace_folders: Some(vec![workspace_folder(root)]),
        ..Default::default()
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
