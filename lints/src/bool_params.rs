//! `bool_params_public`: bare `bool` parameters on public functions are
//! flagged as API debt.

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::{is_in_test, is_trait_impl_item};
use rustc_hir::def::Res;
use rustc_hir::intravisit::FnKind;
use rustc_hir::{Body, FnDecl, HirId, ItemKind, Node, PrimTy, QPath, Ty, TyKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;
use rustc_span::def_id::LocalDefId;

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Warns on public functions that take a bare `bool` parameter, or a
    /// reference or raw pointer to one.
    ///
    /// ### Why is this bad?
    /// A `bool` parameter buries a choice at every call site — `f(true)`
    /// says nothing about what `true` means. An enum names the choice at
    /// both ends; a newtype validates it at construction. Trait methods
    /// are exempt: an impl must match its trait, so the fix belongs at the
    /// trait declaration.
    pub BOOL_PARAMS_PUBLIC,
    Warn,
    "public functions take a named type, not a bare bool",
    BoolParamsPublic
}

/// Flags bare `bool` parameters on public functions.
#[derive(Default)]
pub struct BoolParamsPublic;

impl<'tcx> LateLintPass<'tcx> for BoolParamsPublic {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        kind: FnKind<'tcx>,
        decl: &'tcx FnDecl<'tcx>,
        _body: &'tcx Body<'tcx>,
        _span: Span,
        def_id: LocalDefId,
    ) {
        if matches!(kind, FnKind::Closure) {
            return;
        }
        let hir_id = cx.tcx.local_def_id_to_hir_id(def_id);
        if !cx.tcx.visibility(def_id).is_public()
            || is_trait_impl_item(cx, hir_id)
            || in_trait_declaration(cx.tcx, hir_id)
            || is_in_test(cx.tcx, hir_id)
        {
            return;
        }
        for param_ty in decl.inputs {
            if is_bool_ty(param_ty) {
                span_lint_and_help(
                    cx,
                    BOOL_PARAMS_PUBLIC,
                    param_ty.span,
                    "bare `bool` parameter on a public function",
                    None,
                    "use an enum for the choice, or a newtype that validates at construction",
                );
            }
        }
    }
}

/// Whether `hir_id`'s parent item is a trait declaration — a method there
/// (required or defaulted) shares the trait's signature contract.
fn in_trait_declaration(tcx: TyCtxt<'_>, hir_id: HirId) -> bool {
    matches!(
        tcx.parent_hir_node(hir_id),
        Node::Item(item) if matches!(item.kind, ItemKind::Trait { .. })
    )
}

/// Whether the HIR type is `bool`, or a reference or raw pointer to one.
fn is_bool_ty(ty: &Ty<'_>) -> bool {
    match &ty.kind {
        TyKind::Path(QPath::Resolved(None, path)) => {
            matches!(path.res, Res::PrimTy(PrimTy::Bool))
        }
        TyKind::Ref(_, inner) | TyKind::Ptr(inner) => is_bool_ty(inner.ty),
        _ => false,
    }
}
