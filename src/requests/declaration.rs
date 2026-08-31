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
    conversion::{Direction, convert_position, modify_outgoing_goto_response},
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
        modify_outgoing_goto_response(state, document, response);
    }
}
