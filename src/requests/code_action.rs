use async_lsp::lsp_types::{
    CodeActionOrCommand as LspCodeActionOrCommand, CodeActionParams as LspCodeActionParams, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        modify_incoming_diagnostic, modify_incoming_range, modify_outgoing_diagnostic,
        modify_outgoing_workspace_edit,
    },
};

pub struct CodeAction;

impl Request for CodeAction {
    type Params = LspCodeActionParams;
    type Response = Option<Vec<LspCodeActionOrCommand>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_range(state, document, &mut params.range);
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
                        modify_outgoing_workspace_edit(state, document, edit);
                    }
                }
            }
        }
    }
}
