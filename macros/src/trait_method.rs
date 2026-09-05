//! `lsp_method!` / `lsp_resolve_method!` — append a `Server`-trait method's
//! default body to a bodiless declaration, leaving every written token
//! (docs, attributes, signature) untouched (`macro-no-rewrite-item`).

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, TraitItemFn, parse_quote};

/// Which default body a trait-method declaration receives.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// `method_not_implemented(stringify!(name))` — the 42 normal methods.
    NotImplemented,
    /// `async move { Ok(item) }` — the 6 resolve methods, returning the
    /// last parameter unchanged.
    ResolveUnchanged,
}

/// Entry point shared by the `lsp_method!` / `lsp_resolve_method!` macros:
/// parses the invocation as a bodiless trait-method declaration and emits it
/// with `kind`'s default body appended, or the spanned error as a compile
/// error.
pub(super) fn entry(input: proc_macro::TokenStream, kind: Kind) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(input as syn::TraitItemFn);
    expand(item, kind)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Parses the invocation input as a bodiless trait-method declaration and
/// returns it with the default body for `kind` appended.
///
/// # Errors
///
/// When the declaration already carries a body, or (resolve kind) has no
/// named final parameter to return — the error spans the offending tokens.
fn expand(mut item: TraitItemFn, kind: Kind) -> syn::Result<TokenStream> {
    if let Some(default) = item.default.as_ref() {
        return Err(syn::Error::new(
            default.brace_token.span.join(),
            "expected a bodiless declaration; the macro appends the default body",
        ));
    }
    let body: syn::Block = match kind {
        Kind::NotImplemented => {
            let name = &item.sig.ident;
            parse_quote!({ method_not_implemented(stringify!(#name)) })
        }
        Kind::ResolveUnchanged => {
            let ident = last_param_ident(&item)?;
            parse_quote!({ async move { Ok(#ident) } })
        }
    };
    item.default = Some(body);
    Ok(quote! { #item })
}

/// The identifier of the declaration's final parameter, spanned-erroring
/// when it is missing or not a plain named binding.
fn last_param_ident(item: &TraitItemFn) -> syn::Result<&Ident> {
    let Some(syn::FnArg::Typed(typed)) = item.sig.inputs.last() else {
        return Err(syn::Error::new(
            item.sig.paren_token.span.join(),
            "resolve methods take the item as their final named parameter",
        ));
    };
    match typed.pat.as_ref() {
        syn::Pat::Ident(pat) => Ok(&pat.ident),
        other => Err(syn::Error::new_spanned(
            other,
            "expected a plain named parameter (e.g. `item: CompletionItem`)",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(tokens: proc_macro2::TokenStream) -> TraitItemFn {
        syn::parse2(tokens).expect("declaration parses")
    }

    // Table-driven to avoid the near-duplicate pair the dupes gate flagged;
    // cases differ only in Kind and the needles the body must carry.
    #[test]
    fn expand_appends_default_body_per_kind() {
        let cases: [(Kind, TokenStream, &[&str]); 2] = [
            (
                Kind::NotImplemented,
                quote! {
                    /// doc
                    fn hover(&self, _state: ServerState, _params: HoverParams) -> impl Future<Output = R> + Send;
                },
                // "doc =" pins that docs survive as attributes.
                &["method_not_implemented", "stringify", "(hover)", "doc ="],
            ),
            (
                Kind::ResolveUnchanged,
                quote! {
                    fn completion_resolve(&self, _state: ServerState, item: CompletionItem)
                        -> impl Future<Output = ServerResult<CompletionItem>> + Send;
                },
                // "Ok"/"item" pin that the last parameter is returned unchanged.
                &["async move", "Ok", "item"],
            ),
        ];
        for (i, (kind, decl, needles)) in cases.into_iter().enumerate() {
            let text = expand(parse(decl), kind).expect("expands").to_string();
            for needle in needles {
                assert!(
                    text.contains(needle),
                    "case {i}: {needle:?} missing from {text}"
                );
            }
        }
    }

    // Table-driven to avoid the duplicate reject tests the dupes gate
    // flagged; the per-kind success table above is the precedent.
    #[test]
    fn rejects_bodied_and_malformed_declarations() {
        let cases: [(Kind, TokenStream, &str); 3] = [
            (
                Kind::NotImplemented,
                quote! {
                    fn hover(&self) -> impl Future<Output = R> + Send { ready(()) }
                },
                // "bodiless" pins the has-a-body rejection.
                "bodiless",
            ),
            (
                Kind::ResolveUnchanged,
                quote! { fn r(&self) -> impl Future<Output = R> + Send; },
                // "final named parameter" pins the no-parameter rejection.
                "final named parameter",
            ),
            (
                Kind::ResolveUnchanged,
                quote! {
                    fn r(&self, (a, b): (u8, u8)) -> impl Future<Output = R> + Send;
                },
                // "plain named parameter" pins the pattern-parameter rejection.
                "plain named parameter",
            ),
        ];
        for (i, (kind, decl, needle)) in cases.into_iter().enumerate() {
            let err = expand(parse(decl), kind).expect_err("input rejected");
            assert!(
                err.to_string().contains(needle),
                "case {i}: {needle:?} missing from {err}"
            );
        }
    }
}
