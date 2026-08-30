//! A minimal language server that reports over-long lines as diagnostics.
//!
//! Run from an LSP client that launches it over stdio:
//!
//! ```text
//! cargo run --example minimal
//! ```

use async_language_server::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Position, Range, RelatedFullDocumentDiagnosticReport,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};
use async_language_server::server::{Server, ServerResult, ServerState, Transport, serve};

/// Lines longer than this many bytes are reported.
const MAX_LINE_BYTES: usize = 80;

#[derive(Clone)]
struct LongLineServer;

impl Server for LongLineServer {
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
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
                        "line is {length} bytes long, over the {MAX_LINE_BYTES}-byte limit"
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
    serve(Transport::Stdio, LongLineServer).await
}
