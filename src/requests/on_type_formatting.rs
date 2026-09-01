#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentOnTypeFormattingParams, FormattingOptions, TextDocumentIdentifier,
        TextDocumentPositionParams, TextEdit,
    };

    use crate::requests::OnTypeFormatting;
    use crate::testing::{conversion_tests, line_position, same_line};

    conversion_tests! {
        on_type_formatting_round_trips_both_directions: OnTypeFormatting {
            params: |uri| DocumentOnTypeFormattingParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                ch: "{".into(),
                options: FormattingOptions::default(),
            },
            incoming: |p| p.text_document_position.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }]),
            outgoing: |r| r.as_ref().expect("edits present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
