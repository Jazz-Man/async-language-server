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

/// Parses the invocation input as a bodiless trait-method declaration and
/// returns it with the default body for `kind` appended.
///
/// # Errors
///
/// When the declaration already carries a body, or (resolve kind) has no
/// named final parameter to return — the error spans the offending tokens.
pub(super) fn expand(mut item: TraitItemFn, kind: Kind) -> syn::Result<TokenStream> {
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

    #[test]
    fn not_implemented_appends_body_with_method_name() {
        let item = parse(quote! {
            /// doc
            fn hover(&self, _state: ServerState, _params: HoverParams) -> impl Future<Output = R> + Send;
        });
        let out = expand(item, Kind::NotImplemented).expect("expands");
        let text = out.to_string();
        assert!(text.contains("method_not_implemented"));
        assert!(text.contains("stringify"));
        assert!(text.contains("(hover)"));
        assert!(text.contains("doc =")); // docs survive as attributes
    }

    #[test]
    fn resolve_appends_ok_of_last_param() {
        let item = parse(quote! {
            fn completion_resolve(&self, _state: ServerState, item: CompletionItem)
                -> impl Future<Output = ServerResult<CompletionItem>> + Send;
        });
        let out = expand(item, Kind::ResolveUnchanged).expect("expands");
        let text = out.to_string();
        assert!(text.contains("async move"));
        assert!(text.contains("Ok")); // the last parameter is returned unchanged
        assert!(text.contains("item"));
    }

    #[test]
    fn rejects_declaration_with_body() {
        let item = parse(quote! {
            fn hover(&self) -> impl Future<Output = R> + Send { ready(()) }
        });
        let err = expand(item, Kind::NotImplemented).expect_err("bodied input rejected");
        assert!(err.to_string().contains("bodiless"));
    }

    #[test]
    fn resolve_rejects_missing_param() {
        let item = parse(quote! { fn r(&self) -> impl Future<Output = R> + Send; });
        assert!(expand(item, Kind::ResolveUnchanged).is_err());
    }

    #[test]
    fn resolve_rejects_pattern_parameter() {
        let item = parse(quote! {
            fn r(&self, (a, b): (u8, u8)) -> impl Future<Output = R> + Send;
        });
        let err = expand(item, Kind::ResolveUnchanged).expect_err("pattern parameter rejected");
        assert!(err.to_string().contains("plain named parameter"));
    }
}
