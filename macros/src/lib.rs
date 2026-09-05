//! Procedural macros for `async-language-server`.
//!
//! Workspace-internal build plumbing — not part of the crate's public
//! surface. Input errors are reported as span-accurate [`syn::Error`]s on
//! the offending token, never as panics (`macro-proc-error-spans`); emitted
//! code references call-site `crate::` paths, which resolve inside the main
//! crate only.
//!
//! Macros: `#[lsp_request]` (per-file request registration),
//! `lsp_dispatch!` (dispatch entries for the async-lsp `LanguageServer`
//! impl), `lsp_method!` / `lsp_resolve_method!` (`Server`-trait default
//! bodies), and `conversion_tests!` (W0 conversion-test stamping).

mod conversion_tests {}
mod dispatch {}
mod request {}
mod trait_method;

use proc_macro::TokenStream;

/// Appends the `METHOD_NOT_FOUND` default body to a bodiless `Server`
/// trait-method declaration.
///
/// The written item — doc comments, attributes, signature — is re-emitted
/// unchanged; only the default body is generated. Use inside `trait Server`
/// for each of the 42 non-resolve request methods:
///
/// # Examples
///
/// ```ignore
/// lsp_method! {
///     /// Handles `textDocument/hover` requests from the client. ...
///     fn hover(&self, _state: ServerState, _params: HoverParams)
///         -> impl Future<Output = ServerResult<Option<Hover>>> + Send;
/// }
/// ```
#[proc_macro]
pub fn lsp_method(input: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(input as syn::TraitItemFn);
    trait_method::expand(item, trait_method::Kind::NotImplemented)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Appends the resolve default body (`Ok(item)`) to a bodiless `Server`
/// trait-method declaration; the final named parameter is returned
/// unchanged.
///
/// # Examples
///
/// ```ignore
/// lsp_resolve_method! {
///     /// Resolves a completion item. ...
///     fn completion_resolve(&self, _state: ServerState, item: CompletionItem)
///         -> impl Future<Output = ServerResult<CompletionItem>> + Send;
/// }
/// ```
#[proc_macro]
pub fn lsp_resolve_method(input: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(input as syn::TraitItemFn);
    trait_method::expand(item, trait_method::Kind::ResolveUnchanged)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
