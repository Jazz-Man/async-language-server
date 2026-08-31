use async_lsp::lsp_types::{
    PrepareRenameResponse as LspPrepareRenameResponse,
    TextDocumentPositionParams as LspTextDocumentPositionParams, Url,
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

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

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
