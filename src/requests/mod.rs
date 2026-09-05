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
            pub(crate) struct $req;

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
mod code_lens_resolve;
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
mod document_symbol;
mod execute_command;
mod folding_range;
mod hover;
mod implementation;
mod incoming_calls;
mod inlay_hint;
mod inlay_hint_resolve;
mod inline_value;
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
mod semantic_tokens_full;
mod semantic_tokens_full_delta;
mod semantic_tokens_range;
mod signature_help;
mod subtypes;
mod supertypes;
mod symbol;
mod type_definition;
mod will_create_files;
mod will_delete_files;
mod will_rename_files;
mod will_save_wait_until;
mod workspace_symbol_resolve;

pub(crate) use code_action::CodeAction;
pub(crate) use code_action_resolve::CodeActionResolve;
pub(crate) use code_lens::CodeLensRequest;
pub(crate) use code_lens_resolve::CodeLensResolve;
pub(crate) use color_presentation::ColorPresentationRequest;
pub(crate) use completion::Completion;
pub(crate) use completion_resolve::CompletionResolve;
pub(crate) use conversion::{Direction, convert_resolve_item};
pub(crate) use declaration::DeclarationRequest;
pub(crate) use definition::DefinitionRequest;
pub(crate) use document_color::DocumentColorRequest;
pub(crate) use document_diagnostics::DocumentDiagnostics;
pub(crate) use document_format::DocumentFormatRequest;
pub(crate) use document_highlight::DocumentHighlightRequest;
pub(crate) use document_link::DocumentLinkRequest;
pub(crate) use document_link_resolve::DocumentLinkResolve;
pub(crate) use document_range_format::DocumentRangeFormatRequest;
pub(crate) use document_symbol::DocumentSymbolRequest;
pub(crate) use execute_command::ExecuteCommandRequest;
pub(crate) use folding_range::FoldingRangeRequest;
pub(crate) use hover::HoverRequest;
pub(crate) use implementation::ImplementationRequest;
pub(crate) use incoming_calls::IncomingCalls;
pub(crate) use inlay_hint::InlayHintRequest;
pub(crate) use inlay_hint_resolve::InlayHintResolve;
pub(crate) use inline_value::InlineValue;
pub(crate) use linked_editing_range::LinkedEditingRangeRequest;
pub(crate) use moniker::MonikerRequest;
pub(crate) use on_type_formatting::OnTypeFormattingRequest;
pub(crate) use outgoing_calls::OutgoingCalls;
pub(crate) use prepare_call_hierarchy::CallHierarchyPrepareRequest;
pub(crate) use prepare_type_hierarchy::TypeHierarchyPrepareRequest;
pub(crate) use references::ReferencesRequest;
pub(crate) use rename::RenameRequest;
pub(crate) use rename_prepare::RenamePrepareRequest;
pub(crate) use selection_range::SelectionRange;
pub(crate) use semantic_tokens_full::SemanticTokensFullRequest;
pub(crate) use semantic_tokens_full_delta::SemanticTokensFullDeltaRequest;
pub(crate) use semantic_tokens_range::SemanticTokensRangeRequest;
pub(crate) use signature_help::SignatureHelp;
pub(crate) use subtypes::Subtypes;
pub(crate) use supertypes::Supertypes;
pub(crate) use symbol::Symbol;
pub(crate) use type_definition::TypeDefinitionRequest;
pub(crate) use will_create_files::WillCreateFilesRequest;
pub(crate) use will_delete_files::WillDeleteFilesRequest;
pub(crate) use will_rename_files::WillRenameFilesRequest;
pub(crate) use will_save_wait_until::WillSaveWaitUntilRequest;
pub(crate) use workspace_symbol_resolve::WorkspaceSymbolResolve;

crate::requests::registry::generated_methods!(registry_request_impls);

pub(crate) trait Request {
    type Params;
    type Response;

    fn extract_url(_params: &Self::Params) -> Option<Url> {
        None
    }

    // Delegation, not a no-op: a request that overrides only the standalone
    // hook must run it here too, or the resolve engine's sole-document path
    // would skip its state-driven conversion in exactly that state.
    fn modify_params(state: &ServerState, _document: &Document, params: &mut Self::Params) {
        Self::modify_params_standalone(state, params);
    }

    // Delegation, not a no-op: a request that overrides only the standalone
    // hook must run it here too, or the engine's sole-document fallback for
    // URL-less requests would skip its state-driven conversion in exactly
    // that state.
    fn modify_response(state: &ServerState, _document: &Document, response: &mut Self::Response) {
        Self::modify_response_standalone(state, response);
    }

    /// Response conversion for requests with no document anchor.
    ///
    /// The dispatch engine calls this instead of [`Request::modify_response`]
    /// when it cannot resolve a single conversion document (URL-less
    /// requests in a zero- or multi-document state). Override it for
    /// state-driven conversions that resolve each position against its own
    /// document — the workspace-symbol shape; the default no-op passes the
    /// response through unchanged.
    fn modify_response_standalone(_state: &ServerState, _response: &mut Self::Response) {}

    /// Params conversion for resolve requests with no document anchor.
    ///
    /// The resolve engine calls this instead of [`Request::modify_params`]
    /// when no sole tracked document resolves. Default no-op; override for
    /// state-driven conversions that resolve their own documents (the
    /// workspace-symbol-resolve shape).
    fn modify_params_standalone(_state: &ServerState, _params: &mut Self::Params) {}
}
