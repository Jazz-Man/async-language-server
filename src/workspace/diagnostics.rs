use crate::lsp_requests::Request;
use crate::server::{
    Server, ServerOptions, ServerState, WorkspaceDiagnostics, WorkspaceDiagnosticsSetting,
};
use crate::workspace::for_each_bounded;
use async_lsp::lsp_types::request::{
    RegisterCapability, WorkspaceConfiguration, WorkspaceDiagnosticRefresh,
};
use async_lsp::lsp_types::{
    ClientCapabilities, ConfigurationParams, DiagnosticServerCapabilities,
    DidChangeWatchedFilesRegistrationOptions, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportKind, DocumentDiagnosticReportResult, FileSystemWatcher,
    FullDocumentDiagnosticReport, InitializeResult, LSPAny, OneOf, PartialResultParams,
    Registration, RegistrationParams, TextDocumentIdentifier, Url, WatchKind,
    WorkDoneProgressParams, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceDiagnosticReportResult, WorkspaceDocumentDiagnosticReport,
    WorkspaceFoldersServerCapabilities, WorkspaceFullDocumentDiagnosticReport,
    WorkspaceServerCapabilities, WorkspaceUnchangedDocumentDiagnosticReport,
};
use async_lsp::{ErrorCode, ResponseError, Result};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceDiagnosticsState {
    inner: Arc<WorkspaceDiagnosticsStateInner>,
}

#[derive(Debug)]
struct WorkspaceDiagnosticsStateInner {
    options: WorkspaceDiagnostics,
    supported: AtomicBool,
    enabled: AtomicBool,
    client_configuration: AtomicBool,
    client_dynamic_configuration: AtomicBool,
    client_refresh: AtomicBool,
    generation: AtomicU64,
}

impl WorkspaceDiagnosticsState {
    pub(crate) fn new(options: &ServerOptions) -> Self {
        let enabled = match &options.workspace_diagnostics {
            WorkspaceDiagnostics::Disabled => false,
            WorkspaceDiagnostics::Enabled => true,
            WorkspaceDiagnostics::Configurable(setting) => setting.default_enabled,
        };

        Self {
            inner: Arc::new(WorkspaceDiagnosticsStateInner {
                options: options.workspace_diagnostics.clone(),
                supported: AtomicBool::new(false),
                enabled: AtomicBool::new(enabled),
                client_configuration: AtomicBool::new(false),
                client_dynamic_configuration: AtomicBool::new(false),
                client_refresh: AtomicBool::new(false),
                generation: AtomicU64::new(0),
            }),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.supported_and(&self.inner.enabled)
    }

    pub(crate) fn supported(&self) -> bool {
        self.inner.supported.load(Ordering::Relaxed)
    }

    fn setting(&self) -> Option<&WorkspaceDiagnosticsSetting> {
        if let WorkspaceDiagnostics::Configurable(setting) = &self.inner.options {
            Some(setting)
        } else {
            None
        }
    }

    /// The shared machinery conjunct: nothing wakes unless the final
    /// advertisement said supported.
    fn supported_and(&self, flag: &AtomicBool) -> bool {
        self.supported() && flag.load(Ordering::Relaxed)
    }

    /// The configuration machinery's gate: one of the two client capability
    /// flags under the shared conjunct, plus the `Configurable`-only setting.
    fn configuration_gate(&self, flag: &AtomicBool) -> bool {
        self.supported_and(flag) && self.setting().is_some()
    }

    fn can_request_configuration(&self) -> bool {
        self.configuration_gate(&self.inner.client_configuration)
    }

    fn can_register_configuration(&self) -> bool {
        self.configuration_gate(&self.inner.client_dynamic_configuration)
    }

    fn can_refresh(&self) -> bool {
        self.supported_and(&self.inner.client_refresh)
    }

    /// Records the post-merge advertisement's verdict (`supported`, decided
    /// by the caller from the final `InitializeResult`) and the three client
    /// capability flags the configuration machinery needs.
    fn configure(&self, client_capabilities: &ClientCapabilities, supported: bool) {
        self.inner.supported.store(supported, Ordering::Relaxed);

        let workspace = client_capabilities.workspace.as_ref();
        self.inner.client_configuration.store(
            workspace.and_then(|w| w.configuration).unwrap_or(false),
            Ordering::Relaxed,
        );
        self.inner.client_dynamic_configuration.store(
            workspace
                .and_then(|config| config.did_change_configuration.as_ref())
                .and_then(|did_change_configuration| did_change_configuration.dynamic_registration)
                .unwrap_or(false),
            Ordering::Relaxed,
        );
        self.inner.client_refresh.store(
            workspace
                .and_then(|diag| diag.diagnostic.as_ref())
                .and_then(|diagnostic| diagnostic.refresh_support)
                .unwrap_or(false),
            Ordering::Relaxed,
        );
    }

    fn next_generation(&self) -> u64 {
        self.inner.generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn current_generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Relaxed)
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> bool {
        self.inner.enabled.swap(enabled, Ordering::Relaxed) != enabled
    }
}

pub(crate) fn configure_capabilities(
    state: &ServerState,
    result: &mut InitializeResult,
    client_capabilities: &ClientCapabilities,
) {
    let workspace_diagnostics = state.workspace_diagnostics();

    // The one deliberate override: the kill-switch forces the advertisement
    // off no matter what the implementor declared. Everything else is the
    // implementor's value, verbatim.
    if matches!(
        &workspace_diagnostics.inner.options,
        WorkspaceDiagnostics::Disabled,
    ) {
        set_workspace_diagnostics_advertised(result, false);
    }

    let supported = advertised_workspace_diagnostics(result);
    workspace_diagnostics.configure(client_capabilities, supported);

    if supported {
        enable_workspace_folder_tracking(result);
    }

    let watching = client_capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched| watched.dynamic_registration)
        .unwrap_or(false);
    state.set_file_watching(watching);
}

