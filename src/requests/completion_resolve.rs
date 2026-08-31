use async_lsp::lsp_types::CompletionItem as LspCompletionItem;

use crate::{
    server::{Document, ServerState},
    text_utils::Encoding,
};

use super::{
    Request,
    conversion::{
        modify_incoming_completion_text_edit, modify_incoming_text_edit,
        modify_outgoing_completion_text_edit, modify_outgoing_text_edit,
    },
};

pub struct CompletionResolve;

impl Request for CompletionResolve {
    type Params = LspCompletionItem;
    type Response = LspCompletionItem;

    // CompletionItem doesn't contain a document URI

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        if let Some(edit) = params.text_edit.as_mut() {
            modify_incoming_completion_text_edit(state, document, edit);
        }

        if let Some(additional_edits) = params.additional_text_edits.as_mut() {
            for edit in additional_edits.iter_mut() {
                modify_incoming_text_edit(state, document, edit);
            }
        }
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(edit) = response.text_edit.as_mut() {
            modify_outgoing_completion_text_edit(state, document, edit);
        }

        if let Some(additional_edits) = response.additional_text_edits.as_mut() {
            for edit in additional_edits.iter_mut() {
                modify_outgoing_text_edit(state, document, edit);
            }
        }
    }
}

/// Converts a resolve request's item to UTF-8 against the given document
/// snapshot.
///
/// Resolve requests carry no document URL, so there is no request document:
/// the caller supplies the snapshot to convert against — in the usual
/// completion-then-resolve flow the sole tracked document, captured once for
/// the whole request. Without one, the item passes through unchanged.
pub(crate) fn convert_incoming_completion_resolve(
    state: &ServerState,
    document: Option<&Document>,
    item: &mut LspCompletionItem,
) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let Some(document) = document else {
        return;
    };
    <CompletionResolve as Request>::modify_params(state, document, item);
}

/// Converts a resolve response's edits against the given document snapshot.
///
/// Resolve requests carry no document URL, so there is no request document:
/// the caller supplies the snapshot to convert against — in the usual
/// completion-then-resolve flow the sole tracked document, captured once for
/// the whole request. Without one, the response passes through unchanged.
pub(crate) fn convert_completion_resolve(
    state: &ServerState,
    document: Option<&Document>,
    response: &mut LspCompletionItem,
) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let Some(document) = document else {
        return;
    };
    <CompletionResolve as Request>::modify_response(state, document, response);
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{
        CompletionItem, CompletionTextEdit as LspCompletionTextEdit, TextEdit,
    };

    use crate::server::{ServerOptions, ServerState};
    use crate::testing::{TestServer, open_document, same_line, state_with_documents, url};
    use crate::text_utils::Encoding;

    use super::{convert_completion_resolve, convert_incoming_completion_resolve};

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
                same_line(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        let document = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_completion_resolve(&state, Some(&document), &mut item);

        let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
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
            text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
                same_line(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        convert_completion_resolve(&state, None, &mut item);

        let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
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
            text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
                same_line(0, 2, 2),
                "x".into(),
            ))),
            ..Default::default()
        };

        let sole = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_incoming_completion_resolve(&state, Some(&sole), &mut item);
        convert_completion_resolve(&state, Some(&sole), &mut item);

        let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, same_line(0, 2, 2));
    }
}
