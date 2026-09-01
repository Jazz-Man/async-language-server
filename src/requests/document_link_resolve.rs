use async_lsp::lsp_types::DocumentLink as LspDocumentLink;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct DocumentLinkResolve;

impl Request for DocumentLinkResolve {
    type Params = LspDocumentLink;
    type Response = LspDocumentLink;

    // DocumentLink doesn't contain a source document URI; the resolve
    // dispatch macro supplies the sole tracked document.

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_range(state, document, &mut response.range, Direction::Outgoing);
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{DocumentLink, Range};

    use crate::requests::{Direction, DocumentLinkResolve, convert_resolve_item};
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
        convert_resolve_item::<DocumentLinkResolve, _>(
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

        convert_resolve_item::<DocumentLinkResolve, _>(
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
        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Incoming,
        );
        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 2, 2));
    }
}
