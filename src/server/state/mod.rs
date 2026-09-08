use crate::documents::{Document, DocumentMatchers};
use crate::server::{Server, ServerOptions};
use crate::text_utils::Encoding;
use crate::workspace::WorkspaceDiagnosticsState;
use async_lsp::ClientSocket;
use async_lsp::lsp_types::{SemanticToken, Url};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
    diagnostics_parallelism: usize,
    matchers: DocumentMatchers,
    encoding: Arc<Encoding>,
    semantic_tokens_cache: Arc<DashMap<Url, CachedSemanticTokens>>,
}

/// Filesystem stamp used to skip re-reading unchanged workspace files:
/// (modification time, size in bytes). Any doubt re-reads.
pub(crate) type FileStamp = (std::time::SystemTime, u64);

#[derive(Debug, Clone)]
struct DocumentEntry {
    document: Document,
    origin: DocumentOrigin,
    stamp: Option<FileStamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentOrigin {
    Open,
    Workspace,
}

/// A semantic tokens result cached for delta requests: the document's full
/// token stream in the server's UTF-8 encoding — what the server's next
/// delta is computed against — identified by its `result_id`.
#[derive(Debug, Clone)]
pub(crate) struct CachedSemanticTokens {
    pub(crate) result_id: String,
    pub(crate) data: Vec<SemanticToken>,
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
        let record = self.documents.get(url)?;
        Some(record.document.clone())
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

    /// Returns the version of the tracked document at `url`, if tracked.
    ///
    /// A clone-free probe: unlike [`ServerState::document`], it does not
    /// snapshot the document.
    pub(crate) fn document_version(&self, url: &Url) -> Option<i32> {
        self.documents
            .get(url)
            .map(|entry| entry.document.version())
    }

    /// Returns the sole tracked document when exactly one is tracked.
    ///
    /// The resolve-family heuristic: with zero or several tracked
    /// documents there is no sole document to convert against.
    pub(crate) fn sole_document(&self) -> Option<Document> {
        let mut entries = self.documents.iter();
        let first = entries.next()?.document.clone();
        entries.next().is_none().then_some(first)
    }
}

// Private implementation

impl ServerState {
    pub(crate) fn with_options<T: Server>(client: ClientSocket, options: &ServerOptions) -> Self {
        let documents = Arc::new(DashMap::new());
        let workspace_roots = Arc::new(DashMap::new());
        let workspace_diagnostics = WorkspaceDiagnosticsState::new(options);
        let diagnostics_parallelism = options.diagnostics_parallelism();
        let matchers = DocumentMatchers::new(T::server_document_matchers());
        let encoding = Arc::new(Encoding::default());
        let semantic_tokens_cache = Arc::new(DashMap::new());
        Self {
            client,
            documents,
            workspace_roots,
            workspace_diagnostics,
            diagnostics_parallelism,
            matchers,
            encoding,
            semantic_tokens_cache,
        }
    }

    pub(crate) fn workspace_diagnostics(&self) -> WorkspaceDiagnosticsState {
        self.workspace_diagnostics.clone()
    }

    /// How many documents the batch diagnostics pipeline may work on at
    /// once (`ServerOptions::with_diagnostics_parallelism`, defaulting to
    /// the CPU core count).
    pub(crate) fn diagnostics_parallelism(&self) -> usize {
        self.diagnostics_parallelism
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

    /// Gets the semantic tokens result cached for a document, if one was
    /// stored for its URL.
    ///
    /// The cached stream is the server's UTF-8 data — the state the
    /// client's `previous_result_id` refers to — never the client-encoded
    /// columns a response was converted to.
    #[must_use]
    pub(crate) fn cached_semantic_tokens(&self, url: &Url) -> Option<CachedSemanticTokens> {
        self.semantic_tokens_cache
            .get(url)
            .map(|entry| entry.value().clone())
    }

    /// Stores a semantic tokens result for a document, replacing any
    /// previous one stored for its URL.
    pub(crate) fn store_semantic_tokens(&self, url: &Url, cached: CachedSemanticTokens) {
        self.semantic_tokens_cache.insert(url.clone(), cached);
    }

    pub(crate) fn set_position_encoding(&mut self, kind: impl Into<Encoding>) {
        self.encoding = Arc::new(kind.into());
    }
}

#[cfg(test)]
mod tests;
