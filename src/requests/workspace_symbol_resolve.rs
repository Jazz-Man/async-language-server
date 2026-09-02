use async_lsp::lsp_types::{OneOf, WorkspaceSymbol as LspWorkspaceSymbol};

use crate::server::{Document, ServerState, read_document_from_disk};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct WorkspaceSymbolResolve;

impl Request for WorkspaceSymbolResolve {
    type Params = LspWorkspaceSymbol;
    type Response = LspWorkspaceSymbol;

    // WorkspaceSymbol doesn't contain a request document; the resolve
    // dispatch macro supplies the sole tracked document, but each location
    // below resolves against its own document instead.

    fn modify_params(state: &ServerState, _document: &Document, params: &mut Self::Params) {
        convert_workspace_symbol_location(state, params, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, _document: &Document, response: &mut Self::Response) {
        convert_workspace_symbol_location(state, response, Direction::Outgoing);
    }
}

/// Converts a workspace symbol's ranged location between the client
/// encoding and UTF-8: against the tracked document for its URL
/// (store-first), else against a snapshot read from disk. The
/// `Right(WorkspaceLocation)` variant carries no range and passes through
/// unchanged.
fn convert_workspace_symbol_location(
    state: &ServerState,
    symbol: &mut LspWorkspaceSymbol,
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
