use std::{ops::ControlFlow, sync::Arc};

use async_lsp::{
    ClientSocket, ErrorCode, LanguageServer, ResponseError, Result,
    lsp_types::{
        CreateFilesParams, DeleteFilesParams, DidChangeConfigurationParams,
        DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
        InitializeParams, InitializeResult, InitializedParams, RenameFilesParams, Url,
        WillSaveTextDocumentParams, WorkDoneProgressCancelParams, WorkspaceDiagnosticParams,
        WorkspaceDiagnosticReportResult,
    },
};
use futures::future::BoxFuture;
use lsp_macros::lsp_dispatch;
use ropey::Rope;

use tracing::debug;

use crate::{
    documents::Document,
    requests::{Direction, convert_resolve_item},
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

                // 2. If we got an URL, track the document version
                if let Some(url) = url.as_ref()
                    && let Some(doc) = state.document(url)
                {
                    ver.replace(doc.version());
                }

                // 3. Call the "modify params" callback against the request's
                //    conversion document: the tracked snapshot for a tracked
                //    URL, a disk snapshot for an untracked file URL, or the
                //    sole tracked document for URL-less requests
                let params_doc = conversion_document(&state, url.as_ref());
                if let Some(doc) = params_doc.as_ref() {
                    <$request_type as crate::requests::Request>::modify_params(
                        &state,
                        doc,
                        &mut params,
                    );
                }

                // 4. Call the user-defined language server function
                let mut result = server
                    .$our_server_trait_method(state.clone(), params)
                    .await?;

                // 5. Check our document again, if we had one originally. If the
                //    version changed, our result is stale, and we should try again
                if let Some(url) = url.as_ref()
                    && let Some(doc) = state.document(url)
                    && ver.is_some_and(|v| v != doc.version())
                {
                    return Err(ResponseError::new(
                        ErrorCode::CONTENT_MODIFIED,
                        "document was modified during processing",
                    ));
                }

                // 6. Run the final "modify response" callback against a freshly
                //    resolved conversion document; when none resolves, the
                //    standalone hook runs state-driven conversions instead of
                //    skipping them.
                match conversion_document(&state, url.as_ref()) {
                    Some(doc) => {
                        <$request_type as crate::requests::Request>::modify_response(
                            &state,
                            &doc,
                            &mut result,
                        );
                    }
                    None => {
                        <$request_type as crate::requests::Request>::modify_response_standalone(
                            &state,
                            &mut result,
                        );
                    }
                }

                Ok(result)
            })
        }
    };
}

/// Resolves the document a request's conversions run against: the
/// tracked snapshot for `url` when tracked; otherwise, for file URLs, a
/// per-request snapshot read from disk (best-effort — unreadable or
/// non-file URLs convert nothing, the historical behavior); for URL-less
/// requests, the sole tracked document when exactly one is tracked (the
/// resolve-family heuristic), else none.
fn conversion_document(state: &ServerState, url: Option<&Url>) -> Option<Document> {
    let Some(url) = url else {
        let documents = state.documents();
        return (documents.len() == 1).then(|| documents[0].clone());
    };
    state.document(url).or_else(|| read_document_from_disk(url))
}

/// Reads a per-request document snapshot from a file URL. Blocking by
/// design, matching the crate's other disk reads; never panics on
/// external input — failures return `None` and conversion is skipped.
pub(crate) fn read_document_from_disk(url: &Url) -> Option<Document> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    // arch-lint: allow(no-sync-io) reason="the dispatch fallback reads one file per request via std::fs, matching the crate's other synchronous disk reads"
    let text = std::fs::read_to_string(path).ok()?;
    Some(Document {
        uri: url.clone(),
        text: Rope::from(text),
        version: 0,
        language: String::new(),
        matcher: None,
        #[cfg(feature = "tree-sitter")]
        tree_sitter_lang: None,
        #[cfg(feature = "tree-sitter")]
        tree_sitter_tree: None,
    })
}

macro_rules! implement_methods {
    ($($lsp_method:ident => $server_method:ident @ $request_type:ty),* $(,)?) => {
        $(
            implement_method!($lsp_method => $server_method @ $request_type);
        )*
    };
}

