#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::DocumentFormattingParams,
    response = Option<Vec<async_lsp::lsp_types::TextEdit>>,
    document(text_document),
    outgoing(crate::requests::conversion::modify_outgoing_text_edits),
)]
pub(crate) struct DocumentFormatRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
        WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::DocumentFormatRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        document_format_edits_convert_outgoing: DocumentFormatRequest {
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
