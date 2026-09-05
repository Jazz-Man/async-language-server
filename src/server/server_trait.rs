use async_lsp::{
    ErrorCode,
    lsp_types::{
        ClientCapabilities, CreateFilesParams, DeleteFilesParams, DidChangeConfigurationParams,
        DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
        RenameFilesParams, ServerCapabilities, ServerInfo, WillSaveTextDocumentParams,
        WorkDoneProgressCancelParams,
    },
};
use lsp_macros::lsp_method;

use crate::{
    documents::DocumentMatcher,
    error::{ServerError, ServerResult},
    server::{ServerOptions, ServerState},
};

/// Stamps `Server` trait methods for registry rows (normal methods default
/// to `METHOD_NOT_FOUND`; hook fields are matched and discarded here).
macro_rules! registry_trait_methods {
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
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                _params: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                method_not_implemented(stringify!($trait_name))
            }
        )*
    };
}

/// Stamps `Server` trait methods for resolve rows (default: item unchanged).
macro_rules! registry_trait_resolve_methods {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
        }
    )*) => {
        $(
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                item: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                async move { Ok(item) }
            }
        )*
    };
}

/// The main entrypoint to LSP functionality for a server.
///
/// All of the LSP methods in this trait are optional - if implemented,
/// the respective capabilities must also be registered using the
/// `server_capabilities` function.
///
/// The only exception to this rule are the `*_resolve` methods, which
/// default to doing nothing, and simply resolving the item as-is.
///
/// Handlers report failures by returning `Err(ServerError)`; the wrapper
/// converts them to LSP error responses.
pub trait Server {
    /// Returns the server name and version reported to the client during initialization.
    #[must_use]
    fn server_info() -> Option<ServerInfo> {
        None
    }

    /// Returns the options configuring this crate's behavior, read once during initialization.
    fn server_options(&self) -> ServerOptions {
        ServerOptions::default()
    }

    /// Returns the capabilities to advertise to the client during initialization.
    ///
    /// Merge the capabilities required by the implemented [`Server`] methods
    /// into a [`ServerCapabilities`] value. Returning `None` advertises only
    /// the crate's defaults.
    #[must_use]
    fn server_capabilities(_client_capabilities: ClientCapabilities) -> Option<ServerCapabilities> {
        None
    }

