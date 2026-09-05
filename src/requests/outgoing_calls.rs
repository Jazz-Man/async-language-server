use async_lsp::lsp_types::CallHierarchyOutgoingCallsParams;

use crate::server::{Document, ServerState};

use super::conversion::{
    Direction, convert_call_hierarchy_item, convert_call_hierarchy_outgoing_call,
    convert_optional_vec,
};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CallHierarchyOutgoingCallsParams,
    response = Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>,
    document(item),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct OutgoingCallsRequest;

/// Converts the item's ranges to UTF-8 (the incoming hook).
fn convert_params(
    state: &ServerState,
    document: &Document,
    params: &mut CallHierarchyOutgoingCallsParams,
) {
    convert_call_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
}

/// Converts the returned calls' ranges back (the outgoing hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_call_hierarchy_outgoing_call,
    );
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams,
        PartialResultParams, SymbolKind, WorkDoneProgressParams,
    };

    use crate::requests::{OutgoingCallsRequest, Request};
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
    fn outgoing_calls_convert_from_ranges_against_the_request_document() {
        let (state, plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = CallHierarchyOutgoingCallsParams {
            item: item(emoji.clone(), 2, 3),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <OutgoingCallsRequest as Request>::modify_params(&state, &document, &mut params);

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
        <OutgoingCallsRequest as Request>::modify_response(&state, &document, &mut response);

        let calls = response.expect("calls present");
        assert_eq!(calls[0].to.range, same_line(0, 4, 4));
        assert_eq!(calls[0].to.selection_range, same_line(0, 5, 5));
        assert_eq!(calls[0].from_ranges[0], same_line(0, 2, 2));
        assert_eq!(calls[0].from_ranges[1], same_line(0, 3, 3));
    }
}
