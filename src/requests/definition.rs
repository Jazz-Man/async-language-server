use async_lsp::lsp_types::{
    GotoDefinitionParams as LspGotoDefinitionParams,
    GotoDefinitionResponse as LspGotoDefinitionResponse, Url,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_location, convert_position, modify_outgoing_location_link},
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
        if let Some(response) = response.as_mut() {
            match response {
                LspGotoDefinitionResponse::Scalar(loc) => {
                    convert_location(state, document, loc, Direction::Outgoing);
                }
                LspGotoDefinitionResponse::Array(locations) => {
                    for loc in locations.iter_mut() {
                        convert_location(state, document, loc, Direction::Outgoing);
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
