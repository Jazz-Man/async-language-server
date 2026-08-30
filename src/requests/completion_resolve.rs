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
