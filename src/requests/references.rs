use async_lsp::lsp_types::{Location as LspLocation, ReferenceParams as LspReferenceParams};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_location, convert_optional_vec},
};

pub struct References;

impl Request for References {
    type Params = LspReferenceParams;
    type Response = Option<Vec<LspLocation>>;

    request_extract_url!(text_document_position.text_document);
    request_modify_params_position!(text_document_position.position);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_optional_vec(
            state,
            document,
            response,
            Direction::Outgoing,
            convert_location,
        );
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        Location, PartialResultParams, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::References;

    conversion_tests! {
        references_round_trips_both_directions: References {
            params: |uri| ReferenceParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
            incoming: |p| p.text_document_position.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(vec![Location::new(emoji, same_line(0, 4, 4))]),
            outgoing: |r| r.as_ref().expect("locations present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
