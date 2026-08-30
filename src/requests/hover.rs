use async_lsp::lsp_types::{Hover as LspHover, HoverParams as LspHoverParams, Url};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_incoming_position, modify_outgoing_range},
};

pub struct Hover;

impl Request for Hover {
    type Params = LspHoverParams;
    type Response = Option<LspHover>;

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
        if let Some(hover) = response.as_mut()
            && let Some(range) = hover.range.as_mut()
        {
            modify_outgoing_range(state, document, range);
        }
    }
}
