use async_lsp::lsp_types::CallHierarchyIncomingCallsParams as LspCallHierarchyIncomingCallsParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        Direction, convert_call_hierarchy_incoming_call, convert_call_hierarchy_item,
        convert_optional_vec,
    },
};

pub struct IncomingCalls;

impl Request for IncomingCalls {
    type Params = LspCallHierarchyIncomingCallsParams;
    type Response = Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>;

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
            convert_call_hierarchy_incoming_call,
        );
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams,
        CallHierarchyItem as LspCallHierarchyItem, PartialResultParams, SymbolKind,
        WorkDoneProgressParams,
    };

    use crate::requests::{IncomingCalls, Request};
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
    fn incoming_calls_convert_against_the_items_own_document() {
        let (state, plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = CallHierarchyIncomingCallsParams {
            item: item(emoji.clone(), 2, 3),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <IncomingCalls as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.item.range, same_line(0, 4, 4));
        assert_eq!(params.item.selection_range, same_line(0, 5, 5));

        let mut response = Some(vec![CallHierarchyIncomingCall {
            from: item(emoji, 4, 5),
            from_ranges: vec![same_line(0, 4, 4), same_line(0, 5, 5)],
        }]);
        // The request's own snapshot is the plain document: resolving the
        // response's ranges must follow the `from` item's URL, not this
        // fallback, to land on the client columns.
        let fallback = state.document(&plain).expect("plain document is tracked");

        <IncomingCalls as Request>::modify_response(&state, &fallback, &mut response);

        let calls = response.expect("calls present");
        assert_eq!(calls[0].from.range, same_line(0, 2, 2));
        assert_eq!(calls[0].from.selection_range, same_line(0, 3, 3));
        assert_eq!(calls[0].from_ranges[0], same_line(0, 2, 2));
        assert_eq!(calls[0].from_ranges[1], same_line(0, 3, 3));
    }
}
