use async_lsp::lsp_types::{Hover as LspHover, HoverParams as LspHoverParams};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct Hover;

impl Request for Hover {
    type Params = LspHoverParams;
    type Response = Option<LspHover>;

    request_extract_url!(text_document_position_params.text_document);
    request_modify_params_position!(text_document_position_params.position);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(hover) = response.as_mut()
            && let Some(range) = hover.range.as_mut()
        {
            convert_range(state, document, range, Direction::Outgoing);
        }
    }
}
