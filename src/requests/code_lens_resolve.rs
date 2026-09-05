use async_lsp::lsp_types::CodeLens as LspCodeLens;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub(crate) struct CodeLensResolve;

impl Request for CodeLensResolve {
    type Params = LspCodeLens;
    type Response = LspCodeLens;

    // CodeLens doesn't contain a source document URI; the resolve
    // dispatch macro supplies the sole tracked document.

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_range(state, document, &mut response.range, Direction::Outgoing);
    }
}
