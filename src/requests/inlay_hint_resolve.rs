use async_lsp::lsp_types::InlayHint as LspInlayHint;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_inlay_hint},
};

pub(crate) struct InlayHintResolve;

impl Request for InlayHintResolve {
    type Params = LspInlayHint;
    type Response = LspInlayHint;

    // InlayHint doesn't contain a source document URI; the resolve
    // dispatch macro supplies the sole tracked document.

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_inlay_hint(state, document, params, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_inlay_hint(state, document, response, Direction::Outgoing);
    }
}
