use async_lsp::lsp_types::{
    RenameParams as LspRenameParams, Url, WorkspaceEdit as LspWorkspaceEdit,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_incoming_position, modify_outgoing_workspace_edit},
};

pub struct Rename;

impl Request for Rename {
    type Params = LspRenameParams;
    type Response = Option<LspWorkspaceEdit>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document_position.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_position(state, document, &mut params.text_document_position.position);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            modify_outgoing_workspace_edit(state, document, response);
        }
    }
}
