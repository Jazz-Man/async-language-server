use async_lsp::lsp_types::{
    DocumentRangeFormattingParams as LspDocumentRangeFormattingParams, TextEdit as LspTextEdit, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_optional_vec, convert_range, convert_text_edit},
};

pub struct DocumentRangeFormat;

impl Request for DocumentRangeFormat {
    type Params = LspDocumentRangeFormattingParams;
    type Response = Option<Vec<LspTextEdit>>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_optional_vec(
            state,
            document,
            response,
            Direction::Outgoing,
            convert_text_edit,
        );
    }
}
