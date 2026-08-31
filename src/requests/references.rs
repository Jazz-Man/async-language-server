use async_lsp::lsp_types::{Location as LspLocation, ReferenceParams as LspReferenceParams};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_location, convert_optional_vec},
};

pub struct References;

impl Request for References {
    type Params = LspReferenceParams;
    type Response = Option<Vec<LspLocation>>;

    request_extract_url!(text_document_position.text_document);
    request_modify_params_position!(text_document_position.position);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_optional_vec(
            state,
            document,
            response,
            Direction::Outgoing,
            convert_location,
        );
    }
}
