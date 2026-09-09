use async_lsp::lsp_types::{CodeActionOrCommand, CodeActionParams};

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_diagnostic, convert_range, convert_workspace_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CodeActionParams,
    response = Option<async_lsp::lsp_types::CodeActionResponse>,
    document(text_document),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct CodeActionRequest;

/// Converts the action range and context diagnostics to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut CodeActionParams) {
    convert_range(state, document, &mut params.range, Direction::Incoming);
    for diag in &mut params.context.diagnostics {
        convert_diagnostic(state, document, diag, Direction::Incoming);
    }
}

/// Converts the returned actions' diagnostics and edits to the client
/// encoding (the outgoing hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<CodeActionOrCommand>>,
) {
    if let Some(actions) = response.as_mut() {
        for action in actions.iter_mut() {
            if let CodeActionOrCommand::CodeAction(action) = action {
                if let Some(diagnostics) = action.diagnostics.as_mut() {
                    for diag in diagnostics {
                        convert_diagnostic(state, document, diag, Direction::Outgoing);
                    }
                }
                if let Some(edit) = action.edit.as_mut() {
                    convert_workspace_edit(state, document, edit, Direction::Outgoing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{
        CodeAction, CodeActionContext, CodeActionOrCommand, CodeActionParams, Diagnostic,
        PartialResultParams, TextDocumentIdentifier, TextEdit, WorkDoneProgressParams,
        WorkspaceEdit,
    };

    use crate::testing::{same_line, state_with_documents};

    use crate::lsp_requests::{CodeActionRequest, Request};

    #[test]
    fn code_action_context_diagnostics_are_converted() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let mut params = CodeActionParams {
            text_document: TextDocumentIdentifier::new(target),
            range: same_line(0, 0, 2),
            context: CodeActionContext {
                diagnostics: vec![Diagnostic {
                    range: same_line(0, 2, 2),
                    message: "diagnostic".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <CodeActionRequest as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.range, same_line(0, 0, 4));
        assert_eq!(params.context.diagnostics[0].range, same_line(0, 4, 4));
    }

    #[test]
    fn code_action_outgoing_hook_converts_diagnostics_and_edits() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let mut response = Some(vec![CodeActionOrCommand::CodeAction(CodeAction {
            title: "action".into(),
            diagnostics: Some(vec![Diagnostic {
                range: same_line(0, 4, 4),
                message: "diagnostic".into(),
                ..Default::default()
            }]),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(
                    target.clone(),
                    vec![TextEdit {
                        range: same_line(0, 4, 4),
                        new_text: "x".into(),
                    }],
                )])),
                ..Default::default()
            }),
            ..Default::default()
        })]);

        <CodeActionRequest as Request>::modify_response(&state, &document, &mut response);

        let actions = response.unwrap();
        let [CodeActionOrCommand::CodeAction(action)] = actions.as_slice() else {
            panic!("expected one code action");
        };
        // Keyed at the emoji document: UTF-8 byte 4 converts to client 2.
        assert_eq!(
            action.diagnostics.as_ref().unwrap()[0].range,
            same_line(0, 2, 2),
        );
        assert_eq!(
            action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&target][0].range,
            same_line(0, 2, 2),
        );
    }
}
