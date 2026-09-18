//! The single shared test-support home for inline test modules across the
//! crate.
//!
//! Declared as a `#[cfg(test)]` module in `src/lib.rs`; never compiled into
//! non-test builds. Scopeless like `src/error.rs` — deliberately outside
//! every arch-lint `[[scopes]]` glob — so the harness imports downward into
//! `server` and `text_utils` without adding a layer edge.
//!
//! The `"🙂abc"` document and the UTF-16 encoding in
//! [`state_with_documents`] are load-bearing: U+1F642 is 4 UTF-8 bytes but
//! 2 UTF-16 units, so byte offset 4 == UTF-16 offset 2 — that identity is
//! what the request conversion tests assert.
//!
//! The byte- and tree-sitter `r()` helpers are deliberately not here: each
//! flavor names its local range builder `r`, with types specific to that
//! flavor — they are not (and need not be) the shared LSP fixtures
//! ([`line_position`], [`line_range`], [`same_line`]).
//!
//! The `conversion_tests!` macro — a procedural macro in the workspace
//! `lsp_macros` crate, imported directly from there by test modules — is
//! the table-driven W0 harness: one row stamps the standard conversion
//! test (fixture → `modify_params` → UTF-8 assert → `modify_response` →
//! client assert). Rows pin the single-incoming-position shape; richer
//! tests stay hand-written next to their `Request` impls.

use crate::server::{DocumentMatcher, Server, ServerOptions, ServerState};
use crate::text_utils::Encoding;
use crate::workspace::configure_capabilities;
use async_lsp::ClientSocket;
use async_lsp::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DidOpenTextDocumentParams, InitializeResult, PartialResultParams, Position, Range,
    SemanticToken, ServerCapabilities, TextDocumentItem, Url, WorkDoneProgressParams,
    WorkspaceDiagnosticParams, WorkspaceFolder,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds an LSP [`Position`] with the given line and character.
pub(crate) const fn line_position(line: u32, character: u32) -> Position {
    Position { line, character }
}

/// Builds an LSP [`Range`] spanning from `start` to `end`.
pub(crate) const fn line_range(start: Position, end: Position) -> Range {
    Range { start, end }
}

/// Builds an LSP [`Range`] between two columns of a single line.
pub(crate) const fn same_line(line: u32, start: u32, end: u32) -> Range {
    line_range(line_position(line, start), line_position(line, end))
}

/// Builds a `SemanticToken` with the given relative columns and length
/// (type and modifiers zero).
pub(crate) const fn token(delta_line: u32, delta_start: u32, length: u32) -> SemanticToken {
    SemanticToken {
        delta_line,
        delta_start,
        length,
        token_type: 0,
        token_modifiers_bitset: 0,
    }
}

/// Builds a `file:///tmp/{path}` document URL.
pub(crate) fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}

pub(crate) struct TestServer;

impl Server for TestServer {}

pub(crate) fn open_document(state: &mut ServerState, uri: Url, text: impl Into<String>) {
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri, "test".into(), 1, text.into()),
    });
}

pub(crate) fn state_with_documents() -> (ServerState, Url, Url) {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_position_encoding(Encoding::UTF16);

    let source = url("source.txt");
    let target = url("target.txt");
    open_document(&mut state, source.clone(), "abcdef");
    open_document(&mut state, target.clone(), "🙂abc");

    (state, source, target)
}

/// Builds the `server_capabilities` block of a diagnostic provider: the
/// two advertised flags verbatim, nothing else.
pub(crate) fn diagnostic_provider_capabilities(
    workspace_diagnostics: bool,
    inter_file_dependencies: bool,
) -> ServerCapabilities {
    ServerCapabilities {
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            workspace_diagnostics,
            inter_file_dependencies,
            ..DiagnosticOptions::default()
        })),
        ..ServerCapabilities::default()
    }
}

