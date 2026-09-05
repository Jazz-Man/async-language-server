use async_lsp::lsp_types::TypeHierarchySubtypesParams as LspTypeHierarchySubtypesParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_type_hierarchy_item, modify_outgoing_type_hierarchy_items},
};

pub(crate) struct Subtypes;

impl Request for Subtypes {
    type Params = LspTypeHierarchySubtypesParams;
    type Response = Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>;

    request_extract_url!(item);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_type_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        modify_outgoing_type_hierarchy_items(state, document, response);
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PartialResultParams, SymbolKind, TypeHierarchyItem as LspTypeHierarchyItem,
        TypeHierarchySubtypesParams, WorkDoneProgressParams,
    };

    use crate::requests::{Request, Subtypes};
    use crate::testing::{same_line, state_with_documents};

    fn item(
        uri: async_lsp::lsp_types::Url,
        range_start: u32,
        selection_start: u32,
    ) -> LspTypeHierarchyItem {
        LspTypeHierarchyItem {
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
    fn subtypes_convert_against_the_items_own_document() {
        let (state, plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = TypeHierarchySubtypesParams {
            item: item(emoji.clone(), 2, 3),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <Subtypes as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.item.range, same_line(0, 4, 4));
        assert_eq!(params.item.selection_range, same_line(0, 5, 5));

        let mut response = Some(vec![item(emoji, 4, 5)]);
        // The request's own snapshot is the plain document: resolving the
        // response items' ranges must follow each item's URL, not this
        // fallback, to land on the client columns.
        let fallback = state.document(&plain).expect("plain document is tracked");

        <Subtypes as Request>::modify_response(&state, &fallback, &mut response);

        let items = response.expect("items present");
        assert_eq!(items[0].range, same_line(0, 2, 2));
        assert_eq!(items[0].selection_range, same_line(0, 3, 3));
    }
}
