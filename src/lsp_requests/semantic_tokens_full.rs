#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SemanticTokensParams,
    response = Option<async_lsp::lsp_types::SemanticTokensResult>,
    document(text_document),
    outgoing(crate::lsp_requests::conversion::modify_outgoing_semantic_tokens_result),
)]
pub(crate) struct SemanticTokensFullRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{SemanticTokens, SemanticTokensResult, Url};
    use rstest::rstest;

    use crate::lsp_requests::{Request, SemanticTokensFullRequest};
    use crate::server::ServerState;
    use crate::testing::{token, utf16_state};

    #[rstest]
    fn full_tokens_convert_columns_and_lengths_and_cache(utf16_state: (ServerState, Url, Url)) {
        let (state, _plain, emoji) = utf16_state;
        let document = state.document(&emoji).expect("emoji document is tracked");
        // "🙂abc": UTF-8 bytes — token at byte 0 length 4 (the emoji),
        // token at byte 4 length 3 ("abc"). UTF-16: columns 0 and 2,
        // lengths 2 and 3.
        let mut response = Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some("r1".into()),
            data: vec![token(0, 0, 4), token(0, 4, 3)],
        }));

        <SemanticTokensFullRequest as Request>::modify_response(&state, &document, &mut response);

        let Some(SemanticTokensResult::Tokens(tokens)) = response else {
            panic!("expected tokens");
        };
        assert_eq!(tokens.data[0].delta_start, 0);
        assert_eq!(tokens.data[0].length, 2);
        assert_eq!(tokens.data[1].delta_start, 2);
        assert_eq!(tokens.data[1].length, 3);

        // The cache keeps the UTF-8 stream the response was converted
        // FROM, under the response's result id.
        let cached = state
            .cached_semantic_tokens(&emoji)
            .expect("full response cached a result");
        assert_eq!(cached.result_id, "r1");
        assert_eq!(cached.data, vec![token(0, 0, 4), token(0, 4, 3)]);
    }
}