macro_rules! implement_resolve_method {
    ($lsp_method:ident => $server_method:ident @ $request_type:ty) => {
        fn $lsp_method(
            &mut self,
            mut params: <$request_type as crate::requests::Request>::Params,
        ) -> BoxFuture<
            'static,
            Result<<$request_type as crate::requests::Request>::Response, Self::Error>,
        > {
            let server = Arc::clone(&self.server);
            let state = self.state.clone();
            Box::pin(async move {
                // Resolve requests carry no text-document URL: convert against
                // the sole tracked document, if the server tracks exactly one;
                // with no sole document, the standalone hooks run state-driven
                // conversions instead of skipping them.
                let sole = conversion_document(&state, None);
                match sole.as_ref() {
                    Some(document) => {
                        convert_resolve_item::<$request_type, _>(
                            &state,
                            Some(document),
                            &mut params,
                            Direction::Incoming,
                        );
                    }
                    None => {
                        <$request_type as crate::requests::Request>::modify_params_standalone(
                            &state,
                            &mut params,
                        );
                    }
                }
                let mut result = server.$server_method(state.clone(), params).await?;
                match sole.as_ref() {
                    Some(document) => {
                        convert_resolve_item::<$request_type, _>(
                            &state,
                            Some(document),
                            &mut result,
                            Direction::Outgoing,
                        );
                    }
                    None => {
                        <$request_type as crate::requests::Request>::modify_response_standalone(
                            &state,
                            &mut result,
                        );
                    }
                }
                Ok(result)
            })
        }
    };
}

/// Stamps dispatch entries for registry rows through the existing engine.
macro_rules! registry_dispatch {
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
        implement_methods!(
            $( $alsp_name => $trait_name @ crate::requests::$req, )*
        );
    };
}

