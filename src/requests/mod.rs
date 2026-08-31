use async_lsp::lsp_types::Url;

use crate::server::{Document, ServerState};

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
