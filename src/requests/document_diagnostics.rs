use async_lsp::lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_outgoing_diagnostic, modify_outgoing_diagnostic_report_kind_at_url},
};

pub struct DocumentDiagnostics;

impl Request for DocumentDiagnostics {
    type Params = DocumentDiagnosticParams;
    type Response = DocumentDiagnosticReportResult;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        match response {
            DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) => {
                for diag in &mut report.full_document_diagnostic_report.items {
                    modify_outgoing_diagnostic(state, document, diag);
                }
                if let Some(related) = report.related_documents.as_mut() {
                    for (uri, report) in related {
                        modify_outgoing_diagnostic_report_kind_at_url(state, document, uri, report);
                    }
                }
            }
            DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(report)) => {
                if let Some(related) = report.related_documents.as_mut() {
                    for (uri, report) in related {
                        modify_outgoing_diagnostic_report_kind_at_url(state, document, uri, report);
                    }
                }
            }
            DocumentDiagnosticReportResult::Partial(report) => {
                if let Some(related) = report.related_documents.as_mut() {
                    for (uri, report) in related {
                        modify_outgoing_diagnostic_report_kind_at_url(state, document, uri, report);
                    }
                }
            }
        }
    }
}
