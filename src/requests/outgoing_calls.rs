use async_lsp::lsp_types::CallHierarchyOutgoingCallsParams as LspCallHierarchyOutgoingCallsParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        Direction, convert_call_hierarchy_item, convert_call_hierarchy_outgoing_call,
        convert_optional_vec,
    },
};

pub struct OutgoingCalls;

impl Request for OutgoingCalls {
    type Params = LspCallHierarchyOutgoingCallsParams;
    type Response = Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>;

    request_extract_url!(item);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_call_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_optional_vec(
            state,
            document,
            response,
            Direction::Outgoing,
            convert_call_hierarchy_outgoing_call,
        );
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyItem as LspCallHierarchyItem, CallHierarchyOutgoingCall,
        CallHierarchyOutgoingCallsParams, PartialResultParams, SymbolKind, WorkDoneProgressParams,
    };

    use crate::requests::{OutgoingCalls, Request};
    use crate::testing::{same_line, state_with_documents};

    fn item(
        uri: async_lsp::lsp_types::Url,
        range_start: u32,
        selection_start: u32,
    ) -> LspCallHierarchyItem {
        LspCallHierarchyItem {
            uri,
            range: same_line(0, range_start, range_start),
            selection_range: same_line(0, selection_start, selection_start),
            name: "f".into(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            data: None,
        }
    }

    #[test]
    fn outgoing_calls_convert_from_ranges_against_the_request_document() {
        let (state, plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = CallHierarchyOutgoingCallsParams {
            item: item(emoji.clone(), 2, 3),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <OutgoingCalls as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.item.range, same_line(0, 4, 4));
        assert_eq!(params.item.selection_range, same_line(0, 5, 5));

        let mut response = Some(vec![CallHierarchyOutgoingCall {
            to: item(plain, 4, 5),
            from_ranges: vec![same_line(0, 4, 4), same_line(0, 5, 5)],
        }]);
        // from_ranges sit in the caller's — the request's — document: built
        // at UTF-8 byte 4/5 over the emoji snapshot, they must land on client
        // columns 2/3 through it. The `to` item is the callee, in its own
        // plain document: its ranges resolve per-URL, and plain columns are
        // already UTF-16-identical, so they stay put.
        <OutgoingCalls as Request>::modify_response(&state, &document, &mut response);

        let calls = response.expect("calls present");
        assert_eq!(calls[0].to.range, same_line(0, 4, 4));
        assert_eq!(calls[0].to.selection_range, same_line(0, 5, 5));
        assert_eq!(calls[0].from_ranges[0], same_line(0, 2, 2));
        assert_eq!(calls[0].from_ranges[1], same_line(0, 3, 3));
    }
}
