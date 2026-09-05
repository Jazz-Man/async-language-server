use async_lsp::lsp_types::{OneOf, WorkspaceSymbol};

use crate::server::{ServerState, read_document_from_disk};

use super::conversion::{Direction, convert_range};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::WorkspaceSymbol,
    response = async_lsp::lsp_types::WorkspaceSymbol,
    incoming_standalone(self::convert_params_standalone),
    outgoing_standalone(self::convert_response_standalone),
)]
pub(crate) struct WorkspaceSymbolResolveRequest;

// WorkspaceSymbol doesn't contain a request document: each location
// below resolves against its own document. The standalone pair is
// overridden INSTEAD of the anchored hooks — the resolve engine calls it
// directly when no sole tracked document resolves, and the delegating
// defaults of `modify_params`/`modify_response` keep it running in the
// sole-document state (where `convert_resolve_item` routes through them).

/// Converts the symbol's location to UTF-8 (the incoming standalone hook).
fn convert_params_standalone(state: &ServerState, params: &mut WorkspaceSymbol) {
    convert_workspace_symbol_location(state, params, Direction::Incoming);
}

/// Converts the symbol's location back to the client encoding (the outgoing
/// standalone hook).
fn convert_response_standalone(state: &ServerState, response: &mut WorkspaceSymbol) {
    convert_workspace_symbol_location(state, response, Direction::Outgoing);
}

/// Converts a workspace symbol's ranged location between the client
/// encoding and UTF-8: against the tracked document for its URL
/// (store-first), else against a snapshot read from disk. The
/// `Right(WorkspaceLocation)` variant carries no range and passes through
/// unchanged.
fn convert_workspace_symbol_location(
    state: &ServerState,
    symbol: &mut WorkspaceSymbol,
    direction: Direction,
) {
    let OneOf::Left(location) = &mut symbol.location else {
        return;
    };
    let uri = location.uri.clone();
    let Some(document) = state
        .document(&uri)
        .or_else(|| read_document_from_disk(&uri))
    else {
        return;
    };
    convert_range(state, &document, &mut location.range, direction);
}
