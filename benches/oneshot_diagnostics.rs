//! On-demand wall-clock benchmark of the batch diagnostics pipeline over a
//! synthetic workspace: a CPU-bound stand-in handler, `FILES` matching
//! documents, and the full `oneshot::workspace_diagnostics`
//! walk-open-diagnose path.
//!
//! Run with `cargo bench --bench oneshot_diagnostics`; not part of the CI
//! battery (`--all-targets` builds and lints it in all feature
//! configurations).

use std::{
    io::Write as _,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use async_language_server::lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use async_language_server::oneshot::{WorkspaceDiagnosticConfig, workspace_diagnostics};
use async_language_server::server::{
    DocumentMatcher, Server, ServerOptions, ServerResult, ServerState,
};
use criterion::{Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;

/// Documents in the synthetic workspace.
const FILES: usize = 256;

/// Fixed pipeline width so numbers stay comparable across machines.
///
/// The `None` arm is compile-time validation: const evaluation rejects the
/// program if it could ever be selected, so no runtime panic path exists.
const WIDTH: NonZeroUsize = match NonZeroUsize::new(4) {
    Some(width) => width,
    None => panic!("WIDTH must be nonzero"),
};

/// CPU budget burned per document by [`BurnServer`].
const BURN_ITERATIONS: usize = 20_000;

/// A CPU-bound stand-in for a real validator: burns a fixed budget per
/// document so the bench measures pipeline throughput, not IO.
struct BurnServer;

impl Server for BurnServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("bench").with_url_globs(["**/*.bench", "*.bench"])]
    }

    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_diagnostics_parallelism(WIDTH)
    }

    fn document_diagnostics(
        &self,
        _state: ServerState,
        _params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send
    {
        // The burn runs during the call, inside the engine's width-bounded
        // per-document task; the handler awaits nothing.
        black_box_burn(BURN_ITERATIONS);
        std::future::ready(Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            }),
        )))
    }
}

/// Burns a fixed arithmetic budget so a document's handler time is stable and
/// comparable across runs.
fn black_box_burn(iterations: usize) {
    // The boxed bound keeps the loop runtime-bound: without it the compiler
    // could fold the constant iteration count away entirely.
    let iterations = std::hint::black_box(iterations);
    let mut acc = 0u64;
    for _ in 0..iterations {
        acc = acc.wrapping_add(0x9E37_79B9_7F4A_7C15).rotate_left(7);
    }
    std::hint::black_box(acc);
}

/// Creates the synthetic workspace: `files` matching documents at the root.
///
/// # Errors
///
/// Fails when the temp directory or any document cannot be created; the
/// caller skips the bench.
fn make_workspace(files: usize) -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir =
        std::env::temp_dir().join(format!("als-bench-oneshot-{}-{nanos}", std::process::id()));
    // arch-lint: allow(no-sync-io) reason="bench setup runs before any async runtime exists; there is nothing to block"
    std::fs::create_dir_all(&dir)?;
    for i in 0..files {
        // arch-lint: allow(no-sync-io) reason="bench setup runs before any async runtime exists; there is nothing to block"
        std::fs::write(
            dir.join(format!("file{i}.bench")),
            format!("line one {i}\nline two\nline three\n"),
        )?;
    }
    Ok(dir)
}

/// Removes the synthetic workspace; a failed cleanup is reported, not
/// swallowed — a leaked directory must be attributable to its maker.
fn remove_workspace(root: &Path) {
    // arch-lint: allow(no-sync-io) reason="bench teardown runs outside the async runtime; there is nothing to block"
    if let Err(error) = std::fs::remove_dir_all(root) {
        note(&format!("workspace cleanup failed: {error}"));
    }
}

/// Writes a best-effort line to stderr. The bench owns no protocol channel;
/// the print-macro lints steer even this note through ordinary IO, and a
/// failed write to a vanished terminal has nothing better to report to.
fn note(message: &str) {
    let _ = writeln!(std::io::stderr(), "oneshot_diagnostics: {message}");
}

fn bench_oneshot(c: &mut Criterion) {
    let root = match make_workspace(FILES) {
        Ok(root) => root,
        Err(error) => {
            note(&format!("skipping: workspace setup failed: {error}"));
            return;
        }
    };
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            note(&format!("skipping: runtime creation failed: {error}"));
            remove_workspace(&root);
            return;
        }
    };

    // One untimed probe pins the setup before any measurement: the pipeline
    // runs and every synthetic document actually matches.
    match runtime.block_on(workspace_diagnostics(
        BurnServer,
        WorkspaceDiagnosticConfig::new(&root),
    )) {
        Ok(report) if report.documents.len() == FILES => {}
        Ok(report) => {
            note(&format!(
                "skipping: probe saw {} of {FILES} documents - matcher and workspace disagree",
                report.documents.len()
            ));
            remove_workspace(&root);
            return;
        }
        Err(error) => {
            note(&format!("skipping: probe run failed: {error}"));
            remove_workspace(&root);
            return;
        }
    }

    let failures = AtomicUsize::new(0);
    let mut group = c.benchmark_group("oneshot_diagnostics");
    group.measurement_time(Duration::from_secs(10));
    group.bench_function("parallel", |b| {
        b.iter(|| {
            let outcome = runtime.block_on(workspace_diagnostics(
                BurnServer,
                WorkspaceDiagnosticConfig::new(&root),
            ));
            if outcome.is_err() {
                failures.fetch_add(1, Ordering::Relaxed);
            }
            std::hint::black_box(outcome)
        });
    });
    group.finish();

    let failed = failures.load(Ordering::Relaxed);
    if failed > 0 {
        note(&format!(
            "{failed} measured iterations failed - ignore the numbers above"
        ));
    }

    remove_workspace(&root);
}

criterion_group!(benches, bench_oneshot);
criterion_main!(benches);
