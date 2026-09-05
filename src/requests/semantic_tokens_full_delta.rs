#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SemanticTokensDeltaParams,
    response = Option<async_lsp::lsp_types::SemanticTokensFullDeltaResult>,
    document(text_document),
    outgoing(crate::requests::conversion::modify_outgoing_semantic_tokens_delta_result),
)]
pub(crate) struct SemanticTokensFullDeltaRequest;

#[cfg(test)]
mod tests {
    use async_lsp::{
        ClientSocket,
        lsp_types::{
            SemanticTokens, SemanticTokensDelta, SemanticTokensEdit, SemanticTokensFullDeltaResult,
            SemanticTokensResult, Url,
        },
    };

    use crate::requests::{Request, SemanticTokensFullDeltaRequest, SemanticTokensFullRequest};
    use crate::server::{ServerOptions, ServerState};
    use crate::testing::{TestServer, open_document, token, url};
    use crate::text_utils::Encoding;

    /// The two-line multibyte fixture: line 0 = "🙂abc" (the emoji spans
    /// bytes 0..4), line 1 = "x🙂z" ("z" sits at document byte 5, UTF-16
    /// unit 3 of its line — "x" is one unit, the emoji two).
    fn state_with_delta_document() -> (ServerState, Url) {
        let mut state = ServerState::with_options::<TestServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        state.set_position_encoding(Encoding::UTF16);
        let uri = url("semantic-tokens-delta.txt");
        open_document(&mut state, uri.clone(), "🙂abc\nx🙂z");
        (state, uri)
    }

    /// The pinned delta: replace the second cached token (flat 5..10) with
    /// one starting on line 1 at UTF-8 byte 5, length 1 — relative to the
    /// preceding token at (0, 0) that is `delta_line` 1, `delta_start` 5.
    fn delta_response() -> Option<SemanticTokensFullDeltaResult> {
        Some(SemanticTokensFullDeltaResult::TokensDelta(
            SemanticTokensDelta {
                result_id: Some("r2".into()),
                edits: vec![SemanticTokensEdit {
                    start: 5,
                    delete_count: 5,
                    data: Some(vec![token(1, 5, 1)]),
                }],
            },
        ))
    }

    /// Seeds the delta cache through the full hook with the UTF-8 stream of
    /// line 0 (the emoji at bytes 0..4, "abc" at 4..7), then converts the
    /// pinned delta through the delta hook.
    fn seeded_delta_response() -> (ServerState, Url, Option<SemanticTokensFullDeltaResult>) {
        let (state, uri) = state_with_delta_document();
        let document = state.document(&uri).expect("delta document is tracked");

        let mut seed = Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some("r1".into()),
            data: vec![token(0, 0, 4), token(0, 4, 3)],
        }));
        <SemanticTokensFullRequest as Request>::modify_response(&state, &document, &mut seed);

        let mut response = delta_response();
        <SemanticTokensFullDeltaRequest as Request>::modify_response(
            &state,
            &document,
            &mut response,
        );
        (state, uri, response)
    }

    #[test]
    fn delta_edits_convert_seeded_from_cache() {
        let (_state, _uri, response) = seeded_delta_response();
        let Some(SemanticTokensFullDeltaResult::TokensDelta(delta)) = response else {
            panic!("expected tokens delta");
        };
        let Some(inserted) = delta.edits[0].data.as_ref() else {
            panic!("edit carries data");
        };
        // A new line keeps its own delta_line and starts at its own column:
        // byte 5 of "x🙂z" is UTF-16 unit 3, and the length stays 1.
        assert_eq!(inserted[0].delta_line, 1);
        assert_eq!(inserted[0].delta_start, 3);
        assert_eq!(inserted[0].length, 1);
    }

    #[test]
    fn delta_cache_miss_passes_through() {
        let (state, uri) = state_with_delta_document();
        let document = state.document(&uri).expect("delta document is tracked");
        let mut response = delta_response();
        let expected = response.clone();

        <SemanticTokensFullDeltaRequest as Request>::modify_response(
            &state,
            &document,
            &mut response,
        );

        assert_eq!(response, expected);
    }

    #[test]
    fn delta_splice_keeps_original_utf8_values() {
        let (state, uri, _response) = seeded_delta_response();

        // The spliced cache holds the ORIGINAL UTF-8 inserted values —
        // delta_start 5, not the converted 3 — under the delta's result id.
        let cached = state
            .cached_semantic_tokens(&uri)
            .expect("delta response cached a result");
        assert_eq!(cached.result_id, "r2");
        assert_eq!(cached.data, vec![token(0, 0, 4), token(1, 5, 1)]);
    }
}
