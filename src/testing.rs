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
//! (`line_position`, `line_range`, `same_line`).
//!
//! The `conversion_tests!` macro — a procedural macro in the workspace
//! `lsp_macros` crate, imported directly from there by test modules — is
//! the table-driven W0 harness: one row stamps the standard conversion
//! test (fixture → `modify_params` → UTF-8 assert → `modify_response` →
//! client assert). Rows pin the single-incoming-position shape; richer
//! tests stay hand-written next to their `Request` impls.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use async_lsp::{
    ClientSocket,
    lsp_types::{
        Diagnostic, DidOpenTextDocumentParams, Position, Range, SemanticToken, TextDocumentItem,
        Url, WorkspaceFolder,
    },
};

use crate::server::{Server, ServerOptions, ServerState};
use crate::text_utils::Encoding;

#[cfg(feature = "tree-sitter")]
use crate::server::DocumentMatcher;

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
