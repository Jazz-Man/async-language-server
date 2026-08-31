use async_lsp::lsp_types::{DocumentLink as LspDocumentLink, Url};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct DocumentLinkResolve;

impl Request for DocumentLinkResolve {
    type Params = LspDocumentLink;
    type Response = LspDocumentLink;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        params.target.clone()
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_range(state, document, &mut response.range, Direction::Outgoing);
    }
}
