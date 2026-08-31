//! Test-only baseline for per-request conversion tests.
//!
//! Declared as `#[cfg(test)] mod testing;` in `mod.rs`; never compiled into
//! non-test builds. The `"🙂abc"` document and the UTF-16 encoding are
//! load-bearing: U+1F642 is 4 UTF-8 bytes but 2 UTF-16 units, so byte
//! offset 4 == UTF-16 offset 2 — that identity is what these tests assert.

use async_lsp::{
    ClientSocket,
    lsp_types::{DidOpenTextDocumentParams, Position, Range, TextDocumentItem, Url},
};

use crate::{
    server::{Server, ServerOptions, ServerState},
    text_utils::Encoding,
};

pub(crate) struct TestServer;

impl Server for TestServer {}

pub(crate) fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}

pub(crate) const fn p(line: u32, character: u32) -> Position {
    Position { line, character }
}

pub(crate) const fn r(line: u32, start: u32, end: u32) -> Range {
    Range {
        start: p(line, start),
        end: p(line, end),
    }
}

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
