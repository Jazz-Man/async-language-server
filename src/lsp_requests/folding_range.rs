#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::FoldingRangeParams,
    response = Option<Vec<async_lsp::lsp_types::FoldingRange>>,
    document(text_document),
    outgoing(crate::lsp_requests::conversion::modify_outgoing_folding_ranges),
)]
pub(crate) struct FoldingRangeRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{FoldingRange, Url};
    use rstest::rstest;

    use crate::lsp_requests::{FoldingRangeRequest, Request};
    use crate::server::ServerState;
    use crate::testing::utf16_state;

    #[rstest]
    fn folding_range_characters_convert_outgoing(utf16_state: (ServerState, Url, Url)) {
        let (state, _plain, emoji) = utf16_state;
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
