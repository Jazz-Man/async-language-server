# async-language-server

A higher-level abstraction over [async-lsp] for writing language servers with
less boilerplate: tokio stdio/TCP transports, ropey-based incremental document
sync, automatic position-encoding negotiation (UTF-8/16/32), optional
[tree-sitter] integration, and workspace-wide diagnostics.

## Quick start

Implement the `Server` trait with only the methods you need and run it:

```rust,no_run
use std::future::Future;

use async_language_server::lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use async_language_server::server::{
    DocumentMatcher, Server, ServerResult, ServerState, Transport, serve,
};

#[derive(Clone)]
struct MyServer;

impl Server for MyServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("my-lang").with_lang_strings(["mylang"])]
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send {
        // `state.document` returns the document snapshot; analyze it and
        // build the diagnostics report from your findings.
        let _document = state.document(&params.text_document.uri);
        std::future::ready(Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            }),
        )))
    }
}

#[tokio::main]
async fn main() -> ServerResult<()> {
    serve(Transport::Stdio, MyServer).await
}
```

Every `Server` method receives and produces **UTF-8** positions regardless of
the encoding negotiated with the client — conversions are handled internally.

## Tour

- **`Server` trait** (`server::Server`) — optional async handlers: hover,
  completion, definition, references, rename, formatting, diagnostics, …
  Unimplemented methods answer `METHOD_NOT_FOUND`.
- **`DocumentMatcher`** — associates documents with a language by URL globs
  and/or language-id strings, optionally carrying a tree-sitter grammar
  (language-per-document).
- **`serve()` + `Transport`** — wires your server into async-lsp behind a
  tower middleware stack (tracing, concurrency limit, panic catching,
  client-process monitor) over stdio or a TCP socket.
- **Workspace diagnostics** — `workspace/diagnostic` with walker-based
  scanning; exposure configured through `ServerOptions`.
- **`oneshot`** — run a `Server` over files on disk with no LSP client:
  CLI-style batch diagnostics.
- **`text_utils`** — `Encoding`, `Position`, and range helpers behind the
  transparent encoding conversion.

## Feature flags

Both default on: `tracing` (middleware + handler logging) and `tree-sitter`
(per-document grammars, `tree_sitter_utils`).

## Stability

This crate is the owner's fork of an upstream framework: version 0.0.0, not
published to crates.io, and consumed by pinning a revision or tag. Breaking
changes are named in commit messages; there is no semver safety net.

[async-lsp]: https://crates.io/crates/async-lsp
[tree-sitter]: https://tree-sitter.github.io/tree-sitter/
