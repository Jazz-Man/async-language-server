use async_lsp::lsp_types::{
    CodeActionOrCommand as LspCodeActionOrCommand, CodeActionParams as LspCodeActionParams,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        Direction, convert_range, convert_workspace_edit, modify_incoming_diagnostic,
        modify_outgoing_diagnostic,
    },
};

pub struct CodeAction;

impl Request for CodeAction {
    type Params = LspCodeActionParams;
    type Response = Option<Vec<LspCodeActionOrCommand>>;

    request_extract_url!(text_document);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
        for diag in &mut params.context.diagnostics {
            modify_incoming_diagnostic(state, document, diag);
        }
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(actions) = response.as_mut() {
            for action in actions.iter_mut() {
                if let LspCodeActionOrCommand::CodeAction(action) = action {
                    if let Some(diagnostics) = action.diagnostics.as_mut() {
                        for diag in diagnostics {
                            modify_outgoing_diagnostic(state, document, diag);
                        }
                    }
                    if let Some(edit) = action.edit.as_mut() {
                        convert_workspace_edit(state, document, edit, Direction::Outgoing);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CodeActionContext, CodeActionParams, Diagnostic, PartialResultParams,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::testing::{same_line, state_with_documents};

    use super::{CodeAction, Request};

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

        <CodeAction as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.range, same_line(0, 0, 4));
        assert_eq!(params.context.diagnostics[0].range, same_line(0, 4, 4));
    }
}
