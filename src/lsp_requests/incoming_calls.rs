use async_lsp::lsp_types::CallHierarchyIncomingCallsParams;

use crate::server::{Document, ServerState};

use super::conversion::{
    Direction, convert_call_hierarchy_incoming_call, convert_call_hierarchy_item,
    convert_optional_vec,
};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CallHierarchyIncomingCallsParams,
    response = Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
    document(item),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct IncomingCallsRequest;

/// Converts the item's ranges to UTF-8 (the incoming hook).
fn convert_params(
    state: &ServerState,
    document: &Document,
    params: &mut CallHierarchyIncomingCallsParams,
) {
    convert_call_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
}

/// Converts the returned calls' ranges back (the outgoing hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_call_hierarchy_incoming_call,
    );
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
        PartialResultParams, SymbolKind, WorkDoneProgressParams,
    };

    use crate::requests::{IncomingCallsRequest, Request};
    use crate::testing::{same_line, state_with_documents};

    fn item(
        uri: async_lsp::lsp_types::Url,
        range_start: u32,
        selection_start: u32,
    ) -> CallHierarchyItem {
        CallHierarchyItem {
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

        <IncomingCallsRequest as Request>::modify_params(&state, &document, &mut params);

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

        <IncomingCallsRequest as Request>::modify_response(&state, &fallback, &mut response);

        let calls = response.expect("calls present");
        assert_eq!(calls[0].from.range, same_line(0, 2, 2));
        assert_eq!(calls[0].from.selection_range, same_line(0, 3, 3));
        assert_eq!(calls[0].from_ranges[0], same_line(0, 2, 2));
        assert_eq!(calls[0].from_ranges[1], same_line(0, 3, 3));
    }
}
