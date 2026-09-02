#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        InlayHint, InlayHintLabel, InlayHintLabelPart, InlayHintParams, Location,
        TextDocumentIdentifier, TextEdit, WorkDoneProgressParams,
    };

    use crate::requests::{InlayHint as InlayHintRequest, Request};
    use crate::testing::{conversion_tests, line_position, same_line, state_with_documents};

    conversion_tests! {
        inlay_hint_round_trips_both_directions: InlayHintRequest {
            params: |uri| InlayHintParams {
                text_document: TextDocumentIdentifier::new(uri),
                range: same_line(0, 2, 3),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.range.start,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(vec![InlayHint {
                position: line_position(0, 4),
                label: InlayHintLabel::String("x".into()),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: None,
                padding_right: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("hints present")[0].position,
            returns: line_position(0, 2),
        }
    }

    #[test]
    fn inlay_hint_edits_and_label_part_locations_convert_outgoing() {
        let (state, plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(vec![InlayHint {
            position: line_position(0, 4),
            label: InlayHintLabel::LabelParts(vec![InlayHintLabelPart {
                value: "x".into(),
                tooltip: None,
                location: Some(Location::new(plain, same_line(0, 4, 4))),
                command: None,
            }]),
            kind: None,
            text_edits: Some(vec![TextEdit {
                range: same_line(0, 4, 5),
                new_text: "x".into(),
            }]),
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        }]);

        <InlayHintRequest as Request>::modify_response(&state, &document, &mut response);

        let hint = response.expect("hints present").remove(0);
        let edits = hint.text_edits.expect("edits present");
        assert_eq!(edits[0].range, same_line(0, 2, 3));
        let InlayHintLabel::LabelParts(parts) = hint.label else {
            panic!("label parts present");
        };
        let location = parts[0].location.as_ref().expect("location present");
        // Keyed at the plain document: the conversion follows the location's
        // own URL, not the request's emoji snapshot.
        assert_eq!(location.range, same_line(0, 4, 4));
    }
}
