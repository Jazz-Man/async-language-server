use std::{fs, path::PathBuf};

use async_lsp::lsp_types::{
    Diagnostic, DocumentDiagnosticReport, DocumentDiagnosticReportKind,
    DocumentDiagnosticReportResult, Url,
};

use crate::{
    documents::DocumentMatchers,
    error::{ServerError, ServerResult},
    server::Server,
    workspace::{WorkspaceWalkConfig, WorkspaceWalker, for_each_bounded, path_to_url},
};

use super::server::{OneshotDocument, OneshotServer};

/// Configuration for running a language server once over a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceDiagnosticConfig {
    roots: Vec<PathBuf>,
    walk: WorkspaceWalkConfig,
}

impl WorkspaceDiagnosticConfig {
    /// Creates a new workspace diagnostic configuration for the given root.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            roots: vec![root.into()],
            walk: WorkspaceWalkConfig::default(),
        }
    }

    /// Adds another root to scan for matching documents.
    #[must_use]
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.roots.push(root.into());
        self
    }

    /// Adds several more roots to scan for matching documents.
    #[must_use]
    pub fn with_roots(mut self, root: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.roots.extend(root.into_iter().map(Into::into));
        self
    }

    /// Controls whether hidden files are included when scanning roots.
    #[must_use]
    pub fn with_hidden_files(mut self, yes: bool) -> Self {
        self.walk = self.walk.with_hidden_files(yes);
        self
    }

    /// Controls whether `.gitignore` and related ignore files are respected.
    #[must_use]
    pub fn with_ignore_files(mut self, yes: bool) -> Self {
        self.walk = self.walk.with_ignore_files(yes);
        self
    }
}

/// Diagnostics produced by running a server over a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceDiagnosticReport {
    /// Diagnostics for each matched document, one entry per document.
    pub documents: Vec<DocumentDiagnostics>,
}

impl WorkspaceDiagnosticReport {
    /// Returns `true` if no document reported diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.iter().all(DocumentDiagnostics::is_empty)
    }
}

/// Diagnostics produced for a single document in a workspace.
#[derive(Debug, Clone)]
pub struct DocumentDiagnostics {
    /// URI of the document the diagnostics belong to.
    pub uri: Url,
    /// Document version at the time the diagnostics were produced.
    pub version: i32,
    /// The diagnostic report returned by the server.
    pub report: DocumentDiagnosticReportResult,
}

impl DocumentDiagnostics {
    /// Returns `true` if this document diagnostic result contains no diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics().is_empty()
    }

    /// Gets all diagnostics contained in this document diagnostic result.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<&Diagnostic> {
        match &self.report {
            DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) => {
                report
                    .related_documents
                    .iter()
                    .flat_map(|related| related.values())
                    .flat_map(diagnostics_from_report_kind)
                    .chain(report.full_document_diagnostic_report.items.iter())
                    .collect()
            }
            DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(report)) => {
                report
                    .related_documents
                    .iter()
                    .flat_map(|related| related.values())
                    .flat_map(diagnostics_from_report_kind)
                    .collect()
            }
            DocumentDiagnosticReportResult::Partial(report) => report
                .related_documents
                .iter()
                .flat_map(|related| related.values())
                .flat_map(diagnostics_from_report_kind)
                .collect(),
        }
    }
}

