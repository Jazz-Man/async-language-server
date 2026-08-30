//! A language server that parses JSON with tree-sitter and reports syntax errors.
//!
//! Run from an LSP client that launches it over stdio:
//!
//! ```text
//! cargo run --example tree_sitter
//! ```

use async_language_server::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};
use async_language_server::server::{
    DocumentMatcher, Server, ServerResult, ServerState, Transport, serve,
};

#[derive(Clone)]
struct JsonServer;

impl Server for JsonServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![
            DocumentMatcher::new("json")
                .with_url_globs(["**/*.json"])
                .with_lang_grammar(tree_sitter_json::LANGUAGE.into()),
        ]
    }

    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("json".into()),
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
        if document.has_syntax_tree() {
            // The tree is parsed and incrementally updated by the crate;
            // query it for parser ERROR nodes.
            for capture in document.query("(ERROR) @error").into_iter().flatten() {
                items.push(Diagnostic {
                    range: capture.range,
                    message: "syntax error".to_owned(),
                    severity: Some(DiagnosticSeverity::ERROR),
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
    serve(Transport::Stdio, JsonServer).await
}
