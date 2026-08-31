use async_lsp::lsp_types::{
    GotoDefinitionParams as LspGotoDefinitionParams,
    GotoDefinitionResponse as LspGotoDefinitionResponse, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_position, modify_outgoing_goto_response},
};

pub struct Definition;

impl Request for Definition {
    type Params = LspGotoDefinitionParams;
    type Response = Option<LspGotoDefinitionResponse>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(
            params
                .text_document_position_params
                .text_document
                .uri
                .clone(),
        )
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_position(
            state,
            document,
            &mut params.text_document_position_params.position,
            Direction::Incoming,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        modify_outgoing_goto_response(state, document, response);
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{GotoDefinitionResponse, Location};

    use crate::testing::{same_line, state_with_documents};

    use super::{Definition, Request};

    #[test]
    fn definition_locations_are_converted_using_their_own_document() {
        let (state, source, target) = state_with_documents();
        let document = state.document(&source).unwrap();
        let mut response = Some(GotoDefinitionResponse::Scalar(Location::new(
            target,
            same_line(0, 4, 4),
        )));

        <Definition as Request>::modify_response(&state, &document, &mut response);

        let Some(GotoDefinitionResponse::Scalar(loc)) = response else {
            panic!("expected scalar location");
        };
        assert_eq!(loc.range, same_line(0, 2, 2));
    }
}
