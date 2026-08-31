use std::{ops::ControlFlow, sync::Arc};

use async_lsp::{
    ClientSocket, ErrorCode, LanguageServer, ResponseError, Result,
    lsp_types::{
        CodeAction, CompletionItem, DidChangeConfigurationParams, DidChangeTextDocumentParams,
        DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, Url,
        WorkspaceDiagnosticParams, WorkspaceDiagnosticReportResult,
    },
};
use futures::future::BoxFuture;

#[cfg(feature = "tracing")]
use tracing::debug;

use crate::{
    requests::{CodeActionResolve, CompletionResolve, Direction, convert_resolve_item},
    server::{Server, ServerState},
    text_utils::Encoding,
};

mod initialize;

const POSITION_ENCODING_PREFERRED_ORDER: [Encoding; 3] = [
    // First, prefer to use UTF-8 encoding, since this will make all of
    // the conversions for the custom language server handlers zero-cost
    Encoding::UTF8,
    // Second, prefer to use UTF-32 encoding, since this is
    // practically zero-cost for anything that Ropey needs
    Encoding::UTF32,
    // Lastly, use the standard UTF-16 encoding, which is universally
    // terrible, but also universally supported by all LSP clients
    Encoding::UTF16,
];

macro_rules! implement_method {
    ($async_lsp_method:ident => $our_server_trait_method:ident @ $request_type:ty) => {
        fn $async_lsp_method(
            &mut self,
            mut params: <$request_type as crate::requests::Request>::Params,
        ) -> BoxFuture<
            'static,
            Result<<$request_type as crate::requests::Request>::Response, Self::Error>,
        > {
            let server = Arc::clone(&self.server);
            let state = self.state.clone();
            Box::pin(async move {
                // 1. Try to extract the URL from the params for document tracking
                let url: Option<Url> =
                    <$request_type as crate::requests::Request>::extract_url(&params);
                let mut ver: Option<i32> = None;

                // 2. If we got an URL, track the document version & call the "modify params" callback
                if let Some(url) = url.as_ref() {
                    if let Some(doc) = state.document(url) {
                        ver.replace(doc.version());
                        <$request_type as crate::requests::Request>::modify_params(
                            &state,
                            &doc,
                            &mut params,
                        );
                    }
                }

                // 3. Call the user-defined language server function
                let mut result = server
                    .$our_server_trait_method(state.clone(), params)
                    .await?;

                // 4. Check our document again, if we had one originally
                if let Some(url) = url.as_ref() {
                    if let Some(doc) = state.document(url) {
                        // 4a. If the version changed, our result is stale, and we should try again
                        if ver.is_some_and(|v| v != doc.version()) {
                            return Err(ResponseError::new(
                                ErrorCode::CONTENT_MODIFIED,
                                "document was modified during processing",
                            ));
                        }
                        // 4b. Version is not stale, run the final "modify response" callback
                        <$request_type as crate::requests::Request>::modify_response(
                            &state,
                            &doc,
                            &mut result,
                        );
                    }
                }

                Ok(result)
            })
        }
    };
}

macro_rules! implement_methods {
    ($($lsp_method:ident => $server_method:ident @ $request_type:ty),* $(,)?) => {
        $(
            implement_method!($lsp_method => $server_method @ $request_type);
        )*
    };
}

/// The low-level language server implementation that automatically
/// manages documents and forwards requests to the underlying server.
///
/// Supports incremental updates of documents where possible, falling
/// back to other implementations whenever incremental updates fail.
pub(crate) struct LanguageServerWithState<T: Server> {
    server: Arc<T>,
    state: ServerState,
}

impl<T: Server> LanguageServerWithState<T> {
    pub(crate) fn new(client: ClientSocket, server: T) -> Self {
        let options = server.server_options();
        let server = Arc::new(server);
        let state = ServerState::with_options::<T>(client, &options);
        Self { server, state }
    }
}

