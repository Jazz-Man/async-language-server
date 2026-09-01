#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentRangeFormattingParams, FormattingOptions, TextDocumentIdentifier, TextEdit,
        WorkDoneProgressParams,
    };

    use crate::requests::DocumentRangeFormat;
    use crate::testing::{conversion_tests, line_position, same_line};

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
