#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        HoverContents, MarkupContent, MarkupKind, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::Hover;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        hover_incoming_utf16_becomes_utf8: Hover {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
        }
        hover_outgoing_utf8_becomes_utf16: Hover {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(async_lsp::lsp_types::Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: "x".into(),
                }),
                range: Some(same_line(0, 4, 4)),
            }),
            outgoing: |r| r.as_ref().expect("hover present").range.expect("range present").start,
            returns: line_position(0, 2),
        }
    }
}