impl<T: Server + Send + Sync + 'static> LanguageServer for LanguageServerWithState<T> {
    type Error = ResponseError;
    type NotifyResult = ControlFlow<async_lsp::Result<()>>;

    fn initialize(
        &mut self,
        params: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, Self::Error>> {
        LanguageServerWithState::initialize(self, params)
    }

    // Document notification callbacks & content updating

    fn initialized(&mut self, _params: InitializedParams) -> ControlFlow<Result<()>> {
        crate::workspace::initialized(self.state.clone());
        ControlFlow::Continue(())
    }

    fn did_change_configuration(
        &mut self,
        params: DidChangeConfigurationParams,
    ) -> ControlFlow<Result<()>> {
        crate::workspace::did_change_configuration(self.state.clone(), &params.settings);
        ControlFlow::Continue(())
    }

    fn did_change_workspace_folders(
        &mut self,
        params: DidChangeWorkspaceFoldersParams,
    ) -> ControlFlow<Result<()>> {
        self.state.handle_workspace_folders_change(params)
    }

    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_open: {}", params.text_document.uri);
        self.state.handle_document_open(params)
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_close: {}", params.text_document.uri);
        self.state.handle_document_close(params)
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> ControlFlow<Result<()>> {
        self.state.handle_document_change(params)
    }

    fn did_save(&mut self, params: DidSaveTextDocumentParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_save: {}", params.text_document.uri);
        self.state.handle_document_save(params)
    }

    fn workspace_diagnostic(
        &mut self,
        params: WorkspaceDiagnosticParams,
    ) -> BoxFuture<'static, Result<WorkspaceDiagnosticReportResult, Self::Error>> {
        let server = Arc::clone(&self.server);
        let state = self.state.clone();
        Box::pin(crate::workspace::workspace_diagnostic(
            server, state, params,
        ))
    }

    fn completion_item_resolve(
        &mut self,
        mut params: CompletionItem,
    ) -> BoxFuture<'static, Result<CompletionItem, Self::Error>> {
        let server = Arc::clone(&self.server);
        let state = self.state.clone();
        Box::pin(async move {
            let sole = {
                let documents = state.documents();
                (documents.len() == 1).then(|| documents[0].clone())
            };
            convert_resolve_item::<CompletionResolve, _>(
                &state,
                sole.as_ref(),
                &mut params,
                Direction::Incoming,
            );
            let mut result = server.completion_resolve(state.clone(), params).await?;
            convert_resolve_item::<CompletionResolve, _>(
                &state,
                sole.as_ref(),
                &mut result,
                Direction::Outgoing,
            );
            Ok(result)
        })
    }

    fn code_action_resolve(
        &mut self,
        mut params: CodeAction,
    ) -> BoxFuture<'static, Result<CodeAction, Self::Error>> {
        let server = Arc::clone(&self.server);
        let state = self.state.clone();
        Box::pin(async move {
            let sole = {
                let documents = state.documents();
                (documents.len() == 1).then(|| documents[0].clone())
            };
            convert_resolve_item::<CodeActionResolve, _>(
                &state,
                sole.as_ref(),
                &mut params,
                Direction::Incoming,
            );
            let mut result = server.code_action_resolve(state.clone(), params).await?;
            convert_resolve_item::<CodeActionResolve, _>(
                &state,
                sole.as_ref(),
                &mut result,
                Direction::Outgoing,
            );
            Ok(result)
        })
    }

    // async-lsp method name => our method name @ request type definition

    implement_methods!(
        hover                   => hover                 @ crate::requests::Hover,
        completion              => completion            @ crate::requests::Completion,
        code_action             => code_action           @ crate::requests::CodeAction,
        document_link           => link                  @ crate::requests::DocumentLink,
        document_link_resolve   => link_resolve          @ crate::requests::DocumentLinkResolve,
        declaration             => declaration           @ crate::requests::Declaration,
        definition              => definition            @ crate::requests::Definition,
        references              => references            @ crate::requests::References,
        rename                  => rename                @ crate::requests::Rename,
        prepare_rename          => rename_prepare        @ crate::requests::RenamePrepare,
        formatting              => document_format       @ crate::requests::DocumentFormat,
        range_formatting        => document_range_format @ crate::requests::DocumentRangeFormat,
        document_diagnostic     => document_diagnostics  @ crate::requests::DocumentDiagnostics,
    );
}

#[cfg(test)]
mod tests;
