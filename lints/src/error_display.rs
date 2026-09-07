//! `error_display_lowercase`: display-message conventions for `#[error(...)]`
//! attributes on error enums.
//!
//! This lint runs pre-expansion: `#[error]` and `#[from]` are thiserror
//! helper attributes, and such attributes are consumed by the derive and
//! stripped from the HIR — they are only visible on the syntax tree.

extern crate rustc_ast;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_ast::{LitKind, ast};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::{Span, sym};

dylint_linting::impl_pre_expansion_lint! {
    /// ### What it does
    /// Checks `#[error("...")]` messages on error enums: the first alphabetic
    /// character must be lowercase and the literal must not end in `.`, `!`, or
    /// `?`. Also denies `#[error(transparent)]` combined with `#[from]`.
    ///
    /// ### Why is this bad?
    /// Uppercase starts and trailing punctuation break the house Display
    /// convention; `transparent` + `#[from]` forwards the `source()` call into
    /// an inner error that usually has no source of its own, silently breaking
    /// the error chain. `#[error("{0}")]` keeps the chain intact.
    ///
    /// ### Example
    /// ```rust
    /// # #[derive(Debug, thiserror::Error)]
    /// enum E {
    ///     #[error("Failed to read.")]
    ///     Read,
    /// }
    /// ```
    /// Use instead:
    /// ```rust
    /// # #[derive(Debug, thiserror::Error)]
    /// enum E {
    ///     #[error("failed to read")]
    ///     Read,
    /// }
    /// ```
    pub ERROR_DISPLAY_LOWERCASE,
    Deny,
    "`#[error(...)]` messages must start lowercase, carry no trailing punctuation, and must not be `transparent` on a `#[from]` variant",
    ErrorDisplayLowercase
}

#[derive(Default)]
pub struct ErrorDisplayLowercase;

impl EarlyLintPass for ErrorDisplayLowercase {
    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &ast::Item) {
        let ast::ItemKind::Enum(_, _, enum_def) = &item.kind else {
            return;
        };
        for variant in &enum_def.variants {
            if variant.span.from_expansion() {
                continue;
            }
            for attr in &variant.attrs {
                if !is_error_attr(attr) {
                    continue;
                }
                let Some(args) = attr.meta_item_list() else {
                    // e.g. `#[error = "..."]`, not the supported form.
                    continue;
                };
                for arg in args {
                    if let Some(lit) = arg.lit() {
                        if let LitKind::Str(message, _) = lit.kind {
                            report_message(cx, lit.span, message.as_str());
                        }
                    } else if arg.is_word()
                        && arg.has_name(sym::transparent)
                        && variant_carries_from(&variant.data)
                    {
                        span_lint_and_help(
                            cx,
                            ERROR_DISPLAY_LOWERCASE,
                            arg.span(),
                            "`#[error(transparent)]` combined with `#[from]`",
                            None,
                            "`transparent` forwards `source()` into the `#[from]` inner \
                             error; write `#[error(\"{0}\")]` instead, or drop `#[from]`",
                        );
                    }
                }
            }
        }
    }
}

fn is_error_attr(attr: &ast::Attribute) -> bool {
    // `error` is not a predefined `rustc_span::sym`, so compare the name text.
    attr.name().is_some_and(|name| name.as_str() == "error")
}

fn report_message(cx: &EarlyContext<'_>, span: Span, message: &str) {
    let uppercase_start = message
        .chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(char::is_uppercase);
    let trailing_punct = message
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, '.' | '!' | '?'));
    let msg = match (uppercase_start, trailing_punct) {
        (true, true) => "error message starts with an uppercase letter and ends with punctuation",
        (true, false) => "error message starts with an uppercase letter",
        (false, true) => "error message ends with punctuation",
        (false, false) => return,
    };
    span_lint_and_help(
        cx,
        ERROR_DISPLAY_LOWERCASE,
        span,
        msg,
        None,
        "start the message lowercase and drop any trailing `.`, `!`, or `?`",
    );
}

fn variant_carries_from(data: &ast::VariantData) -> bool {
    data.fields().iter().any(|field| {
        field
            .attrs
            .iter()
            .any(|attr: &ast::Attribute| attr.has_name(sym::from))
    })
}
