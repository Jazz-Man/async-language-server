use async_lsp::lsp_types::request::{
    GotoDeclarationParams as LspGotoDeclarationParams,
    GotoDeclarationResponse as LspGotoDeclarationResponse,
};

use crate::server::{Document, ServerState};

use super::{Request, conversion::modify_outgoing_goto_response};

pub struct Declaration;

impl Request for Declaration {
    type Params = LspGotoDeclarationParams;
    type Response = Option<LspGotoDeclarationResponse>;

    request_extract_url!(text_document_position_params.text_document);
    request_modify_params_position!(text_document_position_params.position);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        modify_outgoing_goto_response(state, document, response);
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, PartialResultParams,
        TextDocumentIdentifier, TextDocumentPositionParams, WorkDoneProgressParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::Declaration;

    conversion_tests! {
        declaration_round_trips_both_directions: Declaration {
            params: |uri| GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(GotoDefinitionResponse::Scalar(Location::new(
                emoji,
                same_line(0, 4, 4),
            ))),
            outgoing: |r| match r.as_ref() {
                Some(GotoDefinitionResponse::Scalar(loc)) => loc.range.start,
                _ => panic!("expected scalar location"),
            },
            returns: line_position(0, 2),
        }
    }
}
