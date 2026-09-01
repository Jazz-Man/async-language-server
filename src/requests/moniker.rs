#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        MonikerParams, PartialResultParams, TextDocumentIdentifier, TextDocumentPositionParams,
        WorkDoneProgressParams,
    };

    use crate::requests::Moniker;
    use crate::testing::{conversion_tests, line_position};

    conversion_tests! {
        moniker_position_converts_incoming: Moniker {
            params: |uri| MonikerParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
        }
    }
}
