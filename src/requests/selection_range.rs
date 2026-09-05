use async_lsp::lsp_types::SelectionRangeParams;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_position, convert_range};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SelectionRangeParams,
    response = Option<Vec<async_lsp::lsp_types::SelectionRange>>,
    document(text_document),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct SelectionRangeRequest;

/// Converts the selection positions to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut SelectionRangeParams) {
    for position in &mut params.positions {
        convert_position(state, document, position, Direction::Incoming);
    }
}

/// Converts each selection chain's ranges to the client encoding (the
/// outgoing hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<async_lsp::lsp_types::SelectionRange>>,
) {
    let Some(chains) = response else { return };
    for chain in chains {
        let mut current = Some(chain);
        while let Some(node) = current {
            convert_range(state, document, &mut node.range, Direction::Outgoing);
            current = node.parent.as_deref_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PartialResultParams, SelectionRange, SelectionRangeParams, TextDocumentIdentifier,
        WorkDoneProgressParams,
    };

    use crate::testing::{line_position, same_line, state_with_documents};

    use crate::requests::{Request, SelectionRangeRequest};

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

        <SelectionRangeRequest as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.positions[0], line_position(0, 4));
        assert_eq!(params.positions[1], line_position(0, 5));

        let mut response = Some(vec![SelectionRange {
            range: same_line(0, 4, 5),
            parent: Some(Box::new(SelectionRange {
                range: same_line(0, 4, 4),
                parent: None,
            })),
        }]);

        <SelectionRangeRequest as Request>::modify_response(&state, &document, &mut response);

        let chains = response.expect("chains present");
        assert_eq!(chains[0].range, same_line(0, 2, 3));
        let parent = chains[0].parent.as_deref().expect("parent present");
        assert_eq!(parent.range, same_line(0, 2, 2));
    }
}
