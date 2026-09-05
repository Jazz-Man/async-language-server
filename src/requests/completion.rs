use async_lsp::lsp_types::CompletionResponse;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_completion_text_edit, convert_text_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CompletionParams,
    response = Option<async_lsp::lsp_types::CompletionResponse>,
    document(text_document_position.text_document),
    incoming_position(text_document_position.position),
    outgoing(self::convert_response),
)]
pub(crate) struct CompletionRequest;

/// Converts completion edits in the response back to the client encoding
/// (the outgoing hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<CompletionResponse>,
) {
    if let Some(response) = response.as_mut() {
        let items = match response {
            CompletionResponse::Array(v) => v,
            CompletionResponse::List(v) => v.items.as_mut(),
        };
        for item in items {
            if let Some(edit) = item.text_edit.as_mut() {
                convert_completion_text_edit(state, document, edit, Direction::Outgoing);
            }
            if let Some(edits) = item.additional_text_edits.as_mut() {
                for edit in edits {
                    convert_text_edit(state, document, edit, Direction::Outgoing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CompletionItem, CompletionParams, CompletionResponse, PartialResultParams,
        TextDocumentIdentifier, TextDocumentPositionParams, TextEdit, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::testing::{line_position, same_line, state_with_documents};

    use crate::requests::{CompletionRequest, Request};

    #[test]
    fn completion_additional_text_edits_are_converted() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let mut response = Some(CompletionResponse::Array(vec![CompletionItem {
            label: "item".into(),
            additional_text_edits: Some(vec![TextEdit::new(same_line(0, 4, 4), "x".into())]),
            ..Default::default()
        }]));

        <CompletionRequest as Request>::modify_response(&state, &document, &mut response);

        let Some(CompletionResponse::Array(items)) = response else {
            panic!("expected completion array");
        };
        assert_eq!(
            items[0].additional_text_edits.as_ref().unwrap()[0].range,
            same_line(0, 2, 2),
        );
    }

    conversion_tests! {
        completion_incoming_utf16_becomes_utf8: CompletionRequest {
            params: |uri| CompletionParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            },
            incoming: |p| p.text_document_position.position,
            expects: line_position(0, 4),
        }
    }
}
