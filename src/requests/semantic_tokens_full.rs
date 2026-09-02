#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{SemanticTokens, SemanticTokensResult};

    use crate::requests::{Request, SemanticTokensFull};
    use crate::testing::{state_with_documents, token};

    #[test]
    fn full_tokens_convert_columns_and_lengths_and_cache() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        // "🙂abc": UTF-8 bytes — token at byte 0 length 4 (the emoji),
        // token at byte 4 length 3 ("abc"). UTF-16: columns 0 and 2,
        // lengths 2 and 3.
        let mut response = Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some("r1".into()),
            data: vec![token(0, 0, 4), token(0, 4, 3)],
        }));

        <SemanticTokensFull as Request>::modify_response(&state, &document, &mut response);

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
