use async_lsp::lsp_types::Url;

use crate::server::{Document, ServerState};

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

pub(crate) use code_action::CodeActionRequest;
pub(crate) use code_action_resolve::CodeActionResolveRequest;
pub(crate) use code_lens::CodeLensRequest;
pub(crate) use code_lens_resolve::CodeLensResolveRequest;
pub(crate) use color_presentation::ColorPresentationRequest;
pub(crate) use completion::CompletionRequest;
pub(crate) use completion_resolve::CompletionResolveRequest;
pub(crate) use conversion::{Direction, convert_resolve_item};
pub(crate) use declaration::DeclarationRequest;
pub(crate) use definition::DefinitionRequest;
pub(crate) use document_color::DocumentColorRequest;
pub(crate) use document_diagnostics::DocumentDiagnosticsRequest;
pub(crate) use document_format::DocumentFormatRequest;
pub(crate) use document_highlight::DocumentHighlightRequest;
pub(crate) use document_link::DocumentLinkRequest;
pub(crate) use document_link_resolve::DocumentLinkResolveRequest;
pub(crate) use document_range_format::DocumentRangeFormatRequest;
pub(crate) use document_symbol::DocumentSymbolRequest;
pub(crate) use execute_command::ExecuteCommandRequest;
pub(crate) use folding_range::FoldingRangeRequest;
pub(crate) use hover::HoverRequest;
pub(crate) use implementation::ImplementationRequest;
pub(crate) use incoming_calls::IncomingCallsRequest;
pub(crate) use inlay_hint::InlayHintRequest;
pub(crate) use inlay_hint_resolve::InlayHintResolveRequest;
pub(crate) use inline_value::InlineValueRequest;
pub(crate) use linked_editing_range::LinkedEditingRangeRequest;
pub(crate) use moniker::MonikerRequest;
pub(crate) use on_type_formatting::OnTypeFormattingRequest;
pub(crate) use outgoing_calls::OutgoingCallsRequest;
pub(crate) use prepare_call_hierarchy::CallHierarchyPrepareRequest;
pub(crate) use prepare_type_hierarchy::TypeHierarchyPrepareRequest;
pub(crate) use references::ReferencesRequest;
pub(crate) use rename::RenameRequest;
pub(crate) use rename_prepare::RenamePrepareRequest;
pub(crate) use selection_range::SelectionRangeRequest;
pub(crate) use semantic_tokens_full::SemanticTokensFullRequest;
pub(crate) use semantic_tokens_full_delta::SemanticTokensFullDeltaRequest;
pub(crate) use semantic_tokens_range::SemanticTokensRangeRequest;
pub(crate) use signature_help::SignatureHelpRequest;
pub(crate) use subtypes::SubtypesRequest;
pub(crate) use supertypes::SupertypesRequest;
pub(crate) use symbol::SymbolRequest;
pub(crate) use type_definition::TypeDefinitionRequest;
pub(crate) use will_create_files::WillCreateFilesRequest;
pub(crate) use will_delete_files::WillDeleteFilesRequest;
pub(crate) use will_rename_files::WillRenameFilesRequest;
pub(crate) use will_save_wait_until::WillSaveWaitUntilRequest;
pub(crate) use workspace_symbol_resolve::WorkspaceSymbolResolveRequest;

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
