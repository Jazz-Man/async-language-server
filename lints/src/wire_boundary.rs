//! `wire_boundary`: only configured adapter paths may construct the wire
//! error type (`async_lsp::ResponseError`) or implement `From` for it.
//!
//! The suite's first configurable lint: patterns come from the linted
//! workspace's `dylint.toml`, keyed by library name, then lint name:
//!
//! ```toml
//! [strict-lints.wire_boundary]
//! allowed_paths = ["src/adapter/**"]
//! ```
//!
//! Converting through `From`/`Into` (`ResponseError::from(error)`) is not
//! flagged: it delegates to a boundary that the impl check already polices.
//! Reading a `ResponseError` is likewise fine. Empty or missing configuration
//! allows nothing — every project adopting the lint must name its boundary
//! sites explicitly. Macro-generated construction evades the lint: the
//! `from_expansion()` guards (standard convention against macro false
//! positives) skip code produced by expansion.
//!
//! The lint is a no-op in workspaces that do not depend on the protocol
//! crates (`async_lsp`, `lsp_types`), so the suite stays portable across the
//! owner's non-LSP projects.

extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rustc_hir::{
    Expr, ExprKind, Impl, Item, ItemKind, LangItem, QPath, TraitRef, TyKind,
    def::{CtorOf, DefKind, Res},
    def_id::{DefId, LOCAL_CRATE},
};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_middle::ty;
use rustc_span::{FileName, Span, Symbol};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Denies constructing the protocol error type (`async_lsp::ResponseError`)
    /// and implementing `From` for it outside configured adapter paths.
    /// Reading a `ResponseError` is not flagged.
    ///
    /// ### Why is this bad?
    /// Domain code must speak typed domain errors; exactly one boundary
    /// converts them to the wire form. A second construction site is a second
    /// boundary and erases the domain error model.
    ///
    /// ### Configuration
    /// `allowed_paths` in the linted workspace's `dylint.toml`, e.g.
    /// ```toml
    /// [strict-lints.wire_boundary]
    /// allowed_paths = ["src/adapter/**"]
    /// ```
    /// Paths are globs relative to the workspace root; an empty or missing
    /// table allows nothing.
    pub WIRE_BOUNDARY,
    Deny,
    "constructing `ResponseError` outside the wire adapter breaks the one-boundary rule",
    WireBoundary::new()
}

/// Flags `ResponseError` construction and `From` impls outside allowed paths.
pub struct WireBoundary {
    allowed: GlobSet,
}

impl Default for WireBoundary {
    fn default() -> Self {
        Self::new()
    }
}

impl WireBoundary {
    /// Reads the `[strict-lints.wire_boundary]` table from the linted
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
            allowed: allowed_globs(&config.wire_boundary.allowed_paths),
        }
    }

    /// Whether `span`'s source file sits under an allowed path.
    fn allowed(&self, cx: &LateContext<'_>, span: Span) -> bool {
        if self.allowed.is_empty() {
            return false;
        }
        let file = cx.sess().source_map().span_to_filename(span);
        let FileName::Real(real) = &file else {
            return false;
        };
        let Some(path) = real.local_path() else {
            return false;
        };
        self.allowed.is_match(workspace_relative(path))
    }

    fn check_construction(&self, cx: &LateContext<'_>, span: Span, is_wire: bool) {
        if !is_wire || self.allowed(cx, span) {
            return;
        }
        span_lint_and_help(
            cx,
            WIRE_BOUNDARY,
            span,
            "constructing `ResponseError` outside the wire adapter",
            None,
            "return a domain error and convert it at the single configured boundary instead",
        );
    }
}

impl<'tcx> LateLintPass<'tcx> for WireBoundary {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let ItemKind::Impl(imp) = &item.kind else {
            return;
        };
        if item.span.from_expansion() || !wire_crate_enabled(cx) {
            return;
        }
        if !from_impl_target(cx, imp) || self.allowed(cx, item.span) {
            return;
        }
        span_lint_and_help(
            cx,
            WIRE_BOUNDARY,
            item.span,
            "implementing `From` for `ResponseError` outside the wire adapter",
            None,
            "convert domain errors to the wire form at the single configured boundary instead",
        );
    }

    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx Expr<'tcx>) {
        if expr.span.from_expansion() || !wire_crate_enabled(cx) {
            return;
        }
        self.check_construction(cx, expr.span, construction_target(cx, expr));
    }
}

