#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::request::GotoImplementationParams,
    response = Option<async_lsp::lsp_types::request::GotoImplementationResponse>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_goto_response),
)]
pub(crate) struct ImplementationRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, PartialResultParams,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::ImplementationRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        implementation_round_trips_both_directions: ImplementationRequest {
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
