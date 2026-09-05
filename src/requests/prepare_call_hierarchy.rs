#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CallHierarchyPrepareParams,
    response = Option<Vec<async_lsp::lsp_types::CallHierarchyItem>>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_call_hierarchy_items),
)]
pub(crate) struct CallHierarchyPrepareRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyItem as LspCallHierarchyItem, CallHierarchyPrepareParams, SymbolKind,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::CallHierarchyPrepareRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        prepare_call_hierarchy_items_convert_both_directions: CallHierarchyPrepareRequest {
            params: |uri| CallHierarchyPrepareParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(vec![LspCallHierarchyItem {
                uri: emoji,
                range: same_line(0, 4, 4),
                selection_range: same_line(0, 4, 4),
                name: "f".into(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                detail: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("items present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
