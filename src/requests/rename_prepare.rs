#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PrepareRenameResponse, TextDocumentIdentifier, TextDocumentPositionParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::RenamePrepare;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        rename_prepare_round_trips_both_directions: RenamePrepare {
            params: |uri| TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(uri),
                line_position(0, 2),
            ),
            incoming: |p| p.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: same_line(0, 4, 4),
                placeholder: "x".into(),
            }),
            outgoing: |r| match r.as_ref() {
                Some(PrepareRenameResponse::RangeWithPlaceholder { range, .. }) => range.start,
                _ => panic!("expected range with placeholder"),
            },
            returns: line_position(0, 2),
        }
    }
}
