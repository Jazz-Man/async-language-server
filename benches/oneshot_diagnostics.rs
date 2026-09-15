//! On-demand wall-clock benchmarks of the batch diagnostics pipeline over a
//! synthetic workspace: a CPU-bound stand-in handler, `FILES` matching
//! documents, and the full `oneshot::workspace_diagnostics`
//! walk-open-diagnose path, plus the two costs a `workspace/diagnostic`
//! poll pays in gitignore handling (`GlobSet` rebuild, fresh parallel walk)
//! extracted from the 2026-09-15 lsp-poc performance research.
//!
//! Run with `cargo bench --bench oneshot_diagnostics`; not part of the CI
//! battery (`--all-targets` builds and lints it in all feature
//! configurations). The groups pin a 10 s measurement time in code, so
//! `--measurement-time` on the command line will not change it.

use async_language_server::lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use async_language_server::oneshot::{WorkspaceDiagnosticConfig, workspace_diagnostics};
use async_language_server::server::{
    DocumentMatcher, Server, ServerOptions, ServerResult, ServerState,
};
use criterion::{Criterion, criterion_group, criterion_main};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
use std::io::Write as _;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
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

/// Patterns in the synthetic gitignore-scale set.
const PATTERNS: usize = 100;

/// Paths in the synthetic match corpus.
const CORPUS: usize = 2_000;

/// Directories (each with [`FILES_PER_DIR`] files) in the synthetic walk tree.
const DIRS: usize = 50;

/// Files per directory in the synthetic walk tree.
const FILES_PER_DIR: usize = 10;

/// Builds one representative mid-path `**` pattern — the glob class that
/// falls through to globset's Regex strategy and dominates gitignore
/// compiles (globset logs a `converted to regex` line exactly for these).
fn pattern(index: usize) -> String {
    format!("**/module{index}-cache/**/*.tmp")
}

/// The synthetic gitignore-scale pattern set.
fn pattern_set() -> Vec<String> {
    (0..PATTERNS).map(pattern).collect()
}

/// Absolute synthetic paths, roughly one in `2 * PATTERNS` matching the set.
fn corpus() -> Vec<String> {
    (0..CORPUS)
        .map(|i| {
            let module = i % (PATTERNS * 2);
            format!("/ws/src/module{module}/gen/file{}.rs", i % 7)
        })
        .collect()
}

/// Creates the synthetic walk tree: `DIRS` directories with
/// [`FILES_PER_DIR`] files each, under a millisecond-unique temp directory.
///
/// # Errors
///
/// Fails when the temp directory or any file cannot be created; the caller
/// skips the bench.
fn make_tree() -> std::io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "als-bench-glob-walk-{}-{nanos}",
        std::process::id()
    ));
    for dir_index in 0..DIRS {
        let sub = dir.join(format!("module{dir_index}"));
        // arch-lint: allow(no-sync-io) reason="bench setup runs before any async runtime exists; there is nothing to block"
        std::fs::create_dir_all(&sub)?;
        for file_index in 0..FILES_PER_DIR {
            // arch-lint: allow(no-sync-io) reason="bench setup runs before any async runtime exists; there is nothing to block"
            std::fs::write(sub.join(format!("file{file_index}.rs")), "x\n")?;
        }
    }
    Ok(dir)
}

/// Compiles the synthetic set once for the match benchmark; returns `None`
/// and reports when a controlled pattern fails to compile (a bench bug,
/// never an expected path).
fn compiled_set(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for source in patterns {
        match Glob::new(source) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(error) => {
                note(&format!(
                    "skipping: pattern {source:?} failed to compile: {error}"
                ));
                return None;
            }
        }
    }
    match builder.build() {
        Ok(set) => Some(set),
        Err(error) => {
            note(&format!("skipping: glob set failed to build: {error}"));
            None
        }
    }
}

/// Benchmarks the gitignore-handling costs a `workspace/diagnostic` poll
/// pays while walks run per request: the `GlobSet` rebuild over a
/// gitignore-scale set, match throughput with a prebuilt set, and the fresh
/// parallel walk the refresh performs.
fn bench_glob_walk(criterion: &mut Criterion) {
    let patterns = pattern_set();
    let paths = corpus();
    let Some(set) = compiled_set(&patterns) else {
        return;
    };

    let root = match make_tree() {
        Ok(root) => root,
        Err(error) => {
            note(&format!("skipping: tree setup failed: {error}"));
            return;
        }
    };

    let mut group = criterion.benchmark_group("glob_walk");
    group.measurement_time(Duration::from_secs(10));

    // The per-rebuild cost: what every fresh `GlobSet::new` pays for a
    // gitignore-scale set — the cost each gitignore compilation inside a
    // walk pays, several times per request.
    group.bench_function("build_gitignore_scale_set", |bench| {
        bench.iter(|| {
            let mut builder = GlobSetBuilder::new();
            for source in &patterns {
                if let Ok(glob) = Glob::new(source) {
                    builder.add(glob);
                }
            }
            std::hint::black_box(builder.build())
        });
    });

    // Match throughput over the corpus with a prebuilt set — the per-entry
    // filter cost, for comparison against alternative matchers.
    group.bench_function("match_corpus_prebuilt", |bench| {
        bench.iter(|| {
            let matches = paths
                .iter()
                .filter(|path| set.is_match(path.as_str()))
                .count();
            std::hint::black_box(matches)
        });
    });

    // The fresh-walk cost: one `WalkBuilder` instantiation and parallel run
    // per iteration — the shape a per-request workspace refresh performs.
    let seen = AtomicUsize::new(0);
    group.bench_function("fresh_parallel_walk", |bench| {
        bench.iter(|| {
            seen.store(0, Ordering::Relaxed);
            WalkBuilder::new(&root).build_parallel().run(|| {
                Box::new(|entry| {
                    match entry {
                        Ok(entry) => {
                            // arch-lint: allow(no-sync-io) reason="FileType::is_file reads a flag off metadata the walker already collected; no filesystem access happens in this closure"
                            if entry.file_type().is_some_and(|ty| ty.is_file()) {
                                seen.fetch_add(1, Ordering::Relaxed);
                            }
                            WalkState::Continue
                        }
                        Err(_) => WalkState::Continue,
                    }
                })
            });
            std::hint::black_box(seen.load(Ordering::Relaxed))
        });
    });

    group.finish();
    remove_workspace(&root);
}

fn bench_oneshot(criterion: &mut Criterion) {
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
                report.documents.len(),
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
    let mut group = criterion.benchmark_group("oneshot_diagnostics");
    group.measurement_time(Duration::from_secs(10));
    group.bench_function("parallel", |bench| {
        bench.iter(|| {
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
            "{failed} measured iterations failed - ignore the numbers above",
        ));
    }

    remove_workspace(&root);
}

criterion_group!(benches, bench_oneshot, bench_glob_walk);
criterion_main!(benches);
