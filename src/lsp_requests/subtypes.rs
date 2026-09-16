use async_lsp::lsp_types::TypeHierarchySubtypesParams;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_type_hierarchy_item};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::TypeHierarchySubtypesParams,
    response = Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
    document(item),
    incoming_custom(self::convert_params),
    outgoing(crate::lsp_requests::conversion::modify_outgoing_type_hierarchy_items),
)]
pub(crate) struct SubtypesRequest;

/// Converts the item's ranges to UTF-8 (the incoming hook).
fn convert_params(
    state: &ServerState,
    document: &Document,
    params: &mut TypeHierarchySubtypesParams,
) {
    convert_type_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PartialResultParams, SymbolKind, TypeHierarchyItem, TypeHierarchySubtypesParams,
        WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::lsp_requests::SubtypesRequest;
    use crate::testing::{line_position, same_line};

    fn item(
        uri: async_lsp::lsp_types::Url,
        range_start: u32,
        selection_start: u32,
    ) -> TypeHierarchyItem {
        TypeHierarchyItem {
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

    conversion_tests! {
        subtypes_item_range_converts_both_directions: SubtypesRequest {
            params: |uri| TypeHierarchySubtypesParams {
                item: item(uri, 2, 3),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.item.range.start,
            expects: line_position(0, 4),
            // The response anchor is the emoji document while the item sits
            // in another tracked file: the item's ranges must follow the
            // item's URL, not this fallback — byte 4 on the plain file is
            // client column 4, not the emoji document's 2.
            response: |plain, _emoji| Some(vec![item(plain, 4, 5)]),
            outgoing: |r| r.as_ref().expect("items present")[0].range.start,
            returns: line_position(0, 4),
        }
        subtypes_item_selection_range_converts_both_directions: SubtypesRequest {
            params: |uri| TypeHierarchySubtypesParams {
                item: item(uri, 2, 3),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.item.selection_range.start,
            expects: line_position(0, 5),
            response: |plain, _emoji| Some(vec![item(plain, 4, 5)]),
            outgoing: |r| r.as_ref().expect("items present")[0].selection_range.start,
            returns: line_position(0, 5),
        }
    }
}
