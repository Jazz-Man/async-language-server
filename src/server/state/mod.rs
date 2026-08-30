use std::{path::PathBuf, sync::Arc};

use async_lsp::{ClientSocket, lsp_types::Url};
use dashmap::DashMap;

use crate::{
    documents::{Document, DocumentMatchers},
    server::{Server, ServerOptions},
    text_utils::Encoding,
    workspace::WorkspaceDiagnosticsState,
};

mod documents;
mod workspace;

/// Managed state for an LSP server.
///
/// Provides access to and automatically tracks the connected
/// client, as well as opened documents and their changes.
#[derive(Debug, Clone)]
pub struct ServerState {
    client: ClientSocket,
    documents: Arc<DashMap<Url, DocumentEntry>>,
    workspace_roots: Arc<DashMap<Url, PathBuf>>,
    workspace_diagnostics: WorkspaceDiagnosticsState,
    matchers: DocumentMatchers,
    encoding: Arc<Encoding>,
}

#[derive(Debug, Clone)]
struct DocumentEntry {
    document: Document,
    origin: DocumentOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentOrigin {
    Open,
    Workspace,
}

impl ServerState {
    /// Gets a handle to the client connected to the server.
    ///
    /// Can be used to send requests and notifications to the client.
    #[must_use]
    pub fn client(&self) -> ClientSocket {
        self.client.clone()
    }

    /// Gets a snapshot of a document by its URL.
    ///
    /// This will return the document exactly as it was
    /// at the time of calling this method - any further
    /// modifications such as saves or edits will not be
    /// reflected in the returned document or its contents.
    ///
    /// Returns `None` if the document is not found.
    #[must_use]
    pub fn document(&self, url: &Url) -> Option<Document> {
        let entry = self.documents.get(url)?;
        Some(entry.document.clone())
    }

    /// Gets snapshots of all documents currently tracked by the server.
    ///
    /// Each document is returned exactly as it was at the time of
    /// calling this method, just like [`ServerState::document`].
    #[must_use]
    pub fn documents(&self) -> Vec<Document> {
        self.documents
            .iter()
            .map(|entry| entry.document.clone())
            .collect()
    }
}

// Private implementation

impl ServerState {
    pub(crate) fn with_options<T: Server>(client: ClientSocket, options: &ServerOptions) -> Self {
        let documents = Arc::new(DashMap::new());
        let workspace_roots = Arc::new(DashMap::new());
        let workspace_diagnostics = WorkspaceDiagnosticsState::new(options);
        let matchers = DocumentMatchers::new(T::server_document_matchers());
        let encoding = Arc::new(Encoding::default());
        Self {
            client,
            documents,
            workspace_roots,
            workspace_diagnostics,
            matchers,
            encoding,
        }
    }

    pub(crate) fn workspace_diagnostics(&self) -> WorkspaceDiagnosticsState {
        self.workspace_diagnostics.clone()
    }

    pub(crate) fn set_workspace_diagnostics_enabled(&self, enabled: bool) -> bool {
        let changed = self.workspace_diagnostics.set_enabled(enabled);
        if changed && !enabled {
            self.remove_workspace_documents();
        }
        changed
    }

    pub(crate) fn get_position_encoding(&self) -> Encoding {
        *self.encoding
    }

    pub(crate) fn set_position_encoding(&mut self, kind: impl Into<Encoding>) {
        self.encoding = Arc::new(kind.into());
    }
}

#[cfg(test)]
mod tests;
