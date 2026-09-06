#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::MonikerParams,
    response = Option<Vec<async_lsp::lsp_types::Moniker>>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
)]
pub(crate) struct MonikerRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        MonikerParams, PartialResultParams, TextDocumentIdentifier, TextDocumentPositionParams,
        WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::MonikerRequest;
    use crate::testing::line_position;

    conversion_tests! {
        moniker_position_converts_incoming: MonikerRequest {
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
