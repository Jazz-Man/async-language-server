use async_lsp::lsp_types::SelectionRangeParams as LspSelectionRangeParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_position, convert_range},
};

pub struct SelectionRange;

impl Request for SelectionRange {
    type Params = LspSelectionRangeParams;
    type Response = Option<Vec<async_lsp::lsp_types::SelectionRange>>;

    request_extract_url!(text_document);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        for position in &mut params.positions {
            convert_position(state, document, position, Direction::Incoming);
        }
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        let Some(chains) = response else { return };
        for chain in chains {
            let mut current = Some(chain);
            while let Some(node) = current {
                convert_range(state, document, &mut node.range, Direction::Outgoing);
                current = node.parent.as_deref_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PartialResultParams, SelectionRange as LspSelectionRange, SelectionRangeParams,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::testing::{line_position, same_line, state_with_documents};

    use super::{Request, SelectionRange};

    #[test]
    fn selection_range_positions_and_chains_convert() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = SelectionRangeParams {
            text_document: TextDocumentIdentifier::new(emoji),
            positions: vec![line_position(0, 2), line_position(0, 3)],
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        <SelectionRange as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.positions[0], line_position(0, 4));
        assert_eq!(params.positions[1], line_position(0, 5));

        let mut response = Some(vec![LspSelectionRange {
            range: same_line(0, 4, 5),
            parent: Some(Box::new(LspSelectionRange {
                range: same_line(0, 4, 4),
                parent: None,
            })),
        }]);

        <SelectionRange as Request>::modify_response(&state, &document, &mut response);

        let chains = response.expect("chains present");
        assert_eq!(chains[0].range, same_line(0, 2, 3));
        let parent = chains[0].parent.as_deref().expect("parent present");
        assert_eq!(parent.range, same_line(0, 2, 2));
    }
}
