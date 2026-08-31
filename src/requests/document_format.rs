use async_lsp::lsp_types::{
    DocumentFormattingParams as LspDocumentFormattingParams, TextEdit as LspTextEdit,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_optional_vec, convert_text_edit},
};

pub struct DocumentFormat;

impl Request for DocumentFormat {
    type Params = LspDocumentFormattingParams;
    type Response = Option<Vec<LspTextEdit>>;

    request_extract_url!(text_document);

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