/// Marks workspace diagnostics as advertised through the same capability
/// merge `initialize` performs: post-initialize machinery (the workspace
/// refresh, close-keep on disk) is gated on `supported`, which starts off
/// before any advertisement exists. Tests that drive that machinery
/// directly, without a live `initialize` call, seed the advertisement with
/// this fixture.
pub(crate) fn advertise_workspace_diagnostics(state: &ServerState) {
    let mut result = InitializeResult {
        capabilities: diagnostic_provider_capabilities(true, false),
        ..InitializeResult::default()
    };
    configure_capabilities(state, &mut result, &ClientCapabilities::default());
}

/// Creates a millisecond-unique temp workspace under `std::env::temp_dir()`.
///
/// `prefix` names the calling test module (`"state"`, `"workspace"`,
/// `"oneshot"`, ...) so a leaked directory can be attributed to its file.
pub(crate) fn temp_workspace(prefix: &str, name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after epoch")
        .as_millis();
    let root = std::env::temp_dir().join(format!("async-language-server-{prefix}-{name}-{millis}"));
    fs::create_dir_all(&root).expect("temp workspace can be created");
    root
}

/// Wraps a workspace root path as a named `WorkspaceFolder`.
pub(crate) fn workspace_folder(path: &PathBuf) -> WorkspaceFolder {
    let uri = Url::from_file_path(path).expect("path can be converted to a URL");
    WorkspaceFolder {
        uri,
        name: "test".into(),
    }
}

/// Builds empty `workspace/diagnostic` params: no identifier, no previous
/// result ids. Callers layer `identifier`/`previous_result_ids` on top via
/// struct-update syntax when the test pins those fields.
pub(crate) fn workspace_diagnostic_params() -> WorkspaceDiagnosticParams {
    WorkspaceDiagnosticParams {
        identifier: None,
        previous_result_ids: Vec::new(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

/// Builds a zero-range diagnostic carrying only a message.
pub(crate) fn diagnostic(message: impl Into<String>) -> Diagnostic {
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

/// Builds the single matcher matching an extension's documents: the
/// `**/*.{extension}` and `*.{extension}` URL globs plus the
/// `extension` language id.
pub(crate) fn extension_matchers(name: &str, extension: &str) -> Vec<DocumentMatcher> {
    vec![
        DocumentMatcher::new(name)
            .with_url_globs([format!("**/*.{extension}"), format!("*.{extension}")])
            .with_lang_strings([extension]),
    ]
}

/// Matchers for plain-text test documents: matched by the `test` language id
/// or the `*.test` URL globs — the fixture suite shared by the state,
/// wrapper, and oneshot tests.
pub(crate) fn test_document_matchers() -> Vec<DocumentMatcher> {
    extension_matchers("Test", "test")
}

/// Matchers for JSON test documents: matched by the `json` language id or
/// the `**/*.json` URL glob, carrying the tree-sitter JSON grammar.
#[cfg(feature = "tree-sitter")]
pub(crate) fn json_matchers() -> Vec<DocumentMatcher> {
    vec![
        DocumentMatcher::new("json")
            .with_url_globs(["**/*.json"])
            .with_lang_strings(["json"])
            .with_lang_grammar(tree_sitter_json::LANGUAGE.into()),
    ]
}

/// Invokes a row closure over an already-converted artifact and asserts the
/// extracted position equals `expected`.
///
/// The closure is taken as an [`Fn`] bound rather than called directly
/// inside `conversion_tests!` because rustc cannot infer the parameter
/// types of an immediately-invoked closure — a plain parenthesized closure
/// call fails identically, so the limitation is that general inference
/// rule, not the `macro_rules!` `expr` metavariable; the
/// `impl Fn(&T) -> Position` bound supplies the expected signature.
pub(crate) fn assert_converted_position<T>(
    value: &T,
    extract: impl Fn(&T) -> Position,
    expected: Position,
    message: &str,
) {
    assert_eq!(extract(value), expected, "{message}");
}

/// Capabilities advertising every gateable dispatch method — the
/// dispatch-row wire fixture (spec W4). The type-hierarchy trio is
/// exempt from the gate and needs no field. Constructor names for the
/// provider-capability enums follow `inventory.rs`'s predicates; the
/// self-verifying test in `src/server/inventory.rs` makes a wrong
/// construction fail loudly, not silently.
pub(crate) fn all_request_capabilities() -> ServerCapabilities {
    use async_lsp::lsp_types::{
        CallHierarchyServerCapability, ColorProviderCapability, DeclarationCapability,
        DiagnosticOptions, DiagnosticServerCapabilities, DocumentOnTypeFormattingOptions,
        ExecuteCommandOptions, FileOperationRegistrationOptions, FoldingRangeProviderCapability,
        HoverProviderCapability, ImplementationProviderCapability,
        LinkedEditingRangeServerCapabilities, OneOf, SelectionRangeProviderCapability,
        SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncOptions,
        TypeDefinitionProviderCapability, WorkspaceFileOperationsServerCapabilities,
        WorkspaceServerCapabilities,
    };

    ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        declaration_provider: Some(DeclarationCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_range_formatting_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: String::new(),
            ..DocumentOnTypeFormattingOptions::default()
        }),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                will_save_wait_until: Some(true),
                ..TextDocumentSyncOptions::default()
            },
        )),
        color_provider: Some(ColorProviderCapability::Simple(true)),
        call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
        moniker_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions::default()),
        semantic_tokens_provider: Some(semantic_tokens_full_delta_provider()),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
            DiagnosticOptions::default(),
        )),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        inline_value_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions::default()),
        workspace: Some(WorkspaceServerCapabilities {
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                will_create: Some(FileOperationRegistrationOptions::default()),
                will_rename: Some(FileOperationRegistrationOptions::default()),
                will_delete: Some(FileOperationRegistrationOptions::default()),
                ..WorkspaceFileOperationsServerCapabilities::default()
            }),
            ..WorkspaceServerCapabilities::default()
        }),
        ..resolve_enabled_providers()
    }
}

