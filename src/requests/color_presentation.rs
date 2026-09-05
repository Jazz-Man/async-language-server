#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::ColorPresentationParams,
    response = Vec<async_lsp::lsp_types::ColorPresentation>,
    document(text_document),
    incoming_range(range),
    outgoing(crate::requests::conversion::modify_outgoing_color_presentations),
)]
pub(crate) struct ColorPresentationRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        Color, ColorPresentation, ColorPresentationParams, PartialResultParams,
        TextDocumentIdentifier, TextEdit, WorkDoneProgressParams,
    };

    use crate::requests::{ColorPresentationRequest, Request};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn color_presentation_round_trips_both_directions() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");

        let mut params = ColorPresentationParams {
            text_document: TextDocumentIdentifier::new(emoji.clone()),
            color: Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            range: same_line(0, 2, 3),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        <ColorPresentationRequest as Request>::modify_params(&state, &document, &mut params);
        assert_eq!(params.range, same_line(0, 4, 5));

        let mut response = vec![ColorPresentation {
            label: "rgb(255, 0, 0)".into(),
            text_edit: Some(TextEdit {
                range: same_line(0, 4, 4),
                new_text: "#ff0000".into(),
            }),
            additional_text_edits: None,
        }];
        <ColorPresentationRequest as Request>::modify_response(&state, &document, &mut response);
        let edit = response[0]
            .text_edit
            .as_ref()
            .expect("text edit is present");
        assert_eq!(edit.range, same_line(0, 2, 2));
    }
}
