use async_lsp::lsp_types::CodeAction;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_diagnostic, convert_workspace_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CodeAction,
    response = async_lsp::lsp_types::CodeAction,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct CodeActionResolveRequest;

// CodeAction doesn't contain a document URI; the resolve dispatch
// engine supplies the sole tracked document.

/// Converts the action's diagnostics and edits to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut CodeAction) {
    convert_code_action(state, document, params, Direction::Incoming);
}

/// Converts the action's diagnostics and edits back to the client encoding
/// (the outgoing hook).
fn convert_response(state: &ServerState, document: &Document, response: &mut CodeAction) {
    convert_code_action(state, document, response, Direction::Outgoing);
}

/// Converts a code action's diagnostics and workspace edit between the
/// client encoding and UTF-8 against the given document snapshot, leaving
/// every other field as-is.
fn convert_code_action(
    state: &ServerState,
    document: &Document,
    action: &mut CodeAction,
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
