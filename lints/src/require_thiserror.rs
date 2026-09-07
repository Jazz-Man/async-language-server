//! `require_thiserror`: enums named `*Error` must derive `thiserror::Error`.
//!
//! This lint runs pre-expansion: derive attributes are consumed by the derive
//! macros and stripped from the HIR, so they are only visible on the syntax
//! tree.

extern crate rustc_ast;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_ast::{MetaItemInner, ast};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::sym;

dylint_linting::impl_pre_expansion_lint! {
    /// ### What it does
    /// Checks that enums named `*Error` derive `thiserror::Error`.
    ///
    /// Detection is on the derive path's text: any derive whose path ends in
    /// `Error` counts (`#[derive(thiserror::Error)]` and
    /// `use thiserror::Error; #[derive(Error)]` both qualify), so an unrelated
    /// derive whose own name ends in `Error` counts too.
    ///
    /// ### Why is this bad?
    /// An error type without `thiserror` hand-rolls `Display` and
    /// `std::error::Error`, and the hand-rolled versions tend to lose the
    /// `source()` chain.
    ///
    /// ### Example
    /// ```rust
    /// enum NotDerivedError {
    ///     A,
    /// }
    /// ```
    /// Use instead:
    /// ```rust
    /// # #[derive(Debug, thiserror::Error)]
    /// enum DerivedError {
    ///     #[error("e")]
    ///     A,
    /// }
    /// ```
    pub REQUIRE_THISERROR,
    Deny,
    "enums named `*Error` must derive `thiserror::Error`",
    RequireThiserror
}

#[derive(Default)]
pub struct RequireThiserror;

impl EarlyLintPass for RequireThiserror {
    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &ast::Item) {
        let ast::ItemKind::Enum(ident, _, _) = &item.kind else {
            return;
        };
        if !ident.as_str().ends_with("Error") || item.span.from_expansion() {
            return;
        }
        if item.attrs.iter().any(derive_attr_has_error) {
            return;
        }
        span_lint_and_help(
            cx,
            REQUIRE_THISERROR,
            ident.span,
            "error enum does not derive `thiserror::Error`",
            None,
            "derive `thiserror::Error` so the enum gets `Display` and `source()` wiring",
        );
    }
}

fn derive_attr_has_error(attr: &ast::Attribute) -> bool {
    if !attr.has_name(sym::derive) {
        return false;
    }
    let Some(args) = attr.meta_item_list() else {
        return false;
    };
    args.iter().any(derive_path_ends_in_error)
}

fn derive_path_ends_in_error(arg: &MetaItemInner) -> bool {
    arg.meta_item()
        .and_then(|meta| meta.path.segments.last())
        .is_some_and(|segment| segment.ident.name.as_str() == "Error")
}
