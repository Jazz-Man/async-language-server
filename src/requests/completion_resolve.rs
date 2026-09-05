use async_lsp::lsp_types::CompletionItem;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_completion_text_edit, convert_text_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CompletionItem,
    response = async_lsp::lsp_types::CompletionItem,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct CompletionResolveRequest;

// CompletionItem doesn't contain a document URI; the resolve dispatch
// engine supplies the sole tracked document.

/// Converts the item's edits to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut CompletionItem) {
    convert_completion_item(state, document, params, Direction::Incoming);
}

/// Converts the item's edits back to the client encoding (the outgoing hook).
fn convert_response(state: &ServerState, document: &Document, response: &mut CompletionItem) {
    convert_completion_item(state, document, response, Direction::Outgoing);
}

/// Converts a completion item's edits between the client encoding and UTF-8
/// against the given document snapshot, leaving every other field as-is.
fn convert_completion_item(
    state: &ServerState,
    document: &Document,
    item: &mut CompletionItem,
    direction: Direction,
) {
    if let Some(edit) = item.text_edit.as_mut() {
        convert_completion_text_edit(state, document, edit, direction);
    }

    if let Some(additional_edits) = item.additional_text_edits.as_mut() {
        for edit in additional_edits.iter_mut() {
            convert_text_edit(state, document, edit, direction);
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{CompletionItem, CompletionTextEdit, TextEdit};

    use crate::requests::{CompletionResolveRequest, Direction, convert_resolve_item};
    use crate::server::{ServerOptions, ServerState};
    use crate::testing::{TestServer, open_document, same_line, state_with_documents, url};
    use crate::text_utils::Encoding;

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
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                same_line(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        let document = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<CompletionResolveRequest, _>(
            &state,
            Some(&document),
            &mut item,
            Direction::Outgoing,
        );

        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, same_line(0, 2, 2));
    }

    #[test]
    fn resolve_edits_pass_through_without_a_document() {
        // No document snapshot: the edits pass through unchanged.
        let (state, _, _) = state_with_documents();

        let mut item = CompletionItem {
            label: "item".into(),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                same_line(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        convert_resolve_item::<CompletionResolveRequest, _>(
            &state,
            None,
            &mut item,
            Direction::Outgoing,
        );

        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, same_line(0, 4, 4));
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
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                same_line(0, 2, 2),
                "x".into(),
            ))),
            ..Default::default()
        };

        let sole = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<CompletionResolveRequest, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Incoming,
        );
        convert_resolve_item::<CompletionResolveRequest, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Outgoing,
        );

        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, same_line(0, 2, 2));
    }
}
