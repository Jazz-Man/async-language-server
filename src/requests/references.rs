#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        Location, PartialResultParams, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::References;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        references_round_trips_both_directions: References {
            params: |uri| ReferenceParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
            incoming: |p| p.text_document_position.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(vec![Location::new(emoji, same_line(0, 4, 4))]),
            outgoing: |r| r.as_ref().expect("locations present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
