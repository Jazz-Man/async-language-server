#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentHighlight as LspDocumentHighlight, DocumentHighlightParams, PartialResultParams,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::DocumentHighlight;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        document_highlight_round_trips_both_directions: DocumentHighlight {
            params: |uri| DocumentHighlightParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(vec![LspDocumentHighlight {
                range: same_line(0, 4, 4),
                kind: None,
            }]),
            outgoing: |r| r.as_ref().expect("highlights present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
