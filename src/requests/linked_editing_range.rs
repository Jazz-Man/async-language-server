#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        LinkedEditingRangeParams, LinkedEditingRanges, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::LinkedEditingRange;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        linked_editing_range_incoming_and_outgoing_convert: LinkedEditingRange {
            params: |uri| LinkedEditingRangeParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(LinkedEditingRanges {
                ranges: vec![same_line(0, 4, 4)],
                word_pattern: None,
            }),
            outgoing: |r| r.as_ref().expect("ranges present").ranges[0].start,
            returns: line_position(0, 2),
        }
    }
}
