use async_lsp::lsp_types::request::{
    GotoDeclarationParams as LspGotoDeclarationParams,
    GotoDeclarationResponse as LspGotoDeclarationResponse,
};

use crate::server::{Document, ServerState};

use super::{Request, conversion::modify_outgoing_goto_response};

pub struct Declaration;

impl Request for Declaration {
    type Params = LspGotoDeclarationParams;
    type Response = Option<LspGotoDeclarationResponse>;

    request_extract_url!(text_document_position_params.text_document);
    request_modify_params_position!(text_document_position_params.position);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        modify_outgoing_goto_response(state, document, response);
    }
}
