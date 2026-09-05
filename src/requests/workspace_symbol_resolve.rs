use async_lsp::lsp_types::{OneOf, WorkspaceSymbol as LspWorkspaceSymbol};

use crate::server::{ServerState, read_document_from_disk};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub(crate) struct WorkspaceSymbolResolve;

impl Request for WorkspaceSymbolResolve {
    type Params = LspWorkspaceSymbol;
    type Response = LspWorkspaceSymbol;

    // WorkspaceSymbol doesn't contain a request document: each location
    // below resolves against its own document. The standalone pair is
    // overridden INSTEAD of the anchored hooks — the resolve engine calls it
    // directly when no sole tracked document resolves, and the delegating
    // defaults of `modify_params`/`modify_response` keep it running in the
    // sole-document state (where `convert_resolve_item` routes through them).

    fn modify_params_standalone(state: &ServerState, params: &mut Self::Params) {
        convert_workspace_symbol_location(state, params, Direction::Incoming);
    }

    fn modify_response_standalone(state: &ServerState, response: &mut Self::Response) {
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