    /// Returns the matchers that associate documents with languages and grammars.
    ///
    /// See [`DocumentMatcher`] for how documents are matched.
    #[must_use]
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![]
    }

    lsp_method! {
        /// Handles `textDocument/hover` requests from the client.
        ///
        /// Returns hover contents for the position in `params`, or `None` when there is nothing to show. Positions and ranges are UTF-8. Requires a hover provider in [`Server::server_capabilities`].
        fn hover(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::HoverParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::Hover>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/declaration` requests from the client.
        ///
        /// Returns the declaration locations of the symbol at the position in `params`, or `None`. Requires a declaration provider in [`Server::server_capabilities`].
        fn declaration(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::request::GotoDeclarationParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::request::GotoDeclarationResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/definition` requests from the client.
        ///
        /// Returns the definition locations of the symbol at the position in `params`, or `None`. Requires a definition provider in [`Server::server_capabilities`].
        fn definition(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::GotoDefinitionParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::GotoDefinitionResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/references` requests from the client.
        ///
        /// Returns the locations that reference the symbol at the position in `params`, or `None`. Requires a references provider in [`Server::server_capabilities`].
        fn references(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::ReferenceParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::Location>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/documentLink` requests from the client.
        ///
        /// Returns links inside the document in `params`, or `None`. Requires a document link provider in [`Server::server_capabilities`].
        fn link(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentLinkParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::DocumentLink>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/rename` requests from the client.
        ///
        /// Returns a workspace edit renaming the symbol at the position in `params` to `params.new_name`, or `None` when renaming is not possible. Requires a rename provider in [`Server::server_capabilities`].
        fn rename(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::RenameParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::WorkspaceEdit>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/prepareRename` requests from the client.
        ///
        /// Returns the range of the symbol at the position in `params` that a rename would apply to, or `None` when renaming is not possible. Requires a rename provider with `prepare_provider` enabled.
        fn rename_prepare(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::TextDocumentPositionParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::PrepareRenameResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/formatting` requests from the client.
        ///
        /// Returns edits formatting the whole document in `params`, or `None`. Requires a document formatting provider in [`Server::server_capabilities`].
        fn document_format(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentFormattingParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TextEdit>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/rangeFormatting` requests from the client.
        ///
        /// Returns edits formatting the range in `params`, or `None`. Requires a document range formatting provider in [`Server::server_capabilities`].
        fn document_range_format(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentRangeFormattingParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TextEdit>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/implementation` requests from the client.
        ///
        /// Returns the implementation locations of the symbol at the position in `params`, or `None`. Requires an implementation provider in [`Server::server_capabilities`].
        fn implementation(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::request::GotoImplementationParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::request::GotoImplementationResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/typeDefinition` requests from the client.
        ///
        /// Returns the type definition locations of the symbol at the position in `params`, or `None`. Requires a type definition provider in [`Server::server_capabilities`].
        fn type_definition(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::request::GotoTypeDefinitionParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::request::GotoTypeDefinitionResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/documentHighlight` requests from the client.
        ///
        /// Returns the highlights of the symbol at the position in `params`, or `None`. Requires a document highlight provider in [`Server::server_capabilities`].
        fn document_highlight(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentHighlightParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::DocumentHighlight>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/onTypeFormatting` requests from the client.
        ///
        /// Returns edits formatting around the typed character at the position in `params`, or `None`. Requires a document on-type formatting provider in [`Server::server_capabilities`].
        fn on_type_formatting(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentOnTypeFormattingParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TextEdit>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/foldingRange` requests from the client.
        ///
        /// Returns the folding ranges of the document in `params`, or `None`. Requires a folding range provider in [`Server::server_capabilities`].
        fn folding_range(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::FoldingRangeParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::FoldingRange>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/linkedEditingRange` requests from the client.
        ///
        /// Returns the ranges that rename together with the symbol at the position in `params`, or `None`. Requires a linked editing range provider in [`Server::server_capabilities`].
        fn linked_editing_range(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::LinkedEditingRangeParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::LinkedEditingRanges>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/codeLens` requests from the client.
        ///
        /// Returns the code lenses of the document in `params`, or `None`. Requires a code lens provider in [`Server::server_capabilities`].
        fn code_lens(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CodeLensParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::CodeLens>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/willSaveWaitUntil` requests from the client.
        ///
        /// Returns edits applied to the document before it is saved, or `None`. Requires `will_save_wait_until` enabled in the text-document sync options of [`Server::server_capabilities`].
        fn will_save_wait_until(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::WillSaveTextDocumentParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TextEdit>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/documentColor` requests from the client.
        ///
        /// Returns all color references found in the document in `params`. Requires a color provider in [`Server::server_capabilities`].
        fn document_color(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentColorParams,
        ) -> impl Future<Output = ServerResult<Vec<async_lsp::lsp_types::ColorInformation>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/colorPresentation` requests from the client.
        ///
        /// Returns the presentations for the color at the range in `params`. Sent as the resolve leg of a document color provider.
        fn color_presentation(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::ColorPresentationParams,
        ) -> impl Future<Output = ServerResult<Vec<async_lsp::lsp_types::ColorPresentation>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/prepareCallHierarchy` requests from the client.
        ///
        /// Returns the call hierarchy items for the symbol at the position in `params`, or `None`. Requires a call hierarchy provider in [`Server::server_capabilities`].
        fn prepare_call_hierarchy(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CallHierarchyPrepareParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::CallHierarchyItem>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/prepareTypeHierarchy` requests from the client.
        ///
        /// Returns the type hierarchy items for the symbol at the position in `params`, or `None`. Requires a type hierarchy provider in [`Server::server_capabilities`].
        fn prepare_type_hierarchy(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::TypeHierarchyPrepareParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/moniker` requests from the client.
        ///
        /// Returns the symbol monikers at the position in `params`, or `None`. Requires a moniker provider in [`Server::server_capabilities`].
        fn moniker(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::MonikerParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::Moniker>>>> + Send;
    }

    lsp_method! {
        /// Handles `workspace/willCreateFiles` requests from the client.
        ///
        /// Returns a workspace edit applied before the files in `params` are created, or `None`. Requires `workspace.fileOperations.willCreate` in [`Server::server_capabilities`].
        fn will_create_files(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CreateFilesParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::WorkspaceEdit>>> + Send;
    }

    lsp_method! {
        /// Handles `workspace/willRenameFiles` requests from the client.
        ///
        /// Returns a workspace edit applied before the files in `params` are renamed, or `None`. Requires `workspace.fileOperations.willRename` in [`Server::server_capabilities`].
        fn will_rename_files(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::RenameFilesParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::WorkspaceEdit>>> + Send;
    }

    lsp_method! {
        /// Handles `workspace/willDeleteFiles` requests from the client.
        ///
        /// Returns a workspace edit applied before the files in `params` are deleted, or `None`. Requires `workspace.fileOperations.willDelete` in [`Server::server_capabilities`].
        fn will_delete_files(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DeleteFilesParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::WorkspaceEdit>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/inlayHint` requests from the client.
        ///
        /// Returns inlay hints for the range in `params`, or `None`. Requires an inlay hint provider in [`Server::server_capabilities`].
        fn inlay_hint(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::InlayHintParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::InlayHint>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/documentSymbol` requests from the client.
        ///
        /// Returns the symbol tree of the document in `params` (nested when the client supports it, flat otherwise), or `None`. Requires a document symbol provider in [`Server::server_capabilities`].
        fn document_symbol(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentSymbolParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::DocumentSymbolResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `workspace/executeCommand` requests from the client.
        ///
        /// Executes the command in `params` and returns an opaque result. Requires an execute command provider in [`Server::server_capabilities`].
        fn execute_command(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::ExecuteCommandParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::LSPAny>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/semanticTokens/full` requests from the client.
        ///
        /// Returns the document's full semantic token stream, or `None`. Token columns and lengths are UTF-8 here and converted to the negotiated encoding on the wire. Requires a semantic tokens provider in [`Server::server_capabilities`].
        fn semantic_tokens_full(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::SemanticTokensParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::SemanticTokensResult>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/semanticTokens/range` requests from the client.
        ///
        /// Returns the semantic token stream for the range in `params`, or `None`. Token columns and lengths are UTF-8 here and converted on the wire. Requires a semantic tokens provider with `range` support in [`Server::server_capabilities`].
        fn semantic_tokens_range(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::SemanticTokensRangeParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::SemanticTokensRangeResult>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/semanticTokens/full/delta` requests from the client.
        ///
        /// Returns edits transforming the previous token stream (identified by `params.previous_result_id`) into the current one, or a full stream when a delta is not practical. Token columns and lengths are UTF-8 here; edits' inserted tokens are converted seeded against the cached previous UTF-8 stream, and flat-array indices pass through unchanged. Requires a semantic tokens provider with `full.delta` support in [`Server::server_capabilities`].
        fn semantic_tokens_full_delta(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::SemanticTokensDeltaParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::SemanticTokensFullDeltaResult>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/completion` requests from the client.
        ///
        /// Returns completion items at the position in `params`, or `None`. Requires a completion provider in [`Server::server_capabilities`].
        fn completion(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CompletionParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::CompletionResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/codeAction` requests from the client.
        ///
        /// Returns code actions available for the range in `params`, or `None`. Requires a code action provider in [`Server::server_capabilities`].
        fn code_action(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CodeActionParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::CodeActionResponse>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/diagnostic` requests from the client.
        ///
        /// Returns the diagnostics for the document in `params`. The document's current snapshot is available through `state.document(&params.text_document.uri)`. Requires a diagnostic provider in [`Server::server_capabilities`].
        fn document_diagnostics(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::DocumentDiagnosticParams,
        ) -> impl Future<Output = ServerResult<async_lsp::lsp_types::DocumentDiagnosticReportResult>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/selectionRange` requests from the client.
        ///
        /// Returns the selection-range chains for the positions in `params`, or `None`; `positions[i]` must be contained in `result[i].range`. Requires a selection range provider in [`Server::server_capabilities`].
        fn selection_range(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::SelectionRangeParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::SelectionRange>>>> + Send;
    }

    lsp_method! {
        /// Handles `textDocument/inlineValue` requests from the client.
        ///
        /// Returns a single inline value computed for the range in `params`, or `None`. Requires an inline value provider in [`Server::server_capabilities`].
        fn inline_value(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::InlineValueParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::InlineValue>>> + Send;
    }

    lsp_method! {
        /// Handles `callHierarchy/incomingCalls` requests from the client.
        ///
        /// Returns the callers of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.
        fn incoming_calls(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CallHierarchyIncomingCallsParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>>> + Send;
    }

    lsp_method! {
        /// Handles `callHierarchy/outgoingCalls` requests from the client.
        ///
        /// Returns the callees of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. The item's own ranges arrive converted to UTF-8 and return converted to the negotiated encoding against the item's document when tracked; the response's `from_ranges` convert against the request document (the caller's).
        fn outgoing_calls(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::CallHierarchyOutgoingCallsParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>>> + Send;
    }

    lsp_method! {
        /// Handles `typeHierarchy/supertypes` requests from the client.
        ///
        /// Returns the supertypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.
        fn supertypes(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::TypeHierarchySupertypesParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>>> + Send;
    }

    lsp_method! {
        /// Handles `typeHierarchy/subtypes` requests from the client.
        ///
        /// Returns the subtypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. Conversion as per `supertypes`.
        fn subtypes(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::TypeHierarchySubtypesParams,
        ) -> impl Future<Output = ServerResult<Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>>> + Send;
    }

    crate::requests::registry::generated_methods!(registry_trait_methods);
    crate::requests::registry::custom_methods!(registry_trait_methods);
    crate::requests::registry::resolve_methods!(registry_trait_resolve_methods);

    // Notification hooks — called after each notification's internal
    // handler, so they observe post-internal state.

    /// Called after the internal handler processes a
    /// `workspace/didChangeConfiguration` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_change_configuration(
        &self,
        _state: &ServerState,
        _params: &DidChangeConfigurationParams,
    ) {
    }

    /// Called after the internal handler processes a
    /// `workspace/didChangeWorkspaceFolders` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_change_workspace_folders(
        &self,
        _state: &ServerState,
        _params: &DidChangeWorkspaceFoldersParams,
    ) {
    }

    /// Called after the internal handler processes a
    /// `textDocument/didOpen` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_open(&self, _state: &ServerState, _params: &DidOpenTextDocumentParams) {}

    /// Called after the internal handler processes a
    /// `textDocument/didClose` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_close(&self, _state: &ServerState, _params: &DidCloseTextDocumentParams) {}

    /// Called after the internal handler processes a
    /// `textDocument/didChange` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_change(&self, _state: &ServerState, _params: &DidChangeTextDocumentParams) {}

    /// Called after the internal handler processes a
    /// `textDocument/didSave` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_save(&self, _state: &ServerState, _params: &DidSaveTextDocumentParams) {}

    /// Called after the internal handler processes a
    /// `textDocument/willSave` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn will_save(&self, _state: &ServerState, _params: &WillSaveTextDocumentParams) {}

    /// Called after the internal handler processes a
    /// `workspace/didChangeWatchedFiles` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_change_watched_files(
        &self,
        _state: &ServerState,
        _params: &DidChangeWatchedFilesParams,
    ) {
    }

    /// Called after the internal handler processes a
    /// `workspace/didCreateFiles` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_create_files(&self, _state: &ServerState, _params: &CreateFilesParams) {}

    /// Called after the internal handler processes a
    /// `workspace/didRenameFiles` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_rename_files(&self, _state: &ServerState, _params: &RenameFilesParams) {}

    /// Called after the internal handler processes a
    /// `workspace/didDeleteFiles` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_delete_files(&self, _state: &ServerState, _params: &DeleteFilesParams) {}

    /// Called after the internal handler processes a
    /// `window/workDoneProgress/cancel` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn work_done_progress_cancel(
        &self,
        _state: &ServerState,
        _params: &WorkDoneProgressCancelParams,
    ) {
    }
}

fn method_not_implemented<T>(name: &'static str) -> std::future::Ready<Result<T, ServerError>> {
    std::future::ready(Err(ServerError::rpc(
        ErrorCode::METHOD_NOT_FOUND,
        format!("LSP method '{name}' has not been implemented"),
    )))
}
