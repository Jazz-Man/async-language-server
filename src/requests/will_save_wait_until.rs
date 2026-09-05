#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::WillSaveTextDocumentParams,
    response = Option<Vec<async_lsp::lsp_types::TextEdit>>,
    document(text_document),
    outgoing(crate::requests::conversion::modify_outgoing_text_edits),
)]
pub(crate) struct WillSaveWaitUntilRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        TextDocumentIdentifier, TextDocumentSaveReason, TextEdit, WillSaveTextDocumentParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::WillSaveWaitUntilRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        will_save_wait_until_edits_convert_outgoing: WillSaveWaitUntilRequest {
            params: |uri| WillSaveTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri),
                reason: TextDocumentSaveReason::MANUAL,
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
