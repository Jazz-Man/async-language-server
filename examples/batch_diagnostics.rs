//! A batch-diagnostics CLI over the same `Server` implementation the editor
//! uses — the second deployment mode of one server.
//!
//! `examples/minimal.rs` runs `LongLineServer` as an editor LSP over stdio;
//! this example runs the *same* validation logic headlessly through
//! `oneshot::workspace_diagnostics`: no LSP client, no transport — the crate
//! drives the server's document management and dispatch directly over files
//! on disk. That is what `oneshot` exists for: `mylang check ./src`-style
//! commands, CI runs, and pre-commit hooks, all sharing the editor server's
//! logic instead of reimplementing it.
//!
//! Exits 0 when the workspace is clean, 1 when diagnostics are found:
//!
//! ```text
//! cargo run --example batch_diagnostics -- <directory-to-lint>
//! ```

use std::io::Write as _;

use async_language_server::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Position, Range, RelatedFullDocumentDiagnosticReport,
    ServerCapabilities,
};
use async_language_server::oneshot::{WorkspaceDiagnosticConfig, workspace_diagnostics};
use async_language_server::server::{
    DocumentMatcher, Server, ServerError, ServerResult, ServerState,
};

/// Lines longer than this many bytes are reported — the same rule as
/// `examples/minimal.rs`.
const MAX_LINE_BYTES: usize = 80;

#[derive(Clone)]
struct LongLineServer;

impl Server for LongLineServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        // Which files the batch scan picks up. The editor path uses the
        // same matcher for workspace-wide features.
        vec![DocumentMatcher::new("long-lines").with_url_globs(["**/*.txt"])]
    }

    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("long-lines".into()),
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..Default::default()
            })),
            ..ServerCapabilities::default()
        })
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send
    {
        let Some(document) = state.document(&params.text_document.uri) else {
            return std::future::ready(Ok(full_report(Vec::new())));
        };

        let mut items = Vec::new();
        for (line, text) in document.text_contents().lines().enumerate() {
            let length = text.len();
            if length > MAX_LINE_BYTES {
                items.push(Diagnostic {
                    range: Range::new(
                        Position::new(u32::try_from(line).unwrap_or(u32::MAX), 0),
                        Position::new(
                            u32::try_from(line).unwrap_or(u32::MAX),
                            u32::try_from(length).unwrap_or(u32::MAX),
                        ),
                    ),
                    message: format!(
                        "line is {length} bytes long, over the {MAX_LINE_BYTES}-byte limit",
                    ),
                    ..Diagnostic::default()
                });
            }
        }

        std::future::ready(Ok(full_report(items)))
    }
}

fn full_report(items: Vec<Diagnostic>) -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items,
            },
        },
    ))
}

#[tokio::main]
async fn main() -> ServerResult<()> {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());

    let report =
        workspace_diagnostics(LongLineServer, WorkspaceDiagnosticConfig::new(&root)).await?;

    // CLI output is this example's product, written through ordinary IO
    // calls; the print-macro lints guard the library, whose stdout is the
    // LSP protocol channel — this example never runs a stdio transport.
    let mut found = 0usize;
    let mut out = std::io::stdout().lock();
    for document in &report.documents {
        let diagnostics = document.diagnostics();
        if diagnostics.is_empty() {
            continue;
        }
        found += diagnostics.len();
        let _ = writeln!(out, "{}", document.uri.path());
        for diagnostic in diagnostics {
            let line = diagnostic.range.start.line + 1;
            let column = diagnostic.range.start.character + 1;
            let _ = writeln!(out, "  {line}:{column}: {}", diagnostic.message);
        }
    }
    let _ = writeln!(
        out,
        "{found} diagnostics in {} files",
        report.documents.len(),
    );

    if found > 0 {
        // Returning Err from main exits 1 — the CLI's "checks failed" code.
        return Err(ServerError::Other(
            format!("{found} diagnostics found").into(),
        ));
    }
    Ok(())
}
