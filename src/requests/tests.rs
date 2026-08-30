use std::collections::HashMap;

use async_lsp::{
    ClientSocket,
    lsp_types::{
        CodeActionContext, CodeActionParams, CompletionItem, CompletionResponse,
        CompletionTextEdit as LspCompletionTextEdit, Diagnostic, DidOpenTextDocumentParams,
        DocumentDiagnosticReport, DocumentDiagnosticReportKind, DocumentDiagnosticReportResult,
        FullDocumentDiagnosticReport, GotoDefinitionResponse, Location, PartialResultParams,
        Position, Range, RelatedFullDocumentDiagnosticReport, TextDocumentIdentifier,
        TextDocumentItem, TextEdit, Url, WorkDoneProgressParams, WorkspaceEdit,
    },
};

use crate::{
    server::{Server, ServerOptions, ServerState},
    text_utils::Encoding,
};

use super::{CodeAction, Completion, Definition, DocumentDiagnostics, Rename, Request};

struct TestServer;

impl Server for TestServer {}

fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}

const fn p(line: u32, character: u32) -> Position {
    Position { line, character }
}

const fn r(line: u32, start: u32, end: u32) -> Range {
    Range {
        start: p(line, start),
        end: p(line, end),
    }
}

fn open_document(state: &mut ServerState, uri: Url, text: impl Into<String>) {
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri, "test".into(), 1, text.into()),
    });
}

fn state_with_documents() -> (ServerState, Url, Url) {
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

#[test]
fn definition_locations_are_converted_using_their_own_document() {
    let (state, source, target) = state_with_documents();
    let document = state.document(&source).unwrap();
    let mut response = Some(GotoDefinitionResponse::Scalar(Location::new(
        target,
        r(0, 4, 4),
    )));

    <Definition as Request>::modify_response(&state, &document, &mut response);

    let Some(GotoDefinitionResponse::Scalar(loc)) = response else {
        panic!("expected scalar location");
    };
    assert_eq!(loc.range, r(0, 2, 2));
}

#[test]
fn workspace_edits_are_converted_using_their_own_document() {
    let (state, source, target) = state_with_documents();
    let document = state.document(&source).unwrap();
    let mut response = Some(WorkspaceEdit {
        changes: Some(HashMap::from([(
            target,
            vec![TextEdit::new(r(0, 4, 4), "x".into())],
        )])),
        ..Default::default()
    });

    <Rename as Request>::modify_response(&state, &document, &mut response);

    let edit = response.unwrap();
    let edit = edit.changes.unwrap().into_values().next().unwrap();
    assert_eq!(edit[0].range, r(0, 2, 2));
}

#[test]
fn completion_additional_text_edits_are_converted() {
    let (state, _, target) = state_with_documents();
    let document = state.document(&target).unwrap();
    let mut response = Some(CompletionResponse::Array(vec![CompletionItem {
        label: "item".into(),
        additional_text_edits: Some(vec![TextEdit::new(r(0, 4, 4), "x".into())]),
        ..Default::default()
    }]));

    <Completion as Request>::modify_response(&state, &document, &mut response);

    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected completion array");
    };
    assert_eq!(
        items[0].additional_text_edits.as_ref().unwrap()[0].range,
        r(0, 2, 2),
    );
}

#[test]
fn code_action_context_diagnostics_are_converted() {
    let (state, _, target) = state_with_documents();
    let document = state.document(&target).unwrap();
    let mut params = CodeActionParams {
        text_document: TextDocumentIdentifier::new(target),
        range: r(0, 0, 2),
        context: CodeActionContext {
            diagnostics: vec![Diagnostic {
                range: r(0, 2, 2),
                message: "diagnostic".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    <CodeAction as Request>::modify_params(&state, &document, &mut params);

    assert_eq!(params.range, r(0, 0, 4));
    assert_eq!(params.context.diagnostics[0].range, r(0, 4, 4));
}

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
                        range: r(0, 4, 4),
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

    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = response
    else {
        panic!("expected full diagnostic report");
    };
    let Some(DocumentDiagnosticReportKind::Full(report)) =
        report.related_documents.unwrap().remove(&target)
    else {
        panic!("expected full related diagnostic report");
    };
    assert_eq!(report.items[0].range, r(0, 2, 2));
}

#[test]
fn rename_edits_fall_back_to_request_document_when_target_is_unknown() {
    let (state, _, target) = state_with_documents();
    let document = state.document(&target).unwrap();
    let missing = url("missing.txt");
    let mut response = Some(WorkspaceEdit {
        changes: Some(HashMap::from([(
            missing,
            vec![TextEdit::new(r(0, 4, 4), "x".into())],
        )])),
        ..Default::default()
    });

    <Rename as Request>::modify_response(&state, &document, &mut response);

    let edit = response.unwrap();
    let edit = edit.changes.unwrap().into_values().next().unwrap();
    assert_eq!(edit[0].range, r(0, 2, 2));
}

#[test]
fn resolve_edits_convert_against_the_sole_tracked_document() {
    // Exactly one tracked document ("🙂abc"), UTF-16 negotiated.
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_position_encoding(Encoding::UTF16);
    open_document(&mut state, url("only.txt"), "🙂abc");

    let mut item = CompletionItem {
        label: "item".into(),
        text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
            r(0, 4, 4),
            "x".into(),
        ))),
        ..Default::default()
    };

    let document = state
        .document(&url("only.txt"))
        .expect("sole document is tracked");
    super::convert_completion_resolve(&state, Some(&document), &mut item);

    let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("expected edit");
    };
    assert_eq!(edit.range, r(0, 2, 2));
}

#[test]
fn resolve_edits_pass_through_without_a_document() {
    // No document snapshot: the edits pass through unchanged.
    let (state, _, _) = state_with_documents();

    let mut item = CompletionItem {
        label: "item".into(),
        text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
            r(0, 4, 4),
            "x".into(),
        ))),
        ..Default::default()
    };

    super::convert_completion_resolve(&state, None, &mut item);

    let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("expected edit");
    };
    assert_eq!(edit.range, r(0, 4, 4));
}

#[test]
fn resolve_echo_round_trip_is_identity() {
    // Sole doc "🙂abc", UTF-16 negotiated. The client echoes the edit at
    // the UTF-16 position it was delivered: the incoming converter must
    // turn it into UTF-8 for the handler, and the outgoing converter must
    // return the original position — no double conversion.
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_position_encoding(Encoding::UTF16);
    open_document(&mut state, url("only.txt"), "🙂abc");

    let mut item = CompletionItem {
        label: "item".into(),
        text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
            r(0, 2, 2),
            "x".into(),
        ))),
        ..Default::default()
    };

    let sole = state
        .document(&url("only.txt"))
        .expect("sole document is tracked");
    super::convert_incoming_completion_resolve(&state, Some(&sole), &mut item);
    super::convert_completion_resolve(&state, Some(&sole), &mut item);

    let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
        panic!("expected edit");
    };
    assert_eq!(edit.range, r(0, 2, 2));
}
