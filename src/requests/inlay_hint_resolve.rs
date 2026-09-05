use async_lsp::lsp_types::InlayHint;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_inlay_hint};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::InlayHint,
    response = async_lsp::lsp_types::InlayHint,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct InlayHintResolveRequest;

// InlayHint doesn't contain a source document URI; the resolve dispatch
// macro supplies the sole tracked document.

/// Converts the hint's position, edits, and label-part locations to UTF-8
/// (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut InlayHint) {
    convert_inlay_hint(state, document, params, Direction::Incoming);
}

/// Converts the hint's position, edits, and label-part locations back to the
/// client encoding (the outgoing hook).
fn convert_response(state: &ServerState, document: &Document, response: &mut InlayHint) {
    convert_inlay_hint(state, document, response, Direction::Outgoing);
}