/// Reads the provider's advertised `workspace_diagnostics` flag; no provider
/// advertises nothing.
fn advertised_workspace_diagnostics(result: &InitializeResult) -> bool {
    result
        .capabilities
        .diagnostic_provider
        .as_ref()
        .is_some_and(|provider| match provider {
            DiagnosticServerCapabilities::Options(options) => options.workspace_diagnostics,
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                options.diagnostic_options.workspace_diagnostics
            }
        })
}

fn set_workspace_diagnostics_advertised(result: &mut InitializeResult, advertised: bool) {
    if let Some(provider) = result.capabilities.diagnostic_provider.as_mut() {
        match provider {
            DiagnosticServerCapabilities::Options(options) => {
                options.workspace_diagnostics = advertised;
            }
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                options.diagnostic_options.workspace_diagnostics = advertised;
            }
        }
    }
}

fn enable_workspace_folder_tracking(result: &mut InitializeResult) {
    // unreachable through configure_capabilities today — supported implies a provider; kept as this function's own invariant
    if result.capabilities.diagnostic_provider.is_none() {
        return;
    }

    let workspace = result
        .capabilities
        .workspace
        .get_or_insert_with(WorkspaceServerCapabilities::default);
    let folders = workspace
        .workspace_folders
        .get_or_insert_with(WorkspaceFoldersServerCapabilities::default);

    folders.supported = Some(true);
    if !matches!(folders.change_notifications, Some(OneOf::Right(_))) {
        folders.change_notifications = Some(OneOf::Left(true));
    }
}

pub(crate) fn apply_initialization_options(state: &ServerState, options: Option<&LSPAny>) {
    let Some(options) = options else {
        return;
    };
    let workspace_diagnostics = state.workspace_diagnostics();
    let Some(setting) = workspace_diagnostics.setting() else {
        return;
    };
    let Some(enabled) = setting.key.value(options) else {
        return;
    };

    state.set_workspace_diagnostics_enabled(enabled);
}

pub(crate) fn initialized(state: ServerState) {
    register_watchers(state.clone());
    register_configuration(state.clone());
    request_configuration(state);
}

pub(crate) fn did_change_configuration(state: ServerState, settings: &LSPAny) {
    let workspace_diagnostics = state.workspace_diagnostics();
    let Some(setting) = workspace_diagnostics.setting() else {
        return;
    };

    if let Some(enabled) = setting.key.value(settings) {
        workspace_diagnostics.next_generation();
        apply_enabled(state, enabled);
    } else {
        request_configuration(state);
    }
}

pub(crate) async fn workspace_diagnostic<T>(
    server: Arc<T>,
    state: ServerState,
    params: WorkspaceDiagnosticParams,
) -> Result<WorkspaceDiagnosticReportResult, ResponseError>
where
    T: Server + Send + Sync + 'static,
{
    if !state.workspace_diagnostics().supported() {
        return Err(ResponseError::new(
            ErrorCode::METHOD_NOT_FOUND,
            "workspace diagnostics are disabled",
        ));
    }

    if !state.workspace_diagnostics().enabled() {
        return Ok(WorkspaceDiagnosticReportResult::Report(
            disabled_workspace_diagnostic_report(&state, params),
        ));
    }

    let items = workspace_diagnostic_items(server, state, params).await?;
    Ok(WorkspaceDiagnosticReportResult::Report(
        WorkspaceDiagnosticReport { items },
    ))
}

fn register_configuration(state: ServerState) {
    let workspace_diagnostics = state.workspace_diagnostics();
    if !workspace_diagnostics.can_register_configuration() {
        return;
    }
    let Some(setting) = workspace_diagnostics.setting().cloned() else {
        return;
    };

    spawn(async move {
        let result = state
            .client()
            .request::<RegisterCapability>(RegistrationParams {
                registrations: vec![Registration {
                    id: "async-language-server.workspaceDiagnostics.configuration".into(),
                    method: "workspace/didChangeConfiguration".into(),
                    register_options: Some(serde_json::json!({
                        "section": setting.key.section(),
                    })),
                }],
            })
            .await;
        if let Err(error) = &result {
            tracing::warn!("workspace diagnostics capability registration failed: {error}");
        }
    });
}