/// Runs workspace diagnostics against a server without starting an LSP transport.
///
/// This uses the same stateful server wrapper as the regular transport path,
/// but drives initialization, document opening, and diagnostic requests directly.
///
/// # Examples
///
/// Run a `Server` over a directory without an LSP client:
///
/// ```
/// use async_lsp::lsp_types::{
///     Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport,
///     DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, Position, Range,
///     RelatedFullDocumentDiagnosticReport,
/// };
/// use async_language_server::oneshot::WorkspaceDiagnosticConfig;
/// use async_language_server::server::{
///     DocumentMatcher, Server, ServerResult, ServerState,
/// };
///
/// struct LongLineServer;
///
/// impl Server for LongLineServer {
///     fn server_document_matchers() -> Vec<DocumentMatcher> {
///         vec![DocumentMatcher::new("demo").with_url_globs(["**/*.demo", "*.demo"])]
///     }
///
///     async fn document_diagnostics(
///         &self,
///         state: ServerState,
///         params: DocumentDiagnosticParams,
///     ) -> ServerResult<DocumentDiagnosticReportResult> {
///         let document = state
///             .document(&params.text_document.uri)
///             .expect("document is open");
///         let mut items = Vec::new();
///         for (line, text) in document.text_contents().lines().enumerate() {
///             if text.len() > 20 {
///                 items.push(Diagnostic {
///                     range: Range {
///                         start: Position { line: line as u32, character: 0 },
///                         end: Position { line: line as u32, character: text.len() as u32 },
///                     },
///                     message: format!("line is {} bytes long", text.len()),
///                     ..Diagnostic::default()
///                 });
///             }
///         }
///         Ok(DocumentDiagnosticReportResult::Report(
///             DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
///                 related_documents: None,
///                 full_document_diagnostic_report: FullDocumentDiagnosticReport {
///                     result_id: None,
///                     items,
///                 },
///             }),
///         ))
///     }
/// }
///
/// # fn main() -> Result<(), async_language_server::server::ServerError> {
/// # use async_language_server::oneshot::workspace_diagnostics;
/// # let root = std::env::temp_dir().join("async-language-server-oneshot-doctest");
/// # let _ = std::fs::remove_dir_all(&root);
/// # std::fs::create_dir_all(&root)?;
/// std::fs::write(root.join("sample.demo"), "short\nthis line is much too long\n")?;
///
/// let report =
///     futures::executor::block_on(workspace_diagnostics(LongLineServer, WorkspaceDiagnosticConfig::new(&root)))?;
///
/// assert_eq!(report.documents.len(), 1);
/// assert!(report.documents[0].uri.path().ends_with("sample.demo"));
/// assert_eq!(report.documents[0].diagnostics().len(), 1);
///
/// std::fs::remove_dir_all(root)?;
/// Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if a workspace root cannot be read, a matched document
/// cannot be opened, or the server returns an error from a diagnostic request.
pub async fn workspace_diagnostics<S>(
    server: S,
    config: WorkspaceDiagnosticConfig,
) -> ServerResult<WorkspaceDiagnosticReport>
where
    S: Server + Send + Sync + 'static,
{
    let width = server.server_options().diagnostics_parallelism();
    let walker = WorkspaceWalker::new(&config.roots, config.walk)?;
    let matchers = DocumentMatchers::new(S::server_document_matchers());

    let mut paths = Vec::new();
    for path in walker.files()? {
        if matchers.find_path(&path).is_some() {
            paths.push(path);
        }
    }
    // The engine restores input order, so sorting the paths preserves the
    // report's deterministic path order.
    paths.sort();

    let mut bootstrap = OneshotServer::new(server);
    bootstrap.initialize_workspace(walker.roots()).await?;

    let results: Result<Vec<Option<DocumentDiagnostics>>, ServerError> =
        for_each_bounded(paths, width, |path| {
            let mut server = bootstrap.clone();
            let matchers = &matchers;
            async move {
                let uri = path_to_url(&path)?;
                let Some(matcher) = matchers.find_path(&path) else {
                    return Ok(None);
                };
                let language_id = matcher
                    .lang_strings()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
                // The read + open composite runs as one blocking hop when a
                // tokio runtime is current (`did_open` is synchronous state,
                // so it is pool-safe); plain-executor oneshot runs have no
                // runtime to offload to and run it inline — bounded by the
                // engine's width either way.
                let open = move || -> ServerResult<(OneshotServer<S>, OneshotDocument)> {
                    // arch-lint: allow(no-sync-io) reason="workspace file IO runs on the blocking pool by design"
                    let text = fs::read_to_string(&path)?;
                    let document = OneshotDocument {
                        uri,
                        language_id,
                        version: 1,
                        text,
                    };
                    server.open_document(&document)?;
                    Ok((server, document))
                };
                let (mut server, document) =
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle
                            .spawn_blocking(open)
                            .await
                            .map_err(std::io::Error::from)??
                    } else {
                        open()?
                    };
                let report = server.document_diagnostics(&document).await?;
                Ok(Some(DocumentDiagnostics {
                    uri: document.uri,
                    version: document.version,
                    report,
                }))
            }
        })
        .await;

    let documents = results?.into_iter().flatten().collect();
    Ok(WorkspaceDiagnosticReport { documents })
}

