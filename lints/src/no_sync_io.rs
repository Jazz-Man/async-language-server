//! `no_sync_io`: synchronous filesystem operations are denied outside
//! configured allowed paths.
//!
//! Configuration comes from the linted workspace's `dylint.toml`, keyed by
//! library name, then lint name:
//!
//! ```toml
//! [strict-lints.no_sync_io]
//! allowed_paths = ["src/state/**", "benches/**"]
//! ```
//!
//! The deny set is deliberately narrow: calls resolving under `std::fs::`
//! (free functions, `std::fs::File` constructors, and the `std::fs::Metadata`
//! / `std::fs::FileType` accessors) plus the four `std::path::Path` probes
//! (`exists`, `metadata`, `is_file`, `is_dir`). Reads and writes through the
//! `std::io` traits are out of scope for now: their file-backed impls cannot
//! be told apart from in-memory ones at the call site.
//!
//! Code inside `#[test]` functions and `#[cfg(test)]` modules is exempt,
//! matching arch-lint's `allow_in_tests` behavior. Benchmarks compile without
//! test configuration, so they are blessed through `allowed_paths` instead.
//! Empty or missing configuration allows nothing — every project adopting
//! the lint must name its synchronous-IO sites explicitly.

extern crate rustc_hir;

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::is_in_test;
use globset::GlobSet;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Denies synchronous filesystem operations (`std::fs` calls and the
    /// `std::path::Path` probes) outside configured allowed paths.
    ///
    /// ### Why is this bad?
    /// Synchronous file IO blocks the thread it runs on. Inside an async
    /// runtime that stalls unrelated tasks; the IO belongs on the blocking
    /// pool, or behind an async filesystem API.
    ///
    /// ### Configuration
    /// `allowed_paths` in the linted workspace's `dylint.toml`, e.g.
    /// ```toml
    /// [strict-lints.no_sync_io]
    /// allowed_paths = ["src/state/**", "benches/**"]
    /// ```
    /// Paths are globs relative to the workspace root; an empty or missing
    /// table allows nothing. Test code is exempt without configuration.
    pub NO_SYNC_IO,
    Deny,
    "synchronous file IO can block the async runtime",
    NoSyncIo::new()
}

/// Flags synchronous filesystem operations outside allowed paths.
pub struct NoSyncIo {
    allowed: GlobSet,
}

impl Default for NoSyncIo {
    fn default() -> Self {
        Self::new()
    }
}

impl NoSyncIo {
    /// Reads the `[strict-lints.no_sync_io]` table from the linted
    /// workspace's `dylint.toml`.
    ///
    /// # Panics
    /// Panics when `allowed_paths` holds an invalid glob, or the table cannot
    /// be parsed as this configuration — a malformed config must fail loudly,
    /// not silently narrow the lint.
    #[must_use]
    pub fn new() -> Self {
        let config = dylint_linting::config_or_default::<LibraryConfig>(env!("CARGO_PKG_NAME"));
        Self {
            allowed: crate::globs::glob_set(
                "no_sync_io.allowed_paths",
                &config.no_sync_io.allowed_paths,
            ),
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for NoSyncIo {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if expr.span.from_expansion() {
            return;
        }
        let did = match expr.kind {
            // A plain call: `std::fs::read_to_string(...)`.
            ExprKind::Call(callee, _) => match callee.kind {
                ExprKind::Path(qpath) => cx.qpath_res(&qpath, callee.hir_id).opt_def_id(),
                _ => None,
            },
            // A method call: `path.exists()`.
            ExprKind::MethodCall(..) => cx.typeck_results().type_dependent_def_id(expr.hir_id),
            _ => return,
        };
        let Some(did) = did else {
            return;
        };
        let path = cx.tcx.def_path_str(did);
        if !is_sync_io(&path)
            || is_in_test(cx.tcx, expr.hir_id)
            || crate::globs::span_matches_file(cx, &self.allowed, expr.span)
        {
            return;
        }
        span_lint_and_help(
            cx,
            NO_SYNC_IO,
            expr.span,
            format!("synchronous file IO via `{path}`"),
            None,
            "use the async filesystem APIs, or move the operation to the blocking pool",
        );
    }
}

/// Deserialized from the `[strict-lints]` table of the linted workspace's
/// `dylint.toml`; each lint of this library owns its sub-table, and unknown
/// keys (other lints, other versions) are ignored.
#[derive(Default, serde::Deserialize)]
struct LibraryConfig {
    #[serde(default)]
    no_sync_io: PathConfig,
}

/// Deserialized from `[strict-lints.no_sync_io]`.
#[derive(Default, serde::Deserialize)]
struct PathConfig {
    /// Workspace-relative globs naming the files allowed to perform
    /// synchronous filesystem operations.
    #[serde(default)]
    allowed_paths: Vec<String>,
}

/// Whether `path` (a `def_path_str`) names a synchronous filesystem operation.
fn is_sync_io(path: &str) -> bool {
    path.starts_with("std::fs::")
        || matches!(
            path,
            "std::path::Path::exists"
                | "std::path::Path::metadata"
                | "std::path::Path::is_file"
                | "std::path::Path::is_dir"
        )
}