/// The watcher registration id: matchers are session-fixed, so the
/// registration is never re-negotiated mid-session.
const WATCHED_FILES_REGISTRATION_ID: &str = "async-language-server.watchedFiles";

/// All three kinds, explicitly: Create/Delete drive the walk cache's
/// invalidation, Change drives the existing eager tracked-doc refresh.
const WATCH_KIND_ALL: WatchKind = WatchKind::Create
    .union(WatchKind::Change)
    .union(WatchKind::Delete);

fn register_watchers(state: ServerState) {
    // The gated triple: the client must support dynamic watching, the
    // consuming feature must be on (watched-file events only carry meaning
    // for Workspace-origin documents, which exist solely under enabled
    // diagnostics), and the registration must not repeat. A later enable
    // re-enters here through `apply_enabled`; disable never unregisters —
    // the events it would carry are no-ops by then.
    if !state.file_watching()
        || !state.workspace_diagnostics().enabled()
        || state.watchers_registered()
    {
        return;
    }
    let globs = state.watcher_globs();
    if globs.is_empty() {
        return;
    }

    // The flag is not reserved here: it is set only when the client accepts
    // the registration (the spawn's success arm), so a failure leaves it
    // false and the next enable transition retries. Two entries can race
    // past this gate before either registers — `initialized` against
    // `apply_enabled`'s re-entry — and both send; benign, the requests
    // carry the same fixed id and a client replaces a registration by id.
    spawn(async move {
        // Static typed options cannot fail to serialize; a failure would be
        // an lsp_types bug. It is traced, not swallowed: the flag stays
        // false, so every later enable transition retries and re-announces.
        let options = match serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
            watchers: globs
                .into_iter()
                .map(|glob| FileSystemWatcher {
                    glob_pattern: glob.into(),
                    kind: Some(WATCH_KIND_ALL),
                })
                .collect(),
        }) {
            Ok(options) => options,
            Err(error) => {
                tracing::warn!("registration options failed to serialize: {error}");
                return;
            }
        };
        let result = state
            .client()
            .request::<RegisterCapability>(RegistrationParams {
                registrations: vec![Registration {
                    id: WATCHED_FILES_REGISTRATION_ID.into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(options),
                }],
            })
            .await;
        match result {
            // "Registered" means accepted: only a success marks the state,
            // keeping the flag a truthful input for the walk-cache gate.
            Ok(()) => state.set_watchers_registered(true),
            Err(error) => {
                tracing::warn!("file watching registration failed: {error}");
            }
        }
    });
}

fn request_configuration(state: ServerState) {
    let workspace_diagnostics = state.workspace_diagnostics();
    if !workspace_diagnostics.can_request_configuration() {
        return;
    }
    let Some(setting) = workspace_diagnostics.setting().cloned() else {
        return;
    };
    let generation = workspace_diagnostics.next_generation();

    spawn(async move {
        let response = state
            .client()
            .request::<WorkspaceConfiguration>(ConfigurationParams {
                items: vec![setting.key.item()],
            })
            .await;
        let Ok(response) = response else {
            return;
        };
        if workspace_diagnostics.current_generation() != generation {
            return;
        }
        let Some(value) = response.first() else {
            return;
        };
        let Some(enabled) = setting.key.value(value) else {
            return;
        };

        apply_enabled(state, enabled);
    });
}

fn apply_enabled(state: ServerState, enabled: bool) {
    let changed = state.set_workspace_diagnostics_enabled(enabled);
    if changed && enabled {
        state.walk_cache().invalidate();
        // Idempotent via the `watchers_registered` flag: a disable does not
        // unregister — matchers are session-fixed — so a re-enable must not
        // double-register.
        register_watchers(state.clone());
    }
    if changed && !enabled {
        state.walk_cache().clear();
    }
    if changed && state.workspace_diagnostics().supported() {
        refresh_diagnostics(state);
    }
}

fn refresh_diagnostics(state: ServerState) {
    if !state.workspace_diagnostics().can_refresh() {
        return;
    }

    spawn(async move {
        let result = state
            .client()
            .request::<WorkspaceDiagnosticRefresh>(())
            .await;
        if let Err(error) = &result {
            tracing::warn!("workspace diagnostic refresh request failed: {error}");
        }
    });
}

fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    }
}

fn disabled_workspace_diagnostic_report(
    state: &ServerState,
    params: WorkspaceDiagnosticParams,
) -> WorkspaceDiagnosticReport {
    let items = params
        .previous_result_ids
        .into_iter()
        .map(|previous| {
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                version: state.document_workspace_version(&previous.uri),
                uri: previous.uri,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            })
        })
        .collect();

    WorkspaceDiagnosticReport { items }
}

