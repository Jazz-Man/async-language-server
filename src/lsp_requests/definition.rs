#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::GotoDefinitionParams,
    response = Option<async_lsp::lsp_types::GotoDefinitionResponse>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::lsp_requests::conversion::modify_outgoing_goto_response),
)]
pub(crate) struct DefinitionRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, PartialResultParams,
        TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;
    use rstest::rstest;

    use crate::lsp_requests::{DefinitionRequest, Request};
    use crate::server::ServerState;
    use crate::testing::{line_position, same_line, utf16_state};

    #[rstest]
    fn definition_locations_are_converted_using_their_own_document(
        utf16_state: (ServerState, Url, Url),
    ) {
        let (state, source, target) = utf16_state;
        let document = state.document(&source).unwrap();
        let mut response = Some(GotoDefinitionResponse::Scalar(Location::new(
            target,
            same_line(0, 4, 4),
        )));

        <DefinitionRequest as Request>::modify_response(&state, &document, &mut response);

        let Some(GotoDefinitionResponse::Scalar(loc)) = response else {
            panic!("expected scalar location");
        };
        assert_eq!(loc.range, same_line(0, 2, 2));
    }

    conversion_tests! {
        definition_round_trips_both_directions: DefinitionRequest {
            params: |uri| GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(GotoDefinitionResponse::Scalar(Location::new(
                emoji,
                same_line(0, 4, 4),
            ))),
            outgoing: |r| match r.as_ref() {
                Some(GotoDefinitionResponse::Scalar(loc)) => loc.range.start,
                _ => panic!("expected scalar location"),
            },
            returns: line_position(0, 2),
        }
    }
}
