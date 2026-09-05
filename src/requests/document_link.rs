#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentLink as LspDocumentLink, DocumentLinkParams, PartialResultParams,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::DocumentLink;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        document_link_ranges_convert_outgoing: DocumentLink {
            params: |uri| DocumentLinkParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            response: |plain, _emoji| Some(vec![LspDocumentLink {
                range: same_line(0, 4, 4),
                target: Some(plain),
                tooltip: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("links present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