async fn workspace_diagnostic_items<T>(
    server: Arc<T>,
    state: ServerState,
    params: WorkspaceDiagnosticParams,
) -> Result<Vec<WorkspaceDocumentDiagnosticReport>, ResponseError>
where
    T: Server + Send + Sync + 'static,
{
    let identifier = params.identifier;
    let previous_result_ids: HashMap<_, _> = params
        .previous_result_ids
        .into_iter()
        .map(|id| (id.uri, id.value))
        .collect();
    let urls = state
        .refresh_workspace_documents()
        .await
        .map_err(ResponseError::from)?;

    let width = state.diagnostics_parallelism();
    let state_for_items = state.clone();
    let item_sinks = for_each_bounded(urls, width, move |url| {
        let server = Arc::clone(&server);
        let state = state_for_items.clone();
        let identifier = identifier.clone();
        let previous = previous_result_ids.get(&url).cloned();
        async move {
            let Some(doc) = state.document(&url) else {
                return Ok(WorkspaceReportSink::default());
            };
            let version = doc.version();
            // A default error on an advertised method draws its single
            // warning, the same guard the dispatch engines run.
            let mut result = match server
                .document_diagnostics(
                    state.clone(),
                    document_diagnostic_params(url.clone(), identifier, previous),
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    state.warn_once_default("document_diagnostics", &error);
                    return Err(error.into());
                }
            };

            if state
                .document_version(&url)
                .is_some_and(|current| current != version)
            {
                return Err(ResponseError::new(
                    ErrorCode::CONTENT_MODIFIED,
                    "document was modified during processing",
                ));
            }

            <crate::lsp_requests::DocumentDiagnosticsRequest as Request>::modify_response(
                &state,
                &doc,
                &mut result,
            );
            let mut sink = WorkspaceReportSink::default();
            push_workspace_reports_from_document_result(&state, url, result, &mut sink);
            Ok(sink)
        }
    })
    .await?;

    // The engine restored input order, so folding the per-item sinks in
    // sequence replays the serial loop's push order — every report merges
    // with the `replace` flag it was pushed with.
    let mut sink = WorkspaceReportSink::default();
    for item_sink in item_sinks {
        for (report, replace) in item_sink.reports {
            push_workspace_report(&mut sink, report, replace);
        }
    }

    let mut reports: Vec<_> = sink.reports.into_iter().map(|(report, _)| report).collect();
    reports.sort_by(|a, b| {
        workspace_report_uri(a)
            .as_str()
            .cmp(workspace_report_uri(b).as_str())
    });
    Ok(reports)
}

fn document_diagnostic_params(
    uri: Url,
    identifier: Option<String>,
    previous_result_id: Option<String>,
) -> DocumentDiagnosticParams {
    DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier::new(uri),
        identifier,
        previous_result_id,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn push_workspace_reports_from_document_result(
    state: &ServerState,
    uri: Url,
    result: DocumentDiagnosticReportResult,
    sink: &mut WorkspaceReportSink,
) {
    match result {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) => {
            push_workspace_report(
                sink,
                WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                    version: state.document_workspace_version(&uri),
                    uri,
                    full_document_diagnostic_report: report.full_document_diagnostic_report,
                }),
                true,
            );
            push_related_reports(state, report.related_documents, sink);
        }
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(report)) => {
            push_workspace_report(
                sink,
                WorkspaceDocumentDiagnosticReport::Unchanged(
                    WorkspaceUnchangedDocumentDiagnosticReport {
                        version: state.document_workspace_version(&uri),
                        uri,
                        unchanged_document_diagnostic_report: report
                            .unchanged_document_diagnostic_report,
                    },
                ),
                true,
            );
            push_related_reports(state, report.related_documents, sink);
        }
        DocumentDiagnosticReportResult::Partial(report) => {
            push_related_reports(state, report.related_documents, sink);
        }
    }
}

/// Ordered report accumulator with an O(1) URI index: pushes replace or
/// append by URI in constant time; the final `Vec` order is the insertion
/// order (the caller's final sort normalizes output). Each report carries
/// the `replace` flag it was pushed with, so per-item sinks fold into a
/// shared sink without losing a flag.
#[derive(Default)]
struct WorkspaceReportSink {
    reports: Vec<(WorkspaceDocumentDiagnosticReport, bool)>,
    index: HashMap<Url, usize>,
}

fn push_workspace_report(
    sink: &mut WorkspaceReportSink,
    report: WorkspaceDocumentDiagnosticReport,
    replace: bool,
) {
    let uri = workspace_report_uri(&report).clone();
    if let Some(&position) = sink.index.get(&uri) {
        if replace {
            sink.reports[position] = (report, replace);
        }
    } else {
        sink.index.insert(uri, sink.reports.len());
        sink.reports.push((report, replace));
    }
}

fn workspace_report_uri(report: &WorkspaceDocumentDiagnosticReport) -> &Url {
    match report {
        WorkspaceDocumentDiagnosticReport::Full(report) => &report.uri,
        WorkspaceDocumentDiagnosticReport::Unchanged(report) => &report.uri,
    }
}

