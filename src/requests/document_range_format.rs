use async_lsp::lsp_types::{
    DocumentRangeFormattingParams as LspDocumentRangeFormattingParams, TextEdit as LspTextEdit,
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

    request_extract_url!(text_document);

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

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentRangeFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
        WorkDoneProgressParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::DocumentRangeFormat;

    conversion_tests! {
        document_range_format_round_trips_both_directions: DocumentRangeFormat {
            params: |uri| DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier::new(uri),
                range: same_line(0, 2, 3),
                options: FormattingOptions::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.range.start,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }]),
            outgoing: |r| r.as_ref().expect("edits present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
