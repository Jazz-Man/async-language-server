#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SemanticTokensRangeParams,
    response = Option<async_lsp::lsp_types::SemanticTokensRangeResult>,
    document(text_document),
    incoming_range(range),
    outgoing(crate::requests::conversion::modify_outgoing_semantic_tokens_range_result),
)]
pub(crate) struct SemanticTokensRangeRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PartialResultParams, SemanticTokens, SemanticTokensPartialResult,
        SemanticTokensRangeParams, SemanticTokensRangeResult, TextDocumentIdentifier,
        WorkDoneProgressParams,
    };

    use crate::requests::{Request, SemanticTokensRangeRequest};
    use crate::testing::{same_line, state_with_documents, token};

    #[test]
    fn range_converts_incoming_range_and_outgoing_columns() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");

        let mut params = SemanticTokensRangeParams {
            range: same_line(0, 2, 2),
            text_document: TextDocumentIdentifier { uri: emoji.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        <SemanticTokensRangeRequest as Request>::modify_params(&state, &document, &mut params);
        // UTF-16 column 2 is UTF-8 byte 4 on "🙂abc".
        assert_eq!(params.range, same_line(0, 4, 4));

        // "🙂abc": UTF-8 bytes — token at byte 0 length 4 (the emoji),
        // token at byte 4 length 3 ("abc"). UTF-16: columns 0 and 2,
        // lengths 2 and 3.
        let mut response = Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: Some("r1".into()),
            data: vec![token(0, 0, 4), token(0, 4, 3)],
        }));
        <SemanticTokensRangeRequest as Request>::modify_response(&state, &document, &mut response);
        let Some(SemanticTokensRangeResult::Tokens(tokens)) = response else {
            panic!("expected tokens");
        };
        assert_eq!(tokens.data[0].delta_start, 0);
        assert_eq!(tokens.data[0].length, 2);
        assert_eq!(tokens.data[1].delta_start, 2);
        assert_eq!(tokens.data[1].length, 3);
        // Range responses never seed the delta cache.
        assert!(state.cached_semantic_tokens(&emoji).is_none());

        let mut response = Some(SemanticTokensRangeResult::Partial(
            SemanticTokensPartialResult {
                data: vec![token(0, 0, 4), token(0, 4, 3)],
            },
        ));
        <SemanticTokensRangeRequest as Request>::modify_response(&state, &document, &mut response);
        let Some(SemanticTokensRangeResult::Partial(partial)) = response else {
            panic!("expected partial result");
        };
        assert_eq!(partial.data[0].delta_start, 0);
        assert_eq!(partial.data[0].length, 2);
        assert_eq!(partial.data[1].delta_start, 2);
        assert_eq!(partial.data[1].length, 3);
    }
}
