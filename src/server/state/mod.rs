use crate::documents::{Document, DocumentMatchers};
use crate::error::ServerError;
use crate::server::{MethodInventory, Server, ServerOptions};
use crate::text_utils::Encoding;
use crate::workspace::WorkspaceDiagnosticsState;
use async_lsp::ClientSocket;
use async_lsp::lsp_types::{SemanticToken, ServerCapabilities, Url};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod documents;
mod walk_cache;
mod workspace;

use self::walk_cache::WalkCache;

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
    ignore_filenames: Arc<[String]>,
    global_ignore_file: Option<PathBuf>,
    encoding: Arc<Encoding>,
    advertised_methods: MethodInventory,
    semantic_tokens_cache: Arc<DashMap<Url, CachedSemanticTokens>>,
    file_watching: Arc<AtomicBool>,
    watchers_registered: Arc<AtomicBool>,
    walk_cache: Arc<WalkCache>,
    conversion_fallbacks: Arc<DashMap<Url, (Option<FileStamp>, Document)>>,
}

/// Filesystem stamp used to skip re-reading unchanged workspace files:
/// (modification time, size in bytes). Any doubt re-reads.
pub(crate) type FileStamp = (std::time::SystemTime, u64);

/// Bound for the conversion-fallback cache: a pure optimization whose
/// entries may be dropped at any time, so a bound breach clears the
/// whole map rather than paying for an eviction policy.
const CONVERSION_FALLBACK_BOUND: usize = 128;

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
        let ignore_filenames: Arc<[String]> = options.ignore_filenames.iter().cloned().collect();
        let global_ignore_file = options.global_ignore_file.clone();
        let encoding = Arc::new(Encoding::default());
        let advertised_methods = MethodInventory::new();
        let semantic_tokens_cache = Arc::new(DashMap::new());
        Self {
            client,
            documents,
            workspace_roots,
            workspace_diagnostics,
            diagnostics_parallelism,
            matchers,
            ignore_filenames,
            global_ignore_file,
            encoding,
            advertised_methods,
            semantic_tokens_cache,
            file_watching: Arc::new(AtomicBool::new(false)),
            watchers_registered: Arc::new(AtomicBool::new(false)),
            walk_cache: Arc::new(WalkCache::new()),
            conversion_fallbacks: Arc::new(DashMap::new()),
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

    /// The configured ignore-file names (`ServerOptions::with_ignore_filenames`).
    pub(crate) fn ignore_filenames(&self) -> &[String] {
        &self.ignore_filenames
    }

    /// The configured global ignore file path, if any.
    pub(crate) fn global_ignore_file(&self) -> Option<&std::path::Path> {
        self.global_ignore_file.as_deref()
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

    /// Whether the client supports dynamic `didChangeWatchedFiles`
    /// registration — captured from its capabilities during initialize.
    pub(crate) fn file_watching(&self) -> bool {
        self.file_watching.load(Ordering::Relaxed)
    }

    pub(crate) fn set_file_watching(&self, supported: bool) {
        self.file_watching.store(supported, Ordering::Relaxed);
    }

    /// Whether the client accepted the `workspace/didChangeWatchedFiles`
    /// registration: set only by the registration's success arm, so a
    /// failed attempt stays false for the next enable transition to retry.
    /// The walk cache serves only under this flag.
    pub(crate) fn watchers_registered(&self) -> bool {
        self.watchers_registered.load(Ordering::Relaxed)
    }

    /// Records whether the client accepted the watcher registration.
    pub(crate) fn set_watchers_registered(&self, registered: bool) {
        self.watchers_registered
            .store(registered, Ordering::Relaxed);
    }

    /// Watcher glob patterns derived from the matchers' url globs: the same
    /// strings, filtered by the matcher's own `Glob::new` validity rule.
    /// Matchers without url globs contribute nothing.
    pub(crate) fn watcher_globs(&self) -> Vec<String> {
        let mut globs: Vec<_> = self.matchers.watcher_globs();
        globs.sort();
        globs.dedup();
        globs
    }

    /// The workspace walk cache: the file list between invalidation events.
    pub(crate) fn walk_cache(&self) -> &WalkCache {
        &self.walk_cache
    }

    /// Records which [`Server`] methods the capabilities sent to the client
    /// advertise. Called once per `initialize`, after the final
    /// `InitializeResult` is composed.
    pub(crate) fn set_advertised_methods(&mut self, caps: &ServerCapabilities) {
        self.advertised_methods = MethodInventory::from_capabilities(caps);
    }

    /// Warns once per method when an advertised method's trait default ran;
    /// returns whether this call warned. See [`MethodInventory`].
    pub(crate) fn warn_once_default(&self, method: &'static str, error: &ServerError) -> bool {
        self.advertised_methods.warn_once_default(method, error)
    }

    /// A disk snapshot for a file URL the server does not track, used by
    /// request conversions. Cache-only: filled by
    /// [`ServerState::prime_conversion_fallback`].
    pub(crate) fn fallback_document(&self, url: &Url) -> Option<Document> {
        self.conversion_fallbacks
            .get(url)
            .map(|entry| entry.value().1.clone())
    }

    /// Primes the fallback cache for `url` off the executor: reads the disk
    /// stamp, skips when the cached entry matches, and otherwise reads and
    /// installs the snapshot. Failures leave the cache untouched — the
    /// conversion then skips, exactly like today's failed disk read.
    pub(crate) async fn prime_conversion_fallback(&self, url: Url) {
        if url.scheme() != "file" {
            return;
        }
        let state = self.clone();
        if let Err(join_error) = tokio::task::spawn_blocking(move || {
            let Ok(path) = url.to_file_path() else {
                return;
            };
            // arch-lint: allow(no-sync-io) reason="the conversion-fallback stamp probe runs on the blocking pool by design"
            let stamp = std::fs::metadata(&path)
                .ok()
                .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
            if state
                .conversion_fallbacks
                .get(&url)
                .is_some_and(|entry| entry.value().0 == stamp)
            {
                return;
            }
            // arch-lint: allow(no-sync-io) reason="the conversion-fallback read runs on the blocking pool by design"
            let Ok(text) = std::fs::read_to_string(&path) else {
                return;
            };
            if state.conversion_fallbacks.len() >= CONVERSION_FALLBACK_BOUND {
                state.conversion_fallbacks.clear();
            }
            state.conversion_fallbacks.insert(
                url.clone(),
                (stamp, crate::server::document_from_disk_text(&url, text)),
            );
        })
        .await
        {
            tracing::warn!("conversion fallback prime failed: {join_error}");
        }
    }
}

#[cfg(test)]
mod tests;
