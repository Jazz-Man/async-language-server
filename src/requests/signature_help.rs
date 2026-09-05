use async_lsp::lsp_types::SignatureHelpParams;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_signature_help_label_offsets};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SignatureHelpParams,
    response = Option<async_lsp::lsp_types::SignatureHelp>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    incoming_custom(self::convert_context_label_offsets),
    outgoing(crate::requests::conversion::modify_outgoing_signature_help),
)]
pub(crate) struct SignatureHelpRequest;

/// Converts the echoed context help's label offsets to UTF-8 (the custom
/// incoming step, composed after the standard position conversion).
fn convert_context_label_offsets(
    state: &ServerState,
    document: &Document,
    params: &mut SignatureHelpParams,
) {
    if let Some(help) = params
        .context
        .as_mut()
        .and_then(|context| context.active_signature_help.as_mut())
    {
        convert_signature_help_label_offsets(state, document, help, Direction::Incoming);
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        ParameterInformation, ParameterLabel, SignatureHelp as LspSignatureHelp,
        SignatureHelpContext, SignatureHelpParams, SignatureHelpTriggerKind, SignatureInformation,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use lsp_macros::conversion_tests;

    use crate::requests::{Request, SignatureHelpRequest};
    use crate::testing::{line_position, state_with_documents};

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

    #[test]
    fn signature_help_context_label_offsets_convert_incoming() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut params = SignatureHelpParams {
            context: Some(SignatureHelpContext {
                trigger_kind: SignatureHelpTriggerKind::INVOKED,
                trigger_character: None,
                is_retrigger: false,
                active_signature_help: Some(LspSignatureHelp {
                    signatures: vec![SignatureInformation {
                        // Client UTF-16 units: 🙂 = 0..2, f = 2, ( = 3, a = 4, ) = 5;
                        // UTF-8 bytes: 🙂 = 0..4, f = 4, ( = 5, a = 6, ) = 7.
                        label: "🙂f(a)".into(),
                        documentation: None,
                        parameters: Some(vec![ParameterInformation {
                            label: ParameterLabel::LabelOffsets([2, 3]),
                            documentation: None,
                        }]),
                        active_parameter: None,
                    }],
                    active_signature: None,
                    active_parameter: None,
                }),
            }),
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(emoji),
                line_position(0, 2),
            ),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        <SignatureHelpRequest as Request>::modify_params(&state, &document, &mut params);

        assert_eq!(
            params.text_document_position_params.position,
            line_position(0, 4)
        );
        let mut help = params
            .context
            .expect("context present")
            .active_signature_help
            .expect("help present");
        let parameters = help
            .signatures
            .remove(0)
            .parameters
            .expect("parameters present");
        // The echoed help's offsets count units of the label string itself:
        // UTF-16 units 2..3 (the `f`) become bytes 4..5, with no document
        // consulted.
        assert_eq!(parameters[0].label, ParameterLabel::LabelOffsets([4, 5]));
    }
}
