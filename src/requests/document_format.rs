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

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
        WorkDoneProgressParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::DocumentFormat;

    conversion_tests! {
        document_format_edits_convert_outgoing: DocumentFormat {
            params: |uri| DocumentFormattingParams {
                text_document: TextDocumentIdentifier::new(uri),
                options: FormattingOptions::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            response: |_plain, _emoji| Some(vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }]),
            outgoing: |r| r.as_ref().expect("edits present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
