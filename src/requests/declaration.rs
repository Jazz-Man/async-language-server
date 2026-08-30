use async_lsp::lsp_types::{
    Url,
    request::{
        GotoDeclarationParams as LspGotoDeclarationParams,
        GotoDeclarationResponse as LspGotoDeclarationResponse,
    },
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        modify_incoming_position, modify_outgoing_location, modify_outgoing_location_link,
    },
};

pub struct Declaration;

impl Request for Declaration {
    type Params = LspGotoDeclarationParams;
    type Response = Option<LspGotoDeclarationResponse>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(
            params
                .text_document_position_params
                .text_document
                .uri
                .clone(),
        )
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_position(
            state,
            document,
            &mut params.text_document_position_params.position,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            match response {
                LspGotoDeclarationResponse::Scalar(loc) => {
                    modify_outgoing_location(state, document, loc);
                }
                LspGotoDeclarationResponse::Array(locations) => {
                    for loc in locations.iter_mut() {
                        modify_outgoing_location(state, document, loc);
                    }
                }
                LspGotoDeclarationResponse::Link(links) => {
                    for link in links.iter_mut() {
                        modify_outgoing_location_link(state, document, link);
                    }
                }
            }
        }
    }
}
