use async_lsp::lsp_types::CodeLens;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_range};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CodeLens,
    response = async_lsp::lsp_types::CodeLens,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct CodeLensResolveRequest;

// CodeLens doesn't contain a source document URI; the resolve dispatch
// macro supplies the sole tracked document.

/// Converts the lens's range to UTF-8 (the incoming hook).
fn convert_params(state: &ServerState, document: &Document, params: &mut CodeLens) {
    convert_range(state, document, &mut params.range, Direction::Incoming);
}

/// Converts the lens's range back to the client encoding (the outgoing hook).
fn convert_response(state: &ServerState, document: &Document, response: &mut CodeLens) {
    convert_range(state, document, &mut response.range, Direction::Outgoing);
}
