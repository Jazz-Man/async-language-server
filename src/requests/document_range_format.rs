use async_lsp::lsp_types::{
    DocumentRangeFormattingParams as LspDocumentRangeFormattingParams, TextEdit as LspTextEdit, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{modify_incoming_range, modify_outgoing_text_edit},
};

pub struct DocumentRangeFormat;

impl Request for DocumentRangeFormat {
    type Params = LspDocumentRangeFormattingParams;
    type Response = Option<Vec<LspTextEdit>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        modify_incoming_range(state, document, &mut params.range);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(edits) = response.as_mut() {
            for edit in edits.iter_mut() {
                modify_outgoing_text_edit(state, document, edit);
            }
        }
    }
}
