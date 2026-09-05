use async_lsp::lsp_types::DocumentLink;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_range};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::DocumentLink,
    response = async_lsp::lsp_types::DocumentLink,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct DocumentLinkResolveRequest;

// DocumentLink doesn't contain a source document URI; the resolve
// dispatch macro supplies the sole tracked document.

/// Converts the link's range to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut DocumentLink) {
    convert_range(state, document, &mut params.range, Direction::Incoming);
}

/// Converts the link's range back to the client encoding (the outgoing hook).
fn convert_response(state: &ServerState, document: &Document, response: &mut DocumentLink) {
    convert_range(state, document, &mut response.range, Direction::Outgoing);
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{DocumentLink, Range};

    use crate::requests::{Direction, DocumentLinkResolveRequest, convert_resolve_item};
    use crate::server::{ServerOptions, ServerState};
    use crate::testing::{TestServer, open_document, same_line, state_with_documents, url};
    use crate::text_utils::Encoding;

    fn link(range: Range) -> DocumentLink {
        DocumentLink {
            range,
            target: None,
            tooltip: None,
            data: None,
        }
    }

    #[test]
    fn resolve_range_converts_against_the_sole_tracked_document() {
        // Exactly one tracked document ("🙂abc"), UTF-16 negotiated.
        let mut state = ServerState::with_options::<TestServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        state.set_position_encoding(Encoding::UTF16);
        open_document(&mut state, url("only.txt"), "🙂abc");

        let mut item = link(same_line(0, 4, 4));

        let document = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<DocumentLinkResolveRequest, _>(
            &state,
            Some(&document),
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 2, 2));
    }

    #[test]
    fn resolve_range_passes_through_without_a_document() {
        // No document snapshot: the range passes through unchanged.
        let (state, _, _) = state_with_documents();

        let mut item = link(same_line(0, 4, 4));

        convert_resolve_item::<DocumentLinkResolveRequest, _>(
            &state,
            None,
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 4, 4));
    }

    #[test]
    fn resolve_echo_round_trip_is_identity() {
        // Sole doc "🙂abc", UTF-16 negotiated. The client echoes the link at
        // the UTF-16 position it was delivered: the incoming converter must
        // turn it into UTF-8 for the handler, and the outgoing converter must
        // return the original position — no double conversion.
        let mut state = ServerState::with_options::<TestServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        state.set_position_encoding(Encoding::UTF16);
        open_document(&mut state, url("only.txt"), "🙂abc");

        let mut item = link(same_line(0, 2, 2));

        let sole = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<DocumentLinkResolveRequest, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Incoming,
        );
        convert_resolve_item::<DocumentLinkResolveRequest, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 2, 2));
    }
}
