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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{CodeAction, Diagnostic, Range, TextEdit, WorkspaceEdit};

    use crate::lsp_requests::{CodeActionResolveRequest, Request};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn code_action_resolve_hooks_convert_in_both_directions() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let action = |range: Range| CodeAction {
            title: "action".into(),
            diagnostics: Some(vec![Diagnostic {
                range,
                message: "diagnostic".into(),
                ..Default::default()
            }]),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(
                    target.clone(),
                    vec![TextEdit {
                        range,
                        new_text: "x".into(),
                    }],
                )])),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Incoming: the resolve request arrives in the client encoding
        // (UTF-16) and must reach the handler as UTF-8.
        let mut incoming = action(same_line(0, 2, 2));
        <CodeActionResolveRequest as Request>::modify_params(&state, &document, &mut incoming);
        assert_eq!(incoming.diagnostics.unwrap()[0].range, same_line(0, 4, 4));
        assert_eq!(
            incoming.edit.unwrap().changes.unwrap()[&target][0].range,
            same_line(0, 4, 4),
        );

        // Outgoing: the resolved action leaves as UTF-8 and must reach the
        // client in its encoding.
        let mut outgoing = action(same_line(0, 4, 4));
        <CodeActionResolveRequest as Request>::modify_response(&state, &document, &mut outgoing);
        assert_eq!(outgoing.diagnostics.unwrap()[0].range, same_line(0, 2, 2));
        assert_eq!(
            outgoing.edit.unwrap().changes.unwrap()[&target][0].range,
            same_line(0, 2, 2),
        );
    }
}
