#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::TypeHierarchyPrepareParams,
    response = Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_type_hierarchy_items),
)]
pub(crate) struct TypeHierarchyPrepareRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        SymbolKind, TextDocumentIdentifier, TextDocumentPositionParams,
        TypeHierarchyItem as LspTypeHierarchyItem, TypeHierarchyPrepareParams,
        WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::TypeHierarchyPrepareRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        prepare_type_hierarchy_items_convert_both_directions: TypeHierarchyPrepareRequest {
            params: |uri| TypeHierarchyPrepareParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(vec![LspTypeHierarchyItem {
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
