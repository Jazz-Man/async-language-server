use crate::error::{ServerError, ServerResult};
use crate::server::{LanguageServerWithState, Server};
use crate::workspace::path_to_url;
use async_lsp::lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, DocumentDiagnosticParams,
    DocumentDiagnosticReportResult, GeneralClientCapabilities, InitializeParams, InitializedParams,
    PartialResultParams, PositionEncodingKind, TextDocumentIdentifier, TextDocumentItem,
    WorkDoneProgressParams, WorkspaceFolder,
};
use async_lsp::{ClientSocket, LanguageServer, ResponseError};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

pub(super) struct OneshotServer<S: Server> {
    inner: LanguageServerWithState<S>,
}

impl<S> OneshotServer<S>
where
    S: Server + Send + Sync + 'static,
{
    pub(super) fn new(server: S) -> Self {
        Self {
            inner: LanguageServerWithState::new(ClientSocket::new_closed(), server),
        }
    }

    pub(super) async fn initialize_workspace(&mut self, roots: &[PathBuf]) -> ServerResult<()> {
        self.inner
            .initialize(initialize_params(roots)?)
            .await
            .map_err(response_error)?;
        notify_result(self.inner.initialized(InitializedParams {}))
    }

    pub(super) fn open_document(&mut self, document: &OneshotDocument) -> ServerResult<()> {
        notify_result(self.inner.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                document.uri.clone(),
                document.language_id.clone(),
                document.version,
                document.text.clone(),
            ),
        }))
    }

    pub(super) async fn document_diagnostics(
        &mut self,
        document: &OneshotDocument,
    ) -> ServerResult<DocumentDiagnosticReportResult> {
        self.inner
            .document_diagnostic(DocumentDiagnosticParams {
                text_document: TextDocumentIdentifier::new(document.uri.clone()),
                identifier: None,
                previous_result_id: None,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .await
            .map_err(response_error)
    }
}

// Manual impl: the inner wrapper holds `S` behind an `Arc`, so cloning
// needs no `S: Clone` bound (a derive would impose one).
impl<S: Server> Clone for OneshotServer<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct OneshotDocument {
    pub(super) uri: async_lsp::lsp_types::Url,
    pub(super) language_id: String,
    pub(super) version: i32,
    pub(super) text: String,
}

fn initialize_params(roots: &[PathBuf]) -> ServerResult<InitializeParams> {
    let workspace_folders = roots
        .iter()
        .map(|root| workspace_folder(root.as_path()))
        .collect::<ServerResult<Vec<_>>>()?;

    Ok(InitializeParams {
        process_id: Some(std::process::id()),
        capabilities: ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: Some(vec![PositionEncodingKind::UTF8]),
                ..Default::default()
            }),
            ..Default::default()
        },
        workspace_folders: Some(workspace_folders),
        ..Default::default()
    })
}

fn workspace_folder(path: &Path) -> ServerResult<WorkspaceFolder> {
    let uri = path_to_url(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_owned();

    Ok(WorkspaceFolder { uri, name })
}

fn notify_result(result: ControlFlow<async_lsp::Result<()>>) -> ServerResult<()> {
    match result {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(result) => result.map_err(Into::into),
    }
}

fn response_error(error: ResponseError) -> ServerError {
    ServerError::rpc(error.code, error.message)
}
