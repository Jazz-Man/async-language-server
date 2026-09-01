#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CodeLens, CodeLensParams, PartialResultParams, TextDocumentIdentifier,
        WorkDoneProgressParams,
    };

    use crate::requests::CodeLens as CodeLensRequest;
    use crate::testing::{conversion_tests, line_position, same_line};

    conversion_tests! {
        code_lens_outgoing_utf8_becomes_utf16: CodeLensRequest {
            params: |uri| CodeLensParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            response: |_plain, _emoji| Some(vec![CodeLens {
                range: same_line(0, 4, 4),
                command: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("lenses present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
