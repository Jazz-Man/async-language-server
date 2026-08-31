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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{
        Diagnostic, DocumentDiagnosticReport, DocumentDiagnosticReportKind,
        DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
        RelatedFullDocumentDiagnosticReport,
    };

    use crate::testing::{same_line, state_with_documents};

    use super::{DocumentDiagnostics, Request};

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

        <DocumentDiagnostics as Request>::modify_response(&state, &document, &mut response);

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
