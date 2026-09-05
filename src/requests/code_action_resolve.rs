use async_lsp::lsp_types::CodeAction as LspCodeAction;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_diagnostic, convert_workspace_edit},
};

pub(crate) struct CodeActionResolve;

impl Request for CodeActionResolve {
    type Params = LspCodeAction;
    type Response = LspCodeAction;

    // CodeAction doesn't contain a document URI

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_code_action(state, document, params, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_code_action(state, document, response, Direction::Outgoing);
    }
}

/// Converts a code action's diagnostics and workspace edit between the
/// client encoding and UTF-8 against the given document snapshot, leaving
/// every other field as-is.
fn convert_code_action(
    state: &ServerState,
    document: &Document,
    action: &mut LspCodeAction,
    direction: Direction,
) {
    if let Some(diagnostics) = action.diagnostics.as_mut() {
        for diag in diagnostics {
            convert_diagnostic(state, document, diag, direction);
        }
    }
    if let Some(edit) = action.edit.as_mut() {
        convert_workspace_edit(state, document, edit, direction);
    }
}
