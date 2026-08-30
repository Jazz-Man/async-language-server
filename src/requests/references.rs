use async_lsp::lsp_types::{Location as LspLocation, ReferenceParams as LspReferenceParams, Url};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_incoming_position, modify_outgoing_location},
};

pub struct References;

impl Request for References {
    type Params = LspReferenceParams;
    type Response = Option<Vec<LspLocation>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document_position.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_position(state, document, &mut params.text_document_position.position);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(locations) = response.as_mut() {
            for loc in locations.iter_mut() {
                modify_outgoing_location(state, document, loc);
            }
        }
    }
}
