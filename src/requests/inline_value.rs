use async_lsp::lsp_types::InlineValueParams;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_range};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::InlineValueParams,
    response = Option<async_lsp::lsp_types::InlineValue>,
    document(text_document),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct InlineValueRequest;

/// Converts the value range and stopped location to UTF-8 (the incoming
/// hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut InlineValueParams) {
    convert_range(state, document, &mut params.range, Direction::Incoming);
    convert_range(
        state,
        document,
        &mut params.context.stopped_location,
        Direction::Incoming,
    );
}

/// Converts the inline value's range to the client encoding (the outgoing
/// hook).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<async_lsp::lsp_types::InlineValue>,
) {
    let Some(value) = response else { return };
    let range = match value {
        async_lsp::lsp_types::InlineValue::Text(v) => &mut v.range,
        async_lsp::lsp_types::InlineValue::VariableLookup(v) => &mut v.range,
        async_lsp::lsp_types::InlineValue::EvaluatableExpression(v) => &mut v.range,
    };
    convert_range(state, document, range, Direction::Outgoing);
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        InlineValue, InlineValueContext, InlineValueParams, InlineValueText,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::requests::{InlineValueRequest, Request};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn inline_value_ranges_convert_both_directions() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = InlineValueParams {
            text_document: TextDocumentIdentifier::new(emoji),
            range: same_line(0, 2, 3),
            context: InlineValueContext {
                frame_id: 0,
                stopped_location: same_line(0, 3, 4),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        <InlineValueRequest as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(params.range, same_line(0, 4, 5));
        assert_eq!(params.context.stopped_location, same_line(0, 5, 6));

        let mut response = Some(InlineValue::Text(InlineValueText {
            range: same_line(0, 4, 6),
            text: "x".into(),
        }));

        <InlineValueRequest as Request>::modify_response(&state, &document, &mut response);

        let InlineValue::Text(value) = response.expect("value present") else {
            panic!("text variant present");
        };
        assert_eq!(value.range, same_line(0, 2, 4));
    }
}