/// The provider fields whose shape carries a resolve (or prepare) option:
/// every resolve-family row plus `rename_prepare` gates on these, so the
/// fixture always sets them on.
fn resolve_enabled_providers() -> ServerCapabilities {
    use async_lsp::lsp_types::{
        CodeActionOptions, CodeActionProviderCapability, CodeLensOptions, CompletionOptions,
        DocumentLinkOptions, InlayHintOptions, InlayHintServerCapabilities, OneOf, RenameOptions,
        WorkDoneProgressOptions, WorkspaceSymbolOptions,
    };

    // `DocumentLinkOptions`, `RenameOptions`, and `WorkspaceSymbolOptions`
    // derive no `Default` in lsp-types 0.95.1 — their flattened
    // `work_done_progress_options` is spelled out.
    ServerCapabilities {
        document_link_provider: Some(DocumentLinkOptions {
            resolve_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(true),
        }),
        workspace_symbol_provider: Some(OneOf::Right(WorkspaceSymbolOptions {
            work_done_progress_options: WorkDoneProgressOptions::default(),
            resolve_provider: Some(true),
        })),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions {
                resolve_provider: Some(true),
                ..InlayHintOptions::default()
            },
        ))),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            ..CompletionOptions::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            resolve_provider: Some(true),
            ..CodeActionOptions::default()
        })),
        ..ServerCapabilities::default()
    }
}

/// The richest semantic-tokens shape: it serves the full, range, and
/// full-delta rows at once.
fn semantic_tokens_full_delta_provider() -> async_lsp::lsp_types::SemanticTokensServerCapabilities {
    use async_lsp::lsp_types::{
        SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
        SemanticTokensServerCapabilities,
    };

    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
        legend: SemanticTokensLegend {
            token_types: Vec::new(),
            token_modifiers: Vec::new(),
        },
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
        range: Some(true),
        ..SemanticTokensOptions::default()
    })
}

/// Test-only: opens the dispatch gate entirely (every method allowed).
pub(crate) fn allow_all_methods(state: &mut ServerState) {
    state.set_advertised_methods_all();
}
