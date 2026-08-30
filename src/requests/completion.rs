use async_lsp::lsp_types::{
    CompletionParams as LspCompletionParams, CompletionResponse as LspCompletionResponse, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        modify_incoming_position, modify_outgoing_completion_text_edit, modify_outgoing_text_edit,
    },
};

pub struct Completion;

impl Request for Completion {
    type Params = LspCompletionParams;
    type Response = Option<LspCompletionResponse>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document_position.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_position(state, document, &mut params.text_document_position.position);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            let items = match response {
                LspCompletionResponse::Array(v) => v,
                LspCompletionResponse::List(v) => v.items.as_mut(),
            };
            for item in items {
                if let Some(edit) = item.text_edit.as_mut() {
                    modify_outgoing_completion_text_edit(state, document, edit);
                }
                if let Some(edits) = item.additional_text_edits.as_mut() {
                    for edit in edits {
                        modify_outgoing_text_edit(state, document, edit);
                    }
                }
            }
        }
    }
}
