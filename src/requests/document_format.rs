#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
        WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::DocumentFormat;
    use crate::testing::{line_position, same_line};

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
