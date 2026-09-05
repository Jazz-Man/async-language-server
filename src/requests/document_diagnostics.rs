use async_lsp::lsp_types::{DocumentDiagnosticReport, DocumentDiagnosticReportResult};

use crate::server::{Document, ServerState};

use super::conversion::{
    Direction, convert_diagnostic, modify_outgoing_diagnostic_report_kind_at_url,
};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::DocumentDiagnosticParams,
    response = async_lsp::lsp_types::DocumentDiagnosticReportResult,
    document(text_document),
    outgoing(self::convert_response),
)]
pub(crate) struct DocumentDiagnosticsRequest;

/// Converts the report's diagnostics to the client encoding (the outgoing
/// hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut DocumentDiagnosticReportResult,
) {
    match response {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) => {
            for diag in &mut report.full_document_diagnostic_report.items {
                convert_diagnostic(state, document, diag, Direction::Outgoing);
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{
        Diagnostic, DocumentDiagnosticReport, DocumentDiagnosticReportKind,
        DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
        RelatedFullDocumentDiagnosticReport,
    };

    use crate::testing::{same_line, state_with_documents};

    use crate::requests::{DocumentDiagnosticsRequest, Request};

    #[test]
    fn document_diagnostic_related_documents_are_converted_using_their_own_document() {
        let (state, source, target) = state_with_documents();
        let document = state.document(&source).unwrap();
        let mut response = DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
            RelatedFullDocumentDiagnosticReport {
                related_documents: Some(HashMap::from([(
                    target.clone(),
                    DocumentDiagnosticReportKind::Full(FullDocumentDiagnosticReport {
                        result_id: None,
                        items: vec![Diagnostic {
                            range: same_line(0, 4, 4),
                            message: "diagnostic".into(),
                            ..Default::default()
                        }],
                    }),
                )])),
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            },
        ));

        <DocumentDiagnosticsRequest as Request>::modify_response(&state, &document, &mut response);

        let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) =
            response
        else {
            panic!("expected full diagnostic report");
        };
        let Some(DocumentDiagnosticReportKind::Full(report)) =
            report.related_documents.unwrap().remove(&target)
        else {
            panic!("expected full related diagnostic report");
        };
        assert_eq!(report.items[0].range, same_line(0, 2, 2));
    }
}
