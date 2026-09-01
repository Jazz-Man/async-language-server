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

/// Implements [`Request::modify_params`] inside an existing `impl Request`
/// block, for a request whose params carry one incoming range at the given
/// field path, e.g. `range`: the generated body delegates to `convert_range`
/// with `Direction::Incoming`.
macro_rules! request_modify_params_range {
    ($($segment:ident).*) => {
        fn modify_params(
            state: &crate::server::ServerState,
            document: &crate::server::Document,
            params: &mut Self::Params,
        ) {
            crate::requests::conversion::convert_range(
                state,
                document,
                &mut params $(.$segment)*,
                crate::requests::conversion::Direction::Incoming,
            );
        }
    };
}

/// Stamps `Request` impls for the registry's generated rows.
macro_rules! registry_request_impls {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
            $(document: $($dseg:ident).+,)?
            $(incoming: position at $($pseg:ident).+,)?
            $(incoming: range at $($rseg:ident).+,)?
            $(outgoing: $outgoing:ident,)?
        }
    )*) => {
        $(
            pub struct $req;

            impl Request for $req {
                type Params = $params;
                type Response = $response;

                $(request_extract_url!($($dseg).+);)?
                $(request_modify_params_position!($($pseg).+);)?
                $(request_modify_params_range!($($rseg).+);)?
                $(
                fn modify_response(
                    state: &crate::server::ServerState,
                    document: &crate::server::Document,
                    response: &mut Self::Response,
                ) {
                    $crate::requests::conversion::$outgoing(state, document, response);
                }
                )?
            }
        )*
    };
}

mod code_action;
mod code_action_resolve;
mod code_lens;
mod color_presentation;
mod completion;
mod completion_resolve;
mod conversion;
mod declaration;
mod definition;
mod document_color;
mod document_diagnostics;
mod document_format;
mod document_highlight;
mod document_link;
mod document_link_resolve;
mod document_range_format;
mod folding_range;
mod hover;
mod implementation;
mod incoming_calls;
mod linked_editing_range;
mod moniker;
mod on_type_formatting;
mod outgoing_calls;
mod prepare_call_hierarchy;
mod prepare_type_hierarchy;
mod references;
pub(crate) mod registry;
mod rename;
mod rename_prepare;
mod selection_range;
mod subtypes;
mod supertypes;
mod type_definition;
mod will_create_files;
mod will_delete_files;
mod will_rename_files;
mod will_save_wait_until;

pub(crate) use code_action::CodeAction;
pub(crate) use code_action_resolve::CodeActionResolve;
pub(crate) use completion::Completion;
pub(crate) use completion_resolve::CompletionResolve;
pub(crate) use conversion::{Direction, convert_resolve_item};
pub(crate) use document_diagnostics::DocumentDiagnostics;
pub(crate) use document_link_resolve::DocumentLinkResolve;
pub(crate) use incoming_calls::IncomingCalls;
pub(crate) use outgoing_calls::OutgoingCalls;
pub(crate) use selection_range::SelectionRange;
pub(crate) use subtypes::Subtypes;
pub(crate) use supertypes::Supertypes;

crate::requests::registry::generated_methods!(registry_request_impls);

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
