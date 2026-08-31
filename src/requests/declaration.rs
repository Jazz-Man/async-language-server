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
    conversion::{Direction, convert_location, convert_position, modify_outgoing_location_link},
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
        convert_position(
            state,
            document,
            &mut params.text_document_position_params.position,
            Direction::Incoming,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            match response {
                LspGotoDeclarationResponse::Scalar(loc) => {
                    convert_location(state, document, loc, Direction::Outgoing);
                }
                LspGotoDeclarationResponse::Array(locations) => {
                    for loc in locations.iter_mut() {
                        convert_location(state, document, loc, Direction::Outgoing);
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