fn push_related_reports(
    state: &ServerState,
    related_documents: Option<HashMap<Url, DocumentDiagnosticReportKind>>,
    sink: &mut WorkspaceReportSink,
) {
    let Some(related_documents) = related_documents else {
        return;
    };

    for (uri, report) in related_documents {
        match report {
            DocumentDiagnosticReportKind::Full(report) => {
                push_workspace_report(
                    sink,
                    WorkspaceDocumentDiagnosticReport::Full(
                        WorkspaceFullDocumentDiagnosticReport {
                            version: state.document_workspace_version(&uri),
                            uri,
                            full_document_diagnostic_report: report,
                        },
                    ),
                    false,
                );
            }
            DocumentDiagnosticReportKind::Unchanged(report) => {
                push_workspace_report(
                    sink,
                    WorkspaceDocumentDiagnosticReport::Unchanged(
                        WorkspaceUnchangedDocumentDiagnosticReport {
                            version: state.document_workspace_version(&uri),
                            uri,
                            unchanged_document_diagnostic_report: report,
                        },
                    ),
                    false,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientCapabilities, DiagnosticServerCapabilities, DocumentDiagnosticReportKind,
        FullDocumentDiagnosticReport, HashMap, InitializeResult, Url, WorkspaceDiagnosticsState,
        WorkspaceDocumentDiagnosticReport, WorkspaceFullDocumentDiagnosticReport,
        WorkspaceReportSink, WorkspaceUnchangedDocumentDiagnosticReport, configure_capabilities,
        push_related_reports, push_workspace_report, workspace_diagnostic_items,
    };
    use crate::error::ServerResult;
    use crate::server::{
        DocumentMatcher, Server, ServerOptions, ServerState, WorkspaceDiagnostics,
    };
    use crate::testing::{
        TempWorkspace, diagnostic_provider_capabilities, workspace, workspace_diagnostic_params,
        workspace_folder,
    };
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{
        DiagnosticOptions, DiagnosticRegistrationOptions, DiagnosticWorkspaceClientCapabilities,
        DidChangeConfigurationClientCapabilities, DidChangeWatchedFilesClientCapabilities,
        DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
        RelatedFullDocumentDiagnosticReport, ServerCapabilities, UnchangedDocumentDiagnosticReport,
        WorkspaceClientCapabilities,
    };
    use rstest::rstest;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Semaphore, mpsc};

    const ENTRY_TIMEOUT: Duration = Duration::from_secs(5);
    const ABSENCE_TIMEOUT: Duration = Duration::from_millis(250);

    #[rstest]
    fn push_workspace_report_replaces_by_uri_and_appends_new() {
        let mut sink = WorkspaceReportSink::default();
        let uri = crate::testing::url("file:///tmp/a.txt");
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Unchanged(
                WorkspaceUnchangedDocumentDiagnosticReport {
                    version: None,
                    uri: uri.clone(),
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: String::new(),
                    },
                },
            ),
            false,
        );
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                version: None,
                uri: uri.clone(),
                full_document_diagnostic_report: FullDocumentDiagnosticReport::default(),
            }),
            true,
        );
        let other = crate::testing::url("file:///tmp/b.txt");
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                version: None,
                uri: other,
                full_document_diagnostic_report: FullDocumentDiagnosticReport::default(),
            }),
            false,
        );
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                version: None,
                uri: uri.clone(),
                full_document_diagnostic_report: FullDocumentDiagnosticReport::default(),
            }),
            false,
        );
        // replace=true overwrote the first URI; the new URI appended; the
        // `replace=false` pushes left one entry per URI — the last one,
        // over the already-present `uri`, changed nothing at all.
        assert_eq!(sink.reports.len(), 2);
    }

    struct GatedDiagnosticsServer {
        entered: mpsc::UnboundedSender<Url>,
        gate: Arc<Semaphore>,
    }

    impl Server for GatedDiagnosticsServer {
        fn server_document_matchers() -> Vec<DocumentMatcher> {
            crate::testing::extension_matchers("Gated", "diag")
        }

        async fn document_diagnostics(
            &self,
            _state: ServerState,
            params: DocumentDiagnosticParams,
        ) -> ServerResult<DocumentDiagnosticReportResult> {
            self.entered
                .send(params.text_document.uri.clone())
                .expect("entry channel open");
            self.gate.acquire().await.expect("gate open").forget();
            Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: Some(params.text_document.uri.to_string()),
                        items: Vec::new(),
                    },
                }),
            ))
        }
    }

    fn gated_setup(
        width: NonZeroUsize,
        workspace: &TempWorkspace,
    ) -> (
        ServerState,
        mpsc::UnboundedSender<Url>,
        mpsc::UnboundedReceiver<Url>,
        Arc<Semaphore>,
    ) {
        for file in ["one", "two", "three"] {
            workspace.write(format!("{file}.diag"), &format!("{file} diagnostics\n"));
        }
        let options = ServerOptions::default()
            .with_workspace_diagnostics(WorkspaceDiagnostics::Enabled)
            .with_diagnostics_parallelism(width);
        let state = ServerState::with_options::<GatedDiagnosticsServer>(
            ClientSocket::new_closed(),
            &options,
        );
        state.set_workspace_folders([workspace_folder(workspace)]);
        // The width engine runs post-initialize: mark the capability as
        // advertised, the way `initialize` would, so the workspace refresh
        // underneath the items request is enabled.
        let mut result = result_with_provider(true);
        configure_capabilities(&state, &mut result, &ClientCapabilities::default());
        let (entered_tx, entered_rx) = mpsc::unbounded_channel();
        (state, entered_tx, entered_rx, Arc::new(Semaphore::new(0)))
    }

    #[rstest]
    #[tokio::test]
    async fn width_three_documents_enter_before_any_releases(
        #[with("diagnostics")] workspace: TempWorkspace,
    ) {
        let (state, entered_tx, mut entered_rx, gate) =
            gated_setup(NonZeroUsize::new(3).expect("nonzero"), &workspace);
        let server = Arc::new(GatedDiagnosticsServer {
            entered: entered_tx,
            gate: Arc::clone(&gate),
        });
        let items = tokio::spawn(workspace_diagnostic_items(
            server,
            state,
            workspace_diagnostic_params(),
        ));

        // All three handlers must enter while the gate is closed — the
        // gate holds zero permits, so nothing has released yet: width 3
        // runs the whole cohort at once.
        let mut entered = Vec::new();
        for _ in 0..3 {
            let url = tokio::time::timeout(ENTRY_TIMEOUT, entered_rx.recv())
                .await
                .expect("handler enters before the timeout")
                .expect("channel open");
            entered.push(url);
        }
        assert_eq!(entered.len(), 3);
        assert!(
            tokio::time::timeout(ABSENCE_TIMEOUT, entered_rx.recv())
                .await
                .is_err(),
            "no fourth document exists to enter",
        );

        gate.add_permits(3);
        let items = tokio::time::timeout(ENTRY_TIMEOUT, items)
            .await
            .expect("task joins before the timeout")
            .expect("task succeeds")
            .expect("diagnostics succeed");
        assert_eq!(items.len(), 3);
    }

    #[rstest]
    #[tokio::test]
    async fn width_one_runs_documents_one_at_a_time(
        #[with("diagnostics")] workspace: TempWorkspace,
    ) {
        let (state, entered_tx, mut entered_rx, gate) =
            gated_setup(NonZeroUsize::new(1).expect("nonzero"), &workspace);
        let server = Arc::new(GatedDiagnosticsServer {
            entered: entered_tx,
            gate: Arc::clone(&gate),
        });
        let items = tokio::spawn(workspace_diagnostic_items(
            server,
            state,
            workspace_diagnostic_params(),
        ));

        let first = tokio::time::timeout(ENTRY_TIMEOUT, entered_rx.recv())
            .await
            .expect("first handler enters")
            .expect("channel open");
        // The only width slot is held by the parked first handler: no
        // second handler may enter until the test opens the gate.
        assert!(
            tokio::time::timeout(ABSENCE_TIMEOUT, entered_rx.recv())
                .await
                .is_err(),
            "width 1 must serialize handlers",
        );

        gate.add_permits(1);
        let second = tokio::time::timeout(ENTRY_TIMEOUT, entered_rx.recv())
            .await
            .expect("second handler enters after the release")
            .expect("channel open");
        assert_ne!(first, second);

        gate.add_permits(2);
        let items = tokio::time::timeout(ENTRY_TIMEOUT, items)
            .await
            .expect("task joins before the timeout")
            .expect("task succeeds")
            .expect("diagnostics succeed");
        assert_eq!(items.len(), 3);
    }

    struct PlainServer;

    impl Server for PlainServer {}

    struct ProviderServer;

    impl Server for ProviderServer {
        fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
            Some(diagnostic_provider_capabilities(true, false))
        }
    }

    fn configurable_state() -> WorkspaceDiagnosticsState {
        let options = ServerOptions::default()
            .with_workspace_diagnostics(WorkspaceDiagnostics::setting("gated"));
        WorkspaceDiagnosticsState::new(&options)
    }

    /// Builds the client capabilities `configure` reads its three flags
    /// from, the way an initializing client advertises them.
    fn client_caps(configuration: bool, dynamic: bool, refresh: bool) -> ClientCapabilities {
        ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                configuration: Some(configuration),
                did_change_configuration: Some(DidChangeConfigurationClientCapabilities {
                    dynamic_registration: Some(dynamic),
                }),
                diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
                    refresh_support: Some(refresh),
                }),
                ..WorkspaceClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        }
    }

    #[rstest]
    fn request_configuration_requires_client_capability_and_setting() {
        let state = configurable_state();

        state.configure(&client_caps(false, false, false), true);
        assert!(
            !state.can_request_configuration(),
            "no interrogation without the client's configuration support",
        );

        state.configure(&client_caps(true, false, false), true);
        assert!(
            state.can_request_configuration(),
            "the capability plus a Configurable setting enables the request",
        );
    }

    #[rstest]
    fn register_configuration_requires_dynamic_registration_support() {
        let state = configurable_state();

        state.configure(&client_caps(false, true, false), true);
        assert!(
            state.can_register_configuration(),
            "dynamic registration plus a Configurable setting enables the registration",
        );
    }

    #[rstest]
    fn refresh_gate_tracks_client_refresh_support(#[values(false, true)] refresh_support: bool) {
        let state = configurable_state();

        state.configure(&client_caps(false, false, refresh_support), true);
        assert_eq!(
            state.can_refresh(),
            refresh_support,
            "refresh_support = {refresh_support} must gate the refresh request",
        );
    }

    #[rstest]
    fn next_generation_is_monotonic() {
        let state = WorkspaceDiagnosticsState::new(&ServerOptions::default());

        assert_eq!(state.next_generation(), 1);
        assert_eq!(state.next_generation(), 2);
    }

    #[rstest]
    fn stale_generation_drops_the_response() {
        let state = WorkspaceDiagnosticsState::new(&ServerOptions::default());

        let captured = state.next_generation();
        assert_eq!(
            state.current_generation(),
            captured,
            "a fresh reply's generation equals the current one",
        );

        let superseding = state.next_generation();
        assert_ne!(
            state.current_generation(),
            captured,
            "advancing the counter makes the captured generation stale",
        );
        assert_eq!(state.current_generation(), superseding);
    }

    #[rstest]
    fn disabled_options_force_workspace_diagnostics_capability_off() {
        let state = ServerState::with_options::<ProviderServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::disabled()),
        );
        let mut result = InitializeResult {
            server_info: None,
            capabilities: ProviderServer::server_capabilities(ClientCapabilities::default())
                .expect("the provider server advertises capabilities"),
        };

        configure_capabilities(&state, &mut result, &ClientCapabilities::default());

        let Some(DiagnosticServerCapabilities::Options(options)) =
            result.capabilities.diagnostic_provider
        else {
            panic!("expected diagnostic options");
        };
        assert!(
            !options.workspace_diagnostics,
            "Disabled options must clear the advertised workspace diagnostics flag",
        );
    }

    fn matrix_state(options: WorkspaceDiagnostics) -> ServerState {
        #[derive(Default)]
        struct MatrixServer;
        impl Server for MatrixServer {}

        let options = ServerOptions::default().with_workspace_diagnostics(options);
        ServerState::with_options::<MatrixServer>(ClientSocket::new_closed(), &options)
    }

    fn result_with_provider(workspace_diagnostics: bool) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        workspace_diagnostics,
                        ..DiagnosticOptions::default()
                    },
                )),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        }
    }

    fn result_with_registration_provider(workspace_diagnostics: bool) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                diagnostic_provider: Some(DiagnosticServerCapabilities::RegistrationOptions(
                    DiagnosticRegistrationOptions {
                        diagnostic_options: DiagnosticOptions {
                            workspace_diagnostics,
                            ..DiagnosticOptions::default()
                        },
                        ..DiagnosticRegistrationOptions::default()
                    },
                )),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        }
    }

    /// Reads the provider's advertised flag from either provider shape — the
    /// test-side mirror of `advertised_workspace_diagnostics`, so both arms
    /// assert the same way.
    fn advertised_flag(provider: &DiagnosticServerCapabilities) -> bool {
        match provider {
            DiagnosticServerCapabilities::Options(options) => options.workspace_diagnostics,
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                options.diagnostic_options.workspace_diagnostics
            }
        }
    }

    // A fresh state has never spoken to a client: `supported` starts false
    // even under Enabled options — only `configure_capabilities` records the
    // post-merge advertisement. Catches a regression re-deriving `supported`
    // from `ServerOptions` in `WorkspaceDiagnosticsState::new`, which every
    // post-configure assertion would otherwise let pass.
    #[rstest]
    fn supported_starts_false_before_configure_capabilities() {
        let state = matrix_state(WorkspaceDiagnostics::enabled());
        assert!(
            !state.workspace_diagnostics().supported(),
            "a fresh state is unsupported until configure_capabilities has spoken",
        );
    }

    // The spec's resolution matrix, one row per case: (ServerOptions mode,
    // advertised flag in the implementor's provider) => expected final
    // advertisement and handler support.
    #[rstest]
    #[case::enabled_advertises(WorkspaceDiagnostics::enabled(), true, true, true)]
    #[case::enabled_silent(WorkspaceDiagnostics::enabled(), false, false, false)]
    #[case::disabled_forces_off(WorkspaceDiagnostics::disabled(), true, false, false)]
    #[case::disabled_silent(WorkspaceDiagnostics::disabled(), false, false, false)]
    #[case::configurable_advertises(
        WorkspaceDiagnostics::Configurable(WorkspaceDiagnostics::setting("test.matrix")),
        true,
        true,
        true
    )]
    #[case::configurable_silent(
        WorkspaceDiagnostics::Configurable(WorkspaceDiagnostics::setting("test.matrix")),
        false,
        false,
        false
    )]
    fn resolution_matrix_advertises_verbatim_and_gates_support(
        #[case] mode: WorkspaceDiagnostics,
        #[case] implementor_flag: bool,
        #[case] advertised: bool,
        #[case] supported: bool,
    ) {
        let state = matrix_state(mode);
        let mut result = result_with_provider(implementor_flag);
        configure_capabilities(&state, &mut result, &ClientCapabilities::default());

        let provider = result
            .capabilities
            .diagnostic_provider
            .as_ref()
            .expect("provider survives the merge");
        assert_eq!(
            advertised_flag(provider),
            advertised,
            "the final advertisement",
        );
        assert_eq!(
            state.workspace_diagnostics().supported(),
            supported,
            "handler support",
        );
    }

    // The RegistrationOptions arm follows the same matrix; the Disabled row
    // is the kill-switch cell where an arm-specific regression would hide.
    #[rstest]
    #[case::enabled_advertises(WorkspaceDiagnostics::enabled(), true)]
    #[case::disabled_forces_off(WorkspaceDiagnostics::disabled(), false)]
    fn registration_options_arm_follows_the_same_matrix(
        #[case] mode: WorkspaceDiagnostics,
        #[case] expected: bool,
    ) {
        let state = matrix_state(mode);
        let mut result = result_with_registration_provider(true);
        configure_capabilities(&state, &mut result, &ClientCapabilities::default());
        let Some(DiagnosticServerCapabilities::RegistrationOptions(options)) =
            result.capabilities.diagnostic_provider.as_ref()
        else {
            panic!("provider survives the merge");
        };
        assert_eq!(
            options.diagnostic_options.workspace_diagnostics, expected,
            "the RegistrationOptions arm carries the resolved advertisement",
        );
        assert_eq!(
            state.workspace_diagnostics().supported(),
            expected,
            "handler support follows the advertisement",
        );
    }

    // A provider-less implementor stays provider-less: the framework creates
    // nothing, and the handler stays unsupported.
    #[rstest]
    #[case::enabled(WorkspaceDiagnostics::enabled())]
    #[case::disabled(WorkspaceDiagnostics::disabled())]
    fn provider_none_advertises_nothing_and_stays_unsupported(#[case] mode: WorkspaceDiagnostics) {
        let state = matrix_state(mode);
        let mut result = InitializeResult::default();
        configure_capabilities(&state, &mut result, &ClientCapabilities::default());

        assert!(result.capabilities.diagnostic_provider.is_none());
        assert!(!state.workspace_diagnostics().supported());
    }

    // The spec's machinery rule: registration, configuration polling, and
    // refresh only activate when the final advertisement said supported —
    // client capabilities alone must not wake them.
    #[rstest]
    fn machinery_gates_on_supported(#[values(true, false)] advertised: bool) {
        let mode = WorkspaceDiagnostics::Configurable(
            WorkspaceDiagnostics::setting("test.machinery").with_default_enabled(true),
        );

        let server_state = matrix_state(mode);
        let mut result = result_with_provider(advertised);
        configure_capabilities(&server_state, &mut result, &client_caps(true, true, true));
        let state = server_state.workspace_diagnostics();
        assert_eq!(state.supported(), advertised, "supported");
        assert_eq!(
            state.can_request_configuration(),
            advertised,
            "configuration polling",
        );
        assert_eq!(
            state.can_register_configuration(),
            advertised,
            "dynamic registration",
        );
        assert_eq!(state.can_refresh(), advertised, "refresh");
    }

    #[rstest]
    fn file_watching_follows_the_client_capability() {
        let state = matrix_state(WorkspaceDiagnostics::enabled());
        let mut result = result_with_provider(true);

        let watching = ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                    dynamic_registration: Some(true),
                    ..DidChangeWatchedFilesClientCapabilities::default()
                }),
                ..WorkspaceClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        };
        configure_capabilities(&state, &mut result, &watching);
        assert!(
            state.file_watching(),
            "dynamic registration support captures as file watching",
        );

        configure_capabilities(&state, &mut result, &ClientCapabilities::default());
        assert!(
            !state.file_watching(),
            "an absent capability — like an explicit false — captures as no watching",
        );
    }

    #[rstest]
    fn related_reports_merge_with_replace_false() {
        let state = ServerState::with_options::<PlainServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        let main_uri = crate::testing::url("file:///tmp/related-main.diag");
        let other_uri = crate::testing::url("file:///tmp/related-other.diag");
        let mut sink = WorkspaceReportSink::default();
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                version: None,
                uri: main_uri.clone(),
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some("main".into()),
                    items: Vec::new(),
                },
            }),
            true,
        );

        let related_documents = HashMap::from([
            (
                main_uri.clone(),
                DocumentDiagnosticReportKind::Full(FullDocumentDiagnosticReport {
                    result_id: Some("related".into()),
                    items: Vec::new(),
                }),
            ),
            (
                other_uri.clone(),
                DocumentDiagnosticReportKind::Unchanged(UnchangedDocumentDiagnosticReport {
                    result_id: "related-unchanged".into(),
                }),
            ),
        ]);
        push_related_reports(&state, Some(related_documents), &mut sink);

        assert_eq!(sink.reports.len(), 2, "both related reports merged in");
        for (report, replace) in &sink.reports {
            match report {
                WorkspaceDocumentDiagnosticReport::Full(full) => {
                    assert_eq!(full.uri, main_uri);
                    assert_eq!(
                        full.full_document_diagnostic_report.result_id.as_deref(),
                        Some("main"),
                        "a related report must not replace a main-document report",
                    );
                    assert!(*replace, "the main report keeps its replace flag");
                }
                WorkspaceDocumentDiagnosticReport::Unchanged(unchanged) => {
                    assert_eq!(unchanged.uri, other_uri);
                    assert!(!*replace, "related reports merge with replace = false");
                }
            }
        }
    }
}
