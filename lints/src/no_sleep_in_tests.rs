//! `no_sleep_in_tests`: real sleeps are denied in test code.

extern crate rustc_hir;

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::is_in_test;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Denies `std::thread::sleep` and `tokio::time::sleep` calls in test
    /// code: inside `#[test]` functions or under `#[cfg(test)]` items.
    ///
    /// ### Why is this bad?
    /// House determinism rule: tests gate on channels and bound every
    /// cross-task wait (`tokio::time::timeout`), never on sleeps. A sleep
    /// either hides a race that still flakes under load, or spends real
    /// test time waiting for nothing.
    pub NO_SLEEP_IN_TESTS,
    Deny,
    "tests gate on channels and bounded waits, never sleep",
    NoSleepInTests
}

/// Flags real sleeps in test code.
#[derive(Default)]
pub struct NoSleepInTests;

impl<'tcx> LateLintPass<'tcx> for NoSleepInTests {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if expr.span.from_expansion() {
            return;
        }
        let did = match expr.kind {
            ExprKind::Call(callee, _) => match callee.kind {
                ExprKind::Path(qpath) => cx.qpath_res(&qpath, callee.hir_id).opt_def_id(),
                _ => None,
            },
            _ => None,
        };
        let Some(did) = did else {
            return;
        };
        let path = cx.tcx.def_path_str(did);
        if !matches!(path.as_str(), "std::thread::sleep" | "tokio::time::sleep")
            || !is_in_test(cx.tcx, expr.hir_id)
        {
            return;
        }
        span_lint_and_help(
            cx,
            NO_SLEEP_IN_TESTS,
            expr.span,
            format!("`{path}` in test code"),
            None,
            "gate on a channel, or bound the wait with `tokio::time::timeout`",
        );
    }
}