macro_rules! registry_dispatch_resolve {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
        }
    )*) => {
        $(
            implement_resolve_method!($alsp_name => $trait_name @ crate::requests::$req);
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
        self.server.did_change_configuration(&self.state, &params);
        ControlFlow::Continue(())
    }

    fn did_change_workspace_folders(
        &mut self,
        params: DidChangeWorkspaceFoldersParams,
    ) -> ControlFlow<Result<()>> {
        let result = self.state.handle_workspace_folders_change(params.clone());
        self.server
            .did_change_workspace_folders(&self.state, &params);
        result
    }

    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> ControlFlow<Result<()>> {
        debug!("did_open: {}", params.text_document.uri);
        let result = self.state.handle_document_open(params.clone());
        self.server.did_open(&self.state, &params);
        result
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> ControlFlow<Result<()>> {
        debug!("did_close: {}", params.text_document.uri);
        let result = self.state.handle_document_close(params.clone());
        self.server.did_close(&self.state, &params);
        result
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> ControlFlow<Result<()>> {
        let result = self.state.handle_document_change(params.clone());
        self.server.did_change(&self.state, &params);
        result
    }

    fn did_save(&mut self, params: DidSaveTextDocumentParams) -> ControlFlow<Result<()>> {
        debug!("did_save: {}", params.text_document.uri);
        let result = self.state.handle_document_save(params.clone());
        self.server.did_save(&self.state, &params);
        result
    }

    fn will_save(&mut self, params: WillSaveTextDocumentParams) -> ControlFlow<Result<()>> {
        debug!("will_save: {}", params.text_document.uri);
        self.server.will_save(&self.state, &params);
        ControlFlow::Continue(())
    }

    fn did_change_watched_files(
        &mut self,
        params: DidChangeWatchedFilesParams,
    ) -> ControlFlow<Result<()>> {
        debug!("did_change_watched_files: {} events", params.changes.len());
        let result = self
            .state
            .handle_watched_files_change(params.changes.clone());
        self.server.did_change_watched_files(&self.state, &params);
        result
    }

    fn did_create_files(&mut self, params: CreateFilesParams) -> ControlFlow<Result<()>> {
        debug!("did_create_files: {} files", params.files.len());
        self.server.did_create_files(&self.state, &params);
        ControlFlow::Continue(())
    }

    fn did_rename_files(&mut self, params: RenameFilesParams) -> ControlFlow<Result<()>> {
        debug!("did_rename_files: {} files", params.files.len());
        let result = self.state.handle_files_renamed(params.files.clone());
        self.server.did_rename_files(&self.state, &params);
        result
    }

    fn did_delete_files(&mut self, params: DeleteFilesParams) -> ControlFlow<Result<()>> {
        debug!("did_delete_files: {} files", params.files.len());
        let result = self.state.handle_files_deleted(params.files.clone());
        self.server.did_delete_files(&self.state, &params);
        result
    }

    fn work_done_progress_cancel(
        &mut self,
        params: WorkDoneProgressCancelParams,
    ) -> ControlFlow<Result<()>> {
        debug!("work_done_progress_cancel: {:?}", params.token);
        self.server.work_done_progress_cancel(&self.state, &params);
        ControlFlow::Continue(())
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

    // async-lsp method name => our method name @ request type definition,
    // stamped from the registry (src/requests/registry.rs)

    crate::requests::registry::generated_methods!(registry_dispatch);
    crate::requests::registry::custom_methods!(registry_dispatch);
    crate::requests::registry::resolve_methods!(registry_dispatch_resolve);

    lsp_dispatch! {
        hover: hover @ crate::requests::HoverRequest,
        declaration: declaration @ crate::requests::DeclarationRequest,
        definition: definition @ crate::requests::DefinitionRequest,
        references: references @ crate::requests::ReferencesRequest,
        link: document_link @ crate::requests::DocumentLinkRequest,
        rename: rename @ crate::requests::RenameRequest,
        rename_prepare: prepare_rename @ crate::requests::RenamePrepareRequest,
        document_format: formatting @ crate::requests::DocumentFormatRequest,
        document_range_format: range_formatting @ crate::requests::DocumentRangeFormatRequest,
        implementation: implementation @ crate::requests::ImplementationRequest,
        type_definition: type_definition @ crate::requests::TypeDefinitionRequest,
        document_highlight: document_highlight @ crate::requests::DocumentHighlightRequest,
        on_type_formatting: on_type_formatting @ crate::requests::OnTypeFormattingRequest,
        folding_range: folding_range @ crate::requests::FoldingRangeRequest,
        linked_editing_range: linked_editing_range @ crate::requests::LinkedEditingRangeRequest,
        code_lens: code_lens @ crate::requests::CodeLensRequest,
        will_save_wait_until: will_save_wait_until @ crate::requests::WillSaveWaitUntilRequest,
        document_color: document_color @ crate::requests::DocumentColorRequest,
        color_presentation: color_presentation @ crate::requests::ColorPresentationRequest,
        prepare_call_hierarchy: prepare_call_hierarchy @ crate::requests::CallHierarchyPrepareRequest,
        prepare_type_hierarchy: prepare_type_hierarchy @ crate::requests::TypeHierarchyPrepareRequest,
        moniker: moniker @ crate::requests::MonikerRequest,
        will_create_files: will_create_files @ crate::requests::WillCreateFilesRequest,
        will_rename_files: will_rename_files @ crate::requests::WillRenameFilesRequest,
        will_delete_files: will_delete_files @ crate::requests::WillDeleteFilesRequest,
        inlay_hint: inlay_hint @ crate::requests::InlayHintRequest,
        document_symbol: document_symbol @ crate::requests::DocumentSymbolRequest,
        execute_command: execute_command @ crate::requests::ExecuteCommandRequest,
        semantic_tokens_full: semantic_tokens_full @ crate::requests::SemanticTokensFullRequest,
        semantic_tokens_range: semantic_tokens_range @ crate::requests::SemanticTokensRangeRequest,
        semantic_tokens_full_delta: semantic_tokens_full_delta @ crate::requests::SemanticTokensFullDeltaRequest,
        completion: completion @ crate::requests::CompletionRequest,
        code_action: code_action @ crate::requests::CodeActionRequest,
        document_diagnostics: document_diagnostic @ crate::requests::DocumentDiagnosticsRequest,
        selection_range: selection_range @ crate::requests::SelectionRangeRequest,
        inline_value: inline_value @ crate::requests::InlineValueRequest,
        incoming_calls: incoming_calls @ crate::requests::IncomingCallsRequest,
        outgoing_calls: outgoing_calls @ crate::requests::OutgoingCallsRequest,
        supertypes: supertypes @ crate::requests::SupertypesRequest,
        subtypes: subtypes @ crate::requests::SubtypesRequest,
    }
}

#[cfg(test)]
mod tests;
