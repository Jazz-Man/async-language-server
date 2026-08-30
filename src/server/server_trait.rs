use async_lsp::{
    ErrorCode,
    lsp_types::{
        ClientCapabilities, CodeAction, CodeActionParams, CodeActionResponse, CompletionItem,
        CompletionParams, CompletionResponse, DocumentDiagnosticParams,
        DocumentDiagnosticReportResult, DocumentFormattingParams, DocumentLink, DocumentLinkParams,
        DocumentRangeFormattingParams, GotoDefinitionParams, GotoDefinitionResponse, Hover,
        HoverParams, Location, PrepareRenameResponse, ReferenceParams, RenameParams,
        ServerCapabilities, ServerInfo, TextDocumentPositionParams, TextEdit, WorkspaceEdit,
        request::{GotoDeclarationParams, GotoDeclarationResponse},
    },
};

use crate::{
    documents::DocumentMatcher,
    error::{ServerError, ServerResult},
    server::{ServerOptions, ServerState},
};

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

    /// Handles `textDocument/hover` requests from the client.
    ///
    /// Returns hover contents for the position in `params`, or `None` when
    /// there is nothing to show. Positions and ranges are UTF-8. Requires a
    /// hover provider in [`Server::server_capabilities`].
    fn hover(
        &self,
        _state: ServerState,
        _params: HoverParams,
    ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
        method_not_implemented("hover")
    }

    /// Handles `textDocument/completion` requests from the client.
    ///
    /// Returns completion items at the position in `params`, or `None`.
    /// Requires a completion provider in [`Server::server_capabilities`].
    fn completion(
        &self,
        _state: ServerState,
        _params: CompletionParams,
    ) -> impl Future<Output = ServerResult<Option<CompletionResponse>>> + Send {
        method_not_implemented("completion")
    }

    /// Handles `completionItem/resolve` requests from the client.
    ///
    /// Fills in additional detail on an item previously returned by
    /// [`Server::completion`]. The default implementation resolves the item
    /// unchanged; returning the item as-is is always valid. Requires a
    /// completion provider with `resolve_provider` enabled. Positions in the
    /// incoming item are converted to UTF-8 before the handler runs, and
    /// positions in returned edits are converted back to the negotiated
    /// encoding — both against the sole tracked document, when exactly one
    /// document is tracked; otherwise they pass through unchanged.
    fn completion_resolve(
        &self,
        _state: ServerState,
        item: CompletionItem,
    ) -> impl Future<Output = ServerResult<CompletionItem>> + Send {
        async move { Ok(item) }
    }

    /// Handles `textDocument/codeAction` requests from the client.
    ///
    /// Returns code actions available for the range in `params`, or `None`.
    /// Requires a code action provider in [`Server::server_capabilities`].
    fn code_action(
        &self,
        _state: ServerState,
        _params: CodeActionParams,
    ) -> impl Future<Output = ServerResult<Option<CodeActionResponse>>> + Send {
        method_not_implemented("code_action")
    }

    /// Handles `codeAction/resolve` requests from the client.
    ///
    /// Fills in additional detail on an action previously returned by
    /// [`Server::code_action`]. The default implementation resolves the
    /// action unchanged. Requires a code action provider with
    /// `resolve_provider` enabled. Positions in the incoming action are
    /// converted to UTF-8 before the handler runs, and positions in returned
    /// edits are converted back to the negotiated encoding — both against
    /// the sole tracked document, when exactly one document is tracked;
    /// otherwise they pass through unchanged.
    fn code_action_resolve(
        &self,
        _state: ServerState,
        action: CodeAction,
    ) -> impl Future<Output = ServerResult<CodeAction>> + Send {
        async move { Ok(action) }
    }

    /// Handles `textDocument/documentLink` requests from the client.
    ///
    /// Returns links inside the document in `params`, or `None`. Requires a
    /// document link provider in [`Server::server_capabilities`].
    fn link(
        &self,
        _state: ServerState,
        _params: DocumentLinkParams,
    ) -> impl Future<Output = ServerResult<Option<Vec<DocumentLink>>>> + Send {
        method_not_implemented("link")
    }

    /// Handles `documentLink/resolve` requests from the client.
    ///
    /// Fills in the target of a link previously returned by [`Server::link`].
    /// The default implementation resolves the link unchanged. Requires a
    /// document link provider with `resolve_provider` enabled.
    fn link_resolve(
        &self,
        _state: ServerState,
        link: DocumentLink,
    ) -> impl Future<Output = ServerResult<DocumentLink>> + Send {
        async move { Ok(link) }
    }

    /// Handles `textDocument/declaration` requests from the client.
    ///
    /// Returns the declaration locations of the symbol at the position in
    /// `params`, or `None`. Requires a declaration provider in
    /// [`Server::server_capabilities`].
    fn declaration(
        &self,
        _state: ServerState,
        _params: GotoDeclarationParams,
    ) -> impl Future<Output = ServerResult<Option<GotoDeclarationResponse>>> + Send {
        method_not_implemented("declaration")
    }

    /// Handles `textDocument/definition` requests from the client.
    ///
    /// Returns the definition locations of the symbol at the position in
    /// `params`, or `None`. Requires a definition provider in
    /// [`Server::server_capabilities`].
    fn definition(
        &self,
        _state: ServerState,
        _params: GotoDefinitionParams,
    ) -> impl Future<Output = ServerResult<Option<GotoDefinitionResponse>>> + Send {
        method_not_implemented("definition")
    }

    /// Handles `textDocument/references` requests from the client.
    ///
    /// Returns the locations that reference the symbol at the position in
    /// `params`, or `None`. Requires a references provider in
    /// [`Server::server_capabilities`].
    fn references(
        &self,
        _state: ServerState,
        _params: ReferenceParams,
    ) -> impl Future<Output = ServerResult<Option<Vec<Location>>>> + Send {
        method_not_implemented("references")
    }

    /// Handles `textDocument/rename` requests from the client.
    ///
    /// Returns a workspace edit renaming the symbol at the position in
    /// `params` to `params.new_name`, or `None` when renaming is not
    /// possible. Requires a rename provider in [`Server::server_capabilities`].
    fn rename(
        &self,
        _state: ServerState,
        _params: RenameParams,
    ) -> impl Future<Output = ServerResult<Option<WorkspaceEdit>>> + Send {
        method_not_implemented("rename")
    }

    /// Handles `textDocument/prepareRename` requests from the client.
    ///
    /// Returns the range of the symbol at the position in `params` that a
    /// rename would apply to, or `None` when renaming is not possible.
    /// Requires a rename provider with `prepare_provider` enabled.
    fn rename_prepare(
        &self,
        _state: ServerState,
        _params: TextDocumentPositionParams,
    ) -> impl Future<Output = ServerResult<Option<PrepareRenameResponse>>> + Send {
        method_not_implemented("rename_prepare")
    }

    /// Handles `textDocument/formatting` requests from the client.
    ///
    /// Returns edits formatting the whole document in `params`, or `None`.
    /// Requires a document formatting provider in
    /// [`Server::server_capabilities`].
    fn document_format(
        &self,
        _state: ServerState,
        _params: DocumentFormattingParams,
    ) -> impl Future<Output = ServerResult<Option<Vec<TextEdit>>>> + Send {
        method_not_implemented("document_format")
    }

    /// Handles `textDocument/rangeFormatting` requests from the client.
    ///
    /// Returns edits formatting the range in `params`, or `None`. Requires a
    /// document range formatting provider in [`Server::server_capabilities`].
    fn document_range_format(
        &self,
        _state: ServerState,
        _params: DocumentRangeFormattingParams,
    ) -> impl Future<Output = ServerResult<Option<Vec<TextEdit>>>> + Send {
        method_not_implemented("document_range_format")
    }

    /// Handles `textDocument/diagnostic` requests from the client.
    ///
    /// Returns the diagnostics for the document in `params`. The document's
    /// current snapshot is available through
    /// `state.document(&params.text_document.uri)`. Requires a diagnostic
    /// provider in [`Server::server_capabilities`].
    fn document_diagnostics(
        &self,
        _state: ServerState,
        _params: DocumentDiagnosticParams,
    ) -> impl Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send {
        method_not_implemented("document_diagnostics")
    }
}

fn method_not_implemented<T>(name: &'static str) -> std::future::Ready<Result<T, ServerError>> {
    std::future::ready(Err(ServerError::rpc(
        ErrorCode::METHOD_NOT_FOUND,
        format!("LSP method '{name}' has not been implemented"),
    )))
}