fn diagnostics_from_report_kind(report: &DocumentDiagnosticReportKind) -> &[Diagnostic] {
    match report {
        DocumentDiagnosticReportKind::Full(report) => &report.items,
        DocumentDiagnosticReportKind::Unchanged(_) => &[],
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, num::NonZeroUsize, sync::Arc, time::Duration};

    use async_lsp::lsp_types::{
        Diagnostic, DocumentDiagnosticParams, FullDocumentDiagnosticReport,
        RelatedFullDocumentDiagnosticReport,
    };
    use tokio::sync::{Barrier, mpsc};

    use crate::server::{DocumentMatcher, Server, ServerOptions, ServerResult, ServerState};
    use crate::testing::{diagnostic, temp_workspace};

    use super::{WorkspaceDiagnosticConfig, workspace_diagnostics};

    struct TestServer;

    impl Server for TestServer {
        fn server_document_matchers() -> Vec<DocumentMatcher> {
            vec![
                DocumentMatcher::new("Test")
                    .with_url_globs(["**/*.test", "*.test"])
                    .with_lang_strings(["test"]),
            ]
        }

        // Serialized: this fixture's documents report how many documents
        // the handler observed, which is only deterministic one item at a
        // time. Concurrent scheduling is pinned by `GatedServer` below.
        fn server_options(&self) -> ServerOptions {
            ServerOptions::default()
                .with_diagnostics_parallelism(NonZeroUsize::new(1).expect("constant is nonzero"))
        }

        fn document_diagnostics(
            &self,
            state: ServerState,
            _params: DocumentDiagnosticParams,
        ) -> impl std::future::Future<
            Output = ServerResult<async_lsp::lsp_types::DocumentDiagnosticReportResult>,
        > + Send {
            std::future::ready(Ok(full_report(vec![diagnostic(format!(
                "{} documents",
                state.documents().len()
            ))])))
        }
    }

    #[test]
    fn workspace_diagnostics_discovers_matching_documents() {
        let root = temp_workspace("oneshot", "discovers");
        fs::write(root.join("a.test"), "").expect("test file can be written");
        fs::write(root.join("b.txt"), "").expect("ignored file can be written");
        fs::create_dir_all(root.join("nested")).expect("nested dir can be created");
        fs::write(root.join("nested").join("c.test"), "").expect("nested file can be written");

        let report = futures::executor::block_on(workspace_diagnostics(
            TestServer,
            WorkspaceDiagnosticConfig::new(&root),
        ))
        .expect("workspace diagnostics succeeds");

        assert_eq!(report.documents.len(), 2);
        assert!(
            report
                .documents
                .iter()
                .any(|doc| doc.uri.path().ends_with("/a.test"))
        );
        assert!(
            report
                .documents
                .iter()
                .any(|doc| doc.uri.path().ends_with("/nested/c.test"))
        );

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[test]
    fn workspace_diagnostics_respects_gitignore_by_default() {
        let root = temp_workspace("oneshot", "gitignore");
        fs::create_dir_all(root.join(".git")).expect("git dir can be created");
        fs::write(root.join(".gitignore"), "ignored/\n").expect("gitignore can be written");
        fs::write(root.join("a.test"), "").expect("test file can be written");
        fs::create_dir_all(root.join("ignored")).expect("ignored dir can be created");
        fs::write(root.join("ignored").join("b.test"), "").expect("ignored file can be written");

        let report = futures::executor::block_on(workspace_diagnostics(
            TestServer,
            WorkspaceDiagnosticConfig::new(&root),
        ))
        .expect("workspace diagnostics succeeds");

        assert_eq!(report.documents.len(), 1);
        assert!(report.documents[0].uri.path().ends_with("/a.test"));

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[test]
    fn workspace_diagnostics_can_disable_ignore_files() {
        let root = temp_workspace("oneshot", "ignore-disabled");
        fs::create_dir_all(root.join(".git")).expect("git dir can be created");
        fs::write(root.join(".gitignore"), "ignored/\n").expect("gitignore can be written");
        fs::write(root.join("a.test"), "").expect("test file can be written");
        fs::create_dir_all(root.join("ignored")).expect("ignored dir can be created");
        fs::write(root.join("ignored").join("b.test"), "").expect("ignored file can be written");

        let report = futures::executor::block_on(workspace_diagnostics(
            TestServer,
            WorkspaceDiagnosticConfig::new(&root).with_ignore_files(false),
        ))
        .expect("workspace diagnostics succeeds");

        assert_eq!(report.documents.len(), 2);
        assert!(
            report
                .documents
                .iter()
                .any(|doc| doc.uri.path().ends_with("/ignored/b.test"))
        );

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[test]
    fn workspace_diagnostics_opens_documents_per_item() {
        let root = temp_workspace("oneshot", "opened");
        fs::write(root.join("a.test"), "").expect("test file can be written");
        fs::write(root.join("b.test"), "").expect("test file can be written");

        let report = futures::executor::block_on(workspace_diagnostics(
            TestServer,
            WorkspaceDiagnosticConfig::new(&root),
        ))
        .expect("workspace diagnostics succeeds");

        assert_eq!(report.documents.len(), 2);
        // Serialized by the fixture's width 1, so the handler of the first
        // document (sorted path order) runs while only it is tracked, and
        // the second handler sees both.
        let observed = |suffix: &str| {
            report
                .documents
                .iter()
                .find(|doc| doc.uri.path().ends_with(suffix))
                .expect("document is in the report")
                .diagnostics()[0]
                .message
                .clone()
        };
        assert_eq!(observed("/a.test"), "1 documents");
        assert_eq!(observed("/b.test"), "2 documents");

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    /// Records each handler entry and releases only once all three are in
    /// flight: under any narrower effective width the barrier never
    /// completes and the run hits the test's timeout instead.
    struct GatedServer {
        entries: mpsc::UnboundedSender<()>,
        barrier: Arc<Barrier>,
    }

    impl Server for GatedServer {
        fn server_document_matchers() -> Vec<DocumentMatcher> {
            vec![DocumentMatcher::new("Gated").with_url_globs(["**/*.gated", "*.gated"])]
        }

        fn server_options(&self) -> ServerOptions {
            ServerOptions::default()
                .with_diagnostics_parallelism(NonZeroUsize::new(3).expect("constant is nonzero"))
        }

        async fn document_diagnostics(
            &self,
            _state: ServerState,
            _params: DocumentDiagnosticParams,
        ) -> ServerResult<async_lsp::lsp_types::DocumentDiagnosticReportResult> {
            self.entries.send(()).expect("test channel stays open");
            self.barrier.wait().await;
            Ok(full_report(Vec::new()))
        }
    }

    #[tokio::test]
    async fn workspace_diagnostics_runs_documents_concurrently_up_to_width() {
        let root = temp_workspace("oneshot", "parallel");
        for name in ["a.gated", "b.gated", "c.gated"] {
            fs::write(root.join(name), "").expect("gated file can be written");
        }

        let (entries, mut entry_rx) = mpsc::unbounded_channel();
        let server = GatedServer {
            entries,
            barrier: Arc::new(Barrier::new(3)),
        };

        let report = tokio::time::timeout(
            Duration::from_secs(5),
            workspace_diagnostics(server, WorkspaceDiagnosticConfig::new(&root)),
        )
        .await
        .expect("workspace diagnostics completes - all three documents must run concurrently")
        .expect("workspace diagnostics succeeds");

        let mut entered = 0;
        while entry_rx.try_recv().is_ok() {
            entered += 1;
        }
        assert_eq!(entered, 3);
        assert_eq!(report.documents.len(), 3);

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    fn full_report(items: Vec<Diagnostic>) -> async_lsp::lsp_types::DocumentDiagnosticReportResult {
        async_lsp::lsp_types::DocumentDiagnosticReportResult::Report(
            async_lsp::lsp_types::DocumentDiagnosticReport::Full(
                RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items,
                    },
                },
            ),
        )
    }
}
