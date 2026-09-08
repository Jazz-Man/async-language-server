//! `panics_doc_debt`: a documented `# Panics` section marks where a type
//! could carry the contract instead.

extern crate rustc_hir;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::intravisit::FnKind;
use rustc_hir::{Body, FnDecl};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::Span;
use rustc_span::def_id::LocalDefId;

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Warns on public functions whose documentation contains a
    /// `# Panics` section.
    ///
    /// ### Why is this bad?
    /// The section itself is good practice — this is a debt marker, not a
    /// style complaint. A documented panic contract says an invalid state
    /// reaches a public function as a value; a type (a `NonZero` newtype,
    /// a validated enum, a fallible constructor) may be able to remove
    /// that state entirely, deleting the panic with it.
    pub PANICS_DOC_DEBT,
    Warn,
    "documented panic contract is type-first debt: a type may be able to remove the invalid state",
    PanicsDocDebt
}

/// Flags public functions that document a `# Panics` section.
#[derive(Default)]
pub struct PanicsDocDebt;

impl<'tcx> LateLintPass<'tcx> for PanicsDocDebt {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        kind: FnKind<'tcx>,
        _decl: &'tcx FnDecl<'tcx>,
        _body: &'tcx Body<'tcx>,
        span: Span,
        def_id: LocalDefId,
    ) {
        if matches!(kind, FnKind::Closure) || !cx.tcx.visibility(def_id).is_public() {
            return;
        }
        let hir_id = cx.tcx.local_def_id_to_hir_id(def_id);
        let mut doc = String::new();
        for attr in cx.tcx.hir_attrs(hir_id) {
            if let Some(text) = attr.doc_str() {
                doc.push_str(text.as_str());
                doc.push('\n');
            }
        }
        if !doc
            .lines()
            .any(|line| line.trim_start().starts_with("# Panics"))
        {
            return;
        }
        span_lint_and_help(
            cx,
            PANICS_DOC_DEBT,
            span,
            "documented panic contract is type-first debt",
            None,
            "a type may be able to remove the invalid state (a `NonZero` newtype, a validated \
             enum, a fallible constructor)",
        );
    }
}
