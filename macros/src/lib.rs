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
mod dispatch;
mod request;
mod trait_method;

use proc_macro::TokenStream;
use trait_method::{Kind, entry};

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
    entry(input, Kind::NotImplemented)
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
    entry(input, Kind::ResolveUnchanged)
}

/// Stamps `LanguageServer` dispatch methods from the request table.
///
/// # Examples
///
/// ```ignore
/// lsp_dispatch! {
///     hover: hover @ crate::requests::HoverRequest,
///     rename_prepare: prepare_rename @ crate::requests::RenamePrepareRequest,
///     resolve(completion_resolve: completion_resolve @ crate::requests::CompletionResolveRequest),
/// }
/// ```
///
/// Rows are `our_trait_method : async_lsp_method @ RequestType`; resolve
/// rows wrap the same triple in `resolve(...)`. Every row expands to one
/// dispatch method whose body is the former `implement_method!` /
/// `implement_resolve_method!` engine: version snapshot, encoding
/// conversion, staleness detection, and the user's `Server` call.
#[proc_macro]
pub fn lsp_dispatch(input: TokenStream) -> TokenStream {
    dispatch::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Registers one LSP request on a non-generic struct, stamping its `impl
/// Request`.
///
/// `params` and `response` carry the wire types; every hook is optional —
/// unspecified hooks keep the `Request` trait's delegating defaults:
///
/// - `document(...)` — field path to the params' document URL (enables
///   `extract_url`)
/// - `incoming_position(...)` / `incoming_range(...)` — field path whose
///   position/range `modify_params` converts client encoding → UTF-8
/// - `incoming_custom(...)` / `outgoing(...)` — function path
///   `fn(&ServerState, &Document, &mut Params or &mut Response)`
/// - `incoming_standalone(...)` / `outgoing_standalone(...)` — function
///   path `fn(&ServerState, &mut Params or &mut Response)` for the
///   no-anchor hooks
///
/// # Examples
///
/// ```ignore
/// #[lsp_request(
///     params = async_lsp::lsp_types::HoverParams,
///     response = Option<async_lsp::lsp_types::Hover>,
///     document(text_document_position_params.text_document),
///     incoming_position(text_document_position_params.position),
/// )]
/// pub struct HoverRequest;
/// ```
#[proc_macro_attribute]
pub fn lsp_request(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(item as syn::ItemStruct);
    request::expand(attr.into(), &item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
