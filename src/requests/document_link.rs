use async_lsp::lsp_types::{
    DocumentLink as LspDocumentLink, DocumentLinkParams as LspDocumentLinkParams, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct DocumentLink;

impl Request for DocumentLink {
    type Params = LspDocumentLinkParams;
    type Response = Option<Vec<LspDocumentLink>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(links) = response.as_mut() {
            for link in links.iter_mut() {
                convert_range(state, document, &mut link.range, Direction::Outgoing);
            }
        }
    }
}
