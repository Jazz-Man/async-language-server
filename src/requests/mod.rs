use async_lsp::lsp_types::Url;

use crate::server::{Document, ServerState};

/// Implements [`Request::extract_url`] inside an existing `impl Request`
/// block, for a request whose params carry the document URL at the given
/// field path, e.g. `text_document` or `text_document_position_params.text_document`.
macro_rules! request_extract_url {
    ($($segment:ident).*) => {
        fn extract_url(params: &Self::Params) -> Option<async_lsp::lsp_types::Url> {
            Some(params $(.$segment)* .uri.clone())
        }
    };
}

/// Implements [`Request::modify_params`] inside an existing `impl Request`
/// block, for a request whose params carry one incoming position at the
/// given field path, e.g. `text_document_position.position`: the generated
/// body delegates to `convert_position` with `Direction::Incoming`.
macro_rules! request_modify_params_position {
    ($($segment:ident).*) => {
        fn modify_params(
            state: &crate::server::ServerState,
            document: &crate::server::Document,
            params: &mut Self::Params,
        ) {
            crate::requests::conversion::convert_position(
                state,
                document,
                &mut params $(.$segment)*,
                crate::requests::conversion::Direction::Incoming,
            );
        }
    };
}

mod code_action;
mod code_action_resolve;
mod completion;
mod completion_resolve;
mod conversion;
mod declaration;
mod definition;
mod document_diagnostics;
mod document_format;
mod document_link;
mod document_link_resolve;
mod document_range_format;
mod hover;
mod references;
mod rename;
mod rename_prepare;

pub(crate) use code_action::CodeAction;
pub(crate) use code_action_resolve::CodeActionResolve;
pub(crate) use completion::Completion;
pub(crate) use completion_resolve::CompletionResolve;
pub(crate) use conversion::{Direction, convert_resolve_item};
pub(crate) use declaration::Declaration;
pub(crate) use definition::Definition;
pub(crate) use document_diagnostics::DocumentDiagnostics;
pub(crate) use document_format::DocumentFormat;
pub(crate) use document_link::DocumentLink;
pub(crate) use document_link_resolve::DocumentLinkResolve;
pub(crate) use document_range_format::DocumentRangeFormat;
pub(crate) use hover::Hover;
pub(crate) use references::References;
pub(crate) use rename::Rename;
pub(crate) use rename_prepare::RenamePrepare;

pub trait Request {
    type Params;
    type Response;

    fn extract_url(_params: &Self::Params) -> Option<Url> {
        None
    }

    fn modify_params(_state: &ServerState, _document: &Document, _params: &mut Self::Params) {}
    fn modify_response(_state: &ServerState, _document: &Document, _response: &mut Self::Response) {
    }
}
