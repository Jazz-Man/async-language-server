use async_lsp::lsp_types::{
    PrepareRenameResponse as LspPrepareRenameResponse,
    TextDocumentPositionParams as LspTextDocumentPositionParams,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_position, convert_range},
};

pub struct RenamePrepare;

impl Request for RenamePrepare {
    type Params = LspTextDocumentPositionParams;
    type Response = Option<LspPrepareRenameResponse>;

    request_extract_url!(text_document);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_position(state, document, &mut params.position, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            match response {
                LspPrepareRenameResponse::Range(range)
                | LspPrepareRenameResponse::RangeWithPlaceholder { range, .. } => {
                    convert_range(state, document, range, Direction::Outgoing);
                }
                LspPrepareRenameResponse::DefaultBehavior { .. } => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PrepareRenameResponse, TextDocumentIdentifier, TextDocumentPositionParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::RenamePrepare;

    conversion_tests! {
        rename_prepare_round_trips_both_directions: RenamePrepare {
            params: |uri| TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                line_position(0, 2),
            ),
            incoming: |p| p.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: same_line(0, 4, 4),
                placeholder: "x".into(),
            }),
            outgoing: |r| match r.as_ref() {
                Some(PrepareRenameResponse::RangeWithPlaceholder { range, .. }) => range.start,
                _ => panic!("expected range with placeholder"),
            },
            returns: line_position(0, 2),
        }
    }
}