/// Deserialized from the `[strict-lints]` table of the linted workspace's
/// `dylint.toml`; each lint of this library owns its sub-table, and unknown
/// keys (other lints, other versions) are ignored.
#[derive(Default, serde::Deserialize)]
struct LibraryConfig {
    #[serde(default)]
    wire_boundary: Config,
}

/// Deserialized from `[strict-lints.wire_boundary]`.
#[derive(Default, serde::Deserialize)]
struct Config {
    /// Workspace-relative globs naming the files allowed to construct the
    /// wire error type.
    #[serde(default)]
    allowed_paths: Vec<String>,
}

/// Compiles `allowed_paths` into a glob set; empty input yields an empty set,
/// which matches nothing — nothing is allowed.
fn allowed_globs(paths: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in paths {
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .expect("valid glob in `wire_boundary.allowed_paths`"),
        );
    }
    builder
        .build()
        .expect("valid glob set from `wire_boundary.allowed_paths`")
}

/// Paths reach the matcher relative to the workspace root: cargo invokes the
/// driver from there and passes source paths relative to it. Absolute paths
/// fall back to stripping the current directory.
fn workspace_relative(path: &std::path::Path) -> &std::path::Path {
    if path.is_absolute()
        && let Ok(cwd) = std::env::current_dir()
        && let Ok(relative) = path.strip_prefix(&cwd)
    {
        return relative;
    }
    path
}

/// The portability no-op: engage only when the protocol crates (`async_lsp`,
/// `lsp_types`) are in the dependency graph.
fn wire_crate_enabled(cx: &LateContext<'_>) -> bool {
    cx.tcx
        .crates(())
        .iter()
        .any(|&cnum| matches!(cx.tcx.crate_name(cnum).as_str(), "async_lsp" | "lsp_types"))
        || matches!(
            cx.tcx.crate_name(LOCAL_CRATE).as_str(),
            "async_lsp" | "lsp_types"
        )
}

/// Whether `imp` implements `From` for the wire error type.
fn from_impl_target(cx: &LateContext<'_>, imp: &Impl<'_>) -> bool {
    let Some(of_trait) = imp.of_trait else {
        return false;
    };
    let TraitRef { path, .. } = of_trait.trait_ref;
    let Res::Def(DefKind::Trait, from_did) = path.res else {
        return false;
    };
    if cx.tcx.lang_items().get(LangItem::From) != Some(from_did) {
        return false;
    }
    let TyKind::Path(QPath::Resolved(None, self_path)) = imp.self_ty.kind else {
        return false;
    };
    self_path
        .res
        .opt_def_id()
        .is_some_and(|did| is_wire_error_type(cx, did))
}

/// Whether the expression constructs the wire error type: a struct literal, a
/// constructor call, or a call to one of the type's own functions
/// (`ResponseError::new`). Going through a `From`/`Into` conversion is not
/// construction: it delegates to a boundary the impl check already polices.
fn construction_target(cx: &LateContext<'_>, expr: &Expr<'_>) -> bool {
    let res = match expr.kind {
        ExprKind::Struct(qpath, ..) => cx.qpath_res(qpath, expr.hir_id),
        ExprKind::Call(callee, _) => match callee.kind {
            ExprKind::Path(qpath) => cx.qpath_res(&qpath, callee.hir_id),
            _ => return false,
        },
        _ => return false,
    };
    let did = match res {
        Res::Def(DefKind::Struct, did) => did,
        Res::Def(DefKind::Ctor(CtorOf::Struct, _), ctor_did) => cx.tcx.parent(ctor_did),
        Res::Def(DefKind::AssocFn, fn_did) => {
            let parent = cx.tcx.parent(fn_did);
            if !matches!(cx.tcx.def_kind(parent), DefKind::Impl { .. }) {
                // A trait method (`From::from`): the conversion goes through
                // some `From` impl, which the item check polices instead.
                return false;
            }
            match cx.tcx.type_of(parent).skip_binder().kind() {
                ty::Adt(adt, _) => adt.did(),
                _ => return false,
            }
        }
        _ => return false,
    };
    is_wire_error_type(cx, did)
}

/// Whether `did` is `async_lsp::ResponseError` — the protocol error type.
/// Checking the defining crate, not just the name, keeps domain types that
/// happen to share the name out of scope.
fn is_wire_error_type(cx: &LateContext<'_>, did: DefId) -> bool {
    cx.tcx.def_kind(did) == DefKind::Struct
        && cx.tcx.crate_name(did.krate).as_str() == "async_lsp"
        && cx.tcx.item_name(did) == Symbol::intern("ResponseError")
}
