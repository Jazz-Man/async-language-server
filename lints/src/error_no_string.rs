//! `error_no_string_catch_all`: error variants whose fields are all strings
//! stringify their failure instead of carrying a typed source.
//!
//! This lint stays a late pass: it only reads resolved type shapes
//! (`String`/`str`), never attributes — thiserror helper attributes are
//! stripped from the HIR, so attribute-based checks must run pre-expansion
//! instead (see `error_display`, `require_thiserror`).

extern crate rustc_hir;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{EnumDef, FieldDef, Item, ItemKind, QPath, Ty, TyKind, VariantData};
use rustc_lint::{LateContext, LateLintPass};
use rustc_span::{Symbol, sym};

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Checks error enums for variants whose fields are all `String` or
    /// `&str` (a single unnamed field like `Unknown(String)`, or named fields
    /// where every field is string-typed and at least one exists).
    ///
    /// ### Why is this bad?
    /// A string field flattens the failure into text: `source()` dies, and
    /// callers cannot match on the cause. A wire message is data; a stringified
    /// cause is not.
    ///
    /// ### Example
    /// ```rust
    /// # #[derive(Debug, thiserror::Error)]
    /// enum E {
    ///     #[error("unknown failure")]
    ///     Unknown(String),
    /// }
    /// ```
    /// Use instead:
    /// ```rust
    /// # #[derive(Debug, thiserror::Error)]
    /// enum E {
    ///     #[error("unknown failure")]
    ///     Unknown(#[source] std::io::Error),
    /// }
    /// ```
    pub ERROR_NO_STRING_CATCH_ALL,
    Deny,
    "error variant stringifies its failure; carry a typed source instead",
    ErrorNoStringCatchAll
}

#[derive(Default)]
pub struct ErrorNoStringCatchAll;

impl<'tcx> LateLintPass<'tcx> for ErrorNoStringCatchAll {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let Some(enum_def) = error_enum(item) else {
            return;
        };
        for variant in enum_def.variants {
            let Some(fields) = variant_fields(&variant.data) else {
                continue;
            };
            if fields.is_empty() || variant.span.from_expansion() {
                continue;
            }
            if !fields.iter().all(|field| is_string_like(field.ty)) {
                continue;
            }
            span_lint_and_help(
                cx,
                ERROR_NO_STRING_CATCH_ALL,
                variant.span,
                "error variant stringifies its failure",
                None,
                "carry a typed source or structured data instead of string fields",
            );
        }
    }
}

const fn error_enum<'tcx>(item: &'tcx Item<'tcx>) -> Option<&'tcx EnumDef<'tcx>> {
    let ItemKind::Enum(_, _, enum_def) = &item.kind else {
        return None;
    };
    Some(enum_def)
}

const fn variant_fields<'a>(data: &'a VariantData<'a>) -> Option<&'a [FieldDef<'a>]> {
    match data {
        VariantData::Struct { fields, .. } | VariantData::Tuple(fields, ..) => Some(fields),
        VariantData::Unit(..) => None,
    }
}

/// The final path segment of a type expression, if it is a resolved path.
fn path_final_name(ty: &Ty<'_>) -> Option<Symbol> {
    let TyKind::Path(QPath::Resolved(None, path)) = &ty.kind else {
        return None;
    };
    path.segments.last().map(|segment| segment.ident.name)
}

fn is_string_like(ty: &Ty<'_>) -> bool {
    match &ty.kind {
        TyKind::Path(..) => path_final_name(ty) == Some(sym::String),
        TyKind::Ref(_, mut_ty) | TyKind::Ptr(mut_ty) => {
            path_final_name(mut_ty.ty) == Some(sym::str)
        }
        _ => false,
    }
}
