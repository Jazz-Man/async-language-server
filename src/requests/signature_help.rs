#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        ParameterInformation, ParameterLabel, SignatureHelp as LspSignatureHelp,
        SignatureHelpParams, SignatureInformation, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };

    use crate::requests::{Request, SignatureHelp as SignatureHelpRequest};
    use crate::testing::{conversion_tests, line_position, state_with_documents};

    conversion_tests! {
        signature_help_position_converts_incoming: SignatureHelpRequest {
            params: |uri| SignatureHelpParams {
                context: None,
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
        }
    }

    #[test]
    fn signature_help_label_offsets_recount_against_the_label_string() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(LspSignatureHelp {
            signatures: vec![SignatureInformation {
                // UTF-8 bytes: 🙂 = 0..4, f = 4, ( = 5, a = 6, ) = 7;
                // UTF-16 units: 🙂 = 0..2, f = 2, ( = 3, a = 4, ) = 5.
                label: "🙂f(a)".into(),
                documentation: None,
                parameters: Some(vec![
                    ParameterInformation {
                        label: ParameterLabel::LabelOffsets([4, 5]),
                        documentation: None,
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("a".into()),
                        documentation: None,
                    },
                ]),
                active_parameter: None,
            }],
            active_signature: None,
            active_parameter: None,
        });

        <SignatureHelpRequest as Request>::modify_response(&state, &document, &mut response);

        let parameters = response
            .expect("help present")
            .signatures
            .remove(0)
            .parameters
            .expect("parameters present");
        // The offsets count units of the label string itself: bytes 4..5
        // (the `f`) become units 2..3, with no document consulted.
        assert_eq!(parameters[0].label, ParameterLabel::LabelOffsets([2, 3]));
        assert_eq!(parameters[1].label, ParameterLabel::Simple("a".into()));
    }
}
