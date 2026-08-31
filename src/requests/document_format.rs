use async_lsp::lsp_types::{
    DocumentFormattingParams as LspDocumentFormattingParams, TextEdit as LspTextEdit, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_text_edit},
};

pub struct DocumentFormat;

impl Request for DocumentFormat {
    type Params = LspDocumentFormattingParams;
    type Response = Option<Vec<LspTextEdit>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(edits) = response.as_mut() {
            for edit in edits.iter_mut() {
                convert_text_edit(state, document, edit, Direction::Outgoing);
            }
        }
    }
}
