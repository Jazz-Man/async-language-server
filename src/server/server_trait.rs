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
