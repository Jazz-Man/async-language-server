use async_lsp::lsp_types::{
    PrepareRenameResponse as LspPrepareRenameResponse,
    TextDocumentPositionParams as LspTextDocumentPositionParams, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_incoming_position, modify_outgoing_range},
};

pub struct RenamePrepare;

impl Request for RenamePrepare {
    type Params = LspTextDocumentPositionParams;
    type Response = Option<LspPrepareRenameResponse>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_position(state, document, &mut params.position);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            match response {
                LspPrepareRenameResponse::Range(range)
                | LspPrepareRenameResponse::RangeWithPlaceholder { range, .. } => {
                    modify_outgoing_range(state, document, range);
                }
                LspPrepareRenameResponse::DefaultBehavior { .. } => {}
            }
        }
    }
}
