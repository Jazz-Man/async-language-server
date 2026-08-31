use async_lsp::lsp_types::{
    GotoDefinitionParams as LspGotoDefinitionParams,
    GotoDefinitionResponse as LspGotoDefinitionResponse, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{
        modify_incoming_position, modify_outgoing_location, modify_outgoing_location_link,
    },
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
        modify_incoming_position(
            state,
            document,
            &mut params.text_document_position_params.position,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            match response {
                LspGotoDefinitionResponse::Scalar(loc) => {
                    modify_outgoing_location(state, document, loc);
                }
                LspGotoDefinitionResponse::Array(locations) => {
                    for loc in locations.iter_mut() {
                        modify_outgoing_location(state, document, loc);
                    }
                }
                LspGotoDefinitionResponse::Link(links) => {
                    for link in links.iter_mut() {
                        modify_outgoing_location_link(state, document, link);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{GotoDefinitionResponse, Location};

    use crate::requests::testing::{r, state_with_documents};

    use super::{Definition, Request};

    #[test]
    fn definition_locations_are_converted_using_their_own_document() {
        let (state, source, target) = state_with_documents();
        let document = state.document(&source).unwrap();
        let mut response = Some(GotoDefinitionResponse::Scalar(Location::new(
            target,
            r(0, 4, 4),
        )));

        <Definition as Request>::modify_response(&state, &document, &mut response);

        let Some(GotoDefinitionResponse::Scalar(loc)) = response else {
            panic!("expected scalar location");
        };
        assert_eq!(loc.range, r(0, 2, 2));
    }
}
