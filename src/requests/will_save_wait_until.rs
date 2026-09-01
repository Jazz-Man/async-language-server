#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        TextDocumentIdentifier, TextDocumentSaveReason, TextEdit, WillSaveTextDocumentParams,
    };

    use crate::requests::WillSaveWaitUntil;
    use crate::testing::{conversion_tests, line_position, same_line};

    conversion_tests! {
        will_save_wait_until_edits_convert_outgoing: WillSaveWaitUntil {
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
