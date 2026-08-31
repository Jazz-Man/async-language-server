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

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{CompletionItem, CompletionResponse, TextEdit};

    use crate::requests::testing::{r, state_with_documents};

    use super::{Completion, Request};

    #[test]
    fn completion_additional_text_edits_are_converted() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let mut response = Some(CompletionResponse::Array(vec![CompletionItem {
            label: "item".into(),
            additional_text_edits: Some(vec![TextEdit::new(r(0, 4, 4), "x".into())]),
            ..Default::default()
        }]));

        <Completion as Request>::modify_response(&state, &document, &mut response);

        let Some(CompletionResponse::Array(items)) = response else {
            panic!("expected completion array");
        };
        assert_eq!(
            items[0].additional_text_edits.as_ref().unwrap()[0].range,
            r(0, 2, 2),
        );
    }
}
