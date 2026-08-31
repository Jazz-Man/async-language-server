use async_lsp::lsp_types::CodeAction as LspCodeAction;

use crate::{
    server::{Document, ServerState},
    text_utils::Encoding,
};

use super::{
    Request,
    conversion::{
        Direction, convert_workspace_edit, modify_incoming_diagnostic, modify_outgoing_diagnostic,
    },
};

pub struct CodeActionResolve;

impl Request for CodeActionResolve {
    type Params = LspCodeAction;
    type Response = LspCodeAction;

    // CodeAction doesn't contain a document URI

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        if let Some(diagnostics) = params.diagnostics.as_mut() {
            for diag in diagnostics {
                modify_incoming_diagnostic(state, document, diag);
            }
        }
        if let Some(edit) = params.edit.as_mut() {
            convert_workspace_edit(state, document, edit, Direction::Incoming);
        }
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(diagnostics) = response.diagnostics.as_mut() {
            for diag in diagnostics {
                modify_outgoing_diagnostic(state, document, diag);
            }
        }
        if let Some(edit) = response.edit.as_mut() {
            convert_workspace_edit(state, document, edit, Direction::Outgoing);
        }
    }
}

/// Converts a resolve request's action to UTF-8 against the given document
/// snapshot.
///
/// Resolve requests carry no document URL, so there is no request document:
/// the caller supplies the snapshot to convert against — in the usual
/// code-action-then-resolve flow the sole tracked document, captured once
/// for the whole request. Without one, the action passes through unchanged.
pub(crate) fn convert_incoming_code_action_resolve(
    state: &ServerState,
    document: Option<&Document>,
    action: &mut LspCodeAction,
) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let Some(document) = document else {
        return;
    };
    <CodeActionResolve as Request>::modify_params(state, document, action);
}

/// Converts a resolve response's diagnostics and edits against the given
/// document snapshot.
///
/// Resolve requests carry no document URL, so there is no request document:
/// the caller supplies the snapshot to convert against — in the usual
/// code-action-then-resolve flow the sole tracked document, captured once
/// for the whole request. Without one, the response passes through
/// unchanged.
pub(crate) fn convert_code_action_resolve(
    state: &ServerState,
    document: Option<&Document>,
    response: &mut LspCodeAction,
) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let Some(document) = document else {
        return;
    };
    <CodeActionResolve as Request>::modify_response(state, document, response);
}
