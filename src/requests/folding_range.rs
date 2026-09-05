#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::FoldingRangeParams,
    response = Option<Vec<async_lsp::lsp_types::FoldingRange>>,
    document(text_document),
    outgoing(crate::requests::conversion::modify_outgoing_folding_ranges),
)]
pub(crate) struct FoldingRangeRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::FoldingRange;

    use crate::requests::{FoldingRangeRequest, Request};
    use crate::testing::state_with_documents;

    #[test]
    fn folding_range_characters_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(vec![FoldingRange {
            start_line: 0,
            start_character: Some(4),
            end_line: 0,
            end_character: Some(5),
            kind: None,
            collapsed_text: None,
        }]);

        <FoldingRangeRequest as Request>::modify_response(&state, &document, &mut response);

        let range = response.expect("ranges present")[0].clone();
        assert_eq!(range.start_character, Some(2));
        assert_eq!(range.end_character, Some(3));
    }
}
