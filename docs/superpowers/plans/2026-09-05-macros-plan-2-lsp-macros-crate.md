# Macros & Structure — Plan 2: The `lsp_macros` Crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the five procedural macros (`#[lsp_request]`, `lsp_dispatch!`, `lsp_method!`, `lsp_resolve_method!`, `conversion_tests!`) as plain-function-backed proc macros with span-accurate errors, prove each in a production position (the hover vertical slice), and run the informational rust-analyzer spike.

**Architecture:** `macros/src/` holds one module per macro family (`trait_method.rs`, `request.rs`, `dispatch.rs`, `conversion_tests.rs`) exposing ordinary `fn … -> syn::Result<TokenStream2>` functions — directly unit-testable — while `lib.rs` holds only the five thin `#[proc_macro]`/`#[proc_macro_attribute]` entrypoints (proc-macro definitions must live at the crate root). Generated code uses call-site `crate::` paths that resolve inside the main crate; every parse failure becomes a `syn::Error` on the offending token. The plan ends with hover migrated end-to-end (registry row removed, attribute + `lsp_method!` + one `lsp_dispatch!` row in place) — the same surgery Plan 3 repeats for the remaining 47 requests.

**Tech Stack:** syn 3.0.4 (`full`), quote 1.0.47, `syn::parse::Parse` implementations, `TraitItemFn`, `parse_quote!`.

**Spec:** `docs/superpowers/specs/2026-09-04-macros-and-structure-design.md` — decisions 1, 2, 4 (probe + fallback), 5, 7; sections "The `#[lsp_request]` attribute", "`lsp_dispatch!`", "The `Server` trait (T4)", "Testing", "Spike D3".

## Global Constraints

- The owner commits; the agent never runs git write commands, and NEVER dispatches subagents with worktree isolation — all work happens in the current branch's working tree.
- Behavior-identical migration: every generated body must be line-for-line today's expansion (the `cargo expand` diff in Task 5 checks it). Zero public-API change (Plan 1 already landed the one approved breaking change).
- Errors: `syn::Error` with the span of the offending token → `to_compile_error()`. No `panic!`/`expect`/`unwrap` in `lsp_macros` production code (`expect_used`/`unwrap_used` are `deny` via `[workspace.lints]`; allowed in `#[cfg(test)]` per `clippy.toml`).
- Generated code references call-site paths (`crate::requests::Request`, `crate::server::ServerState`, …) — never `$crate` (proc macros have none) and never `lsp_macros::` items.
- Docs on every public macro (`missing_docs` is deny). English only.
- Battery (both crates): `cargo build --all-targets`, `cargo test` ×3 feature configurations, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Single-macro smoke checks may use `-p lsp_macros` / `-p async-language-server` scoping; every task ends battery-green for what it touched, Task 5 ends with the full battery.
- On any failure: invoke `superpowers:systematic-debugging` and `no-workarounds`; fix root causes, never suppress.
- The dupes gate stays quiet in this plan by construction (macro invocations are opaque nodes); if `cargo dupes check` is run and a group appears, the avoidance analysis rule applies (memory + spec decision 4).

## File Structure

| file | responsibility |
|---|---|
| `macros/src/lib.rs` | the five proc-macro entrypoints, each `match … { Ok → tokens, Err → compile_error }`; module declarations; crate docs |
| `macros/src/trait_method.rs` | `lsp_method!` / `lsp_resolve_method!`: parse a bodiless `TraitItemFn`, append the default body |
| `macros/src/request.rs` | `#[lsp_request]`: DSL parsing (NameValue + list metas), field-path extraction, `impl Request` emission |
| `macros/src/dispatch.rs` | `lsp_dispatch!`: row grammar (`a: b @ Path`, `resolve(a: b @ Path)`), the two engine emitters |
| `macros/src/conversion_tests.rs` | `conversion_tests!`: row grammar of today's `macro_rules!`, `#[test]` emission |
| `Cargo.toml` (root) | `lsp_macros` path dependency (workspace + member entries) |
| `src/testing.rs` | definition replaced by a re-export of `lsp_macros::conversion_tests` |
| `src/requests/hover.rs`, `src/requests/registry.rs`, `src/requests/mod.rs`, `src/server/server_trait.rs`, `src/server/with_state/mod.rs` | the hover vertical slice (Task 5) |

---

### Task 1: `lsp_method!` / `lsp_resolve_method!` + the trait-position probe

**Files:**
- Modify: `Cargo.toml` (workspace + member entry for `lsp_macros`)
- Rewrite: `macros/src/lib.rs` (crate docs, module wiring, two entrypoints)
- Create: `macros/src/trait_method.rs`

**Interfaces:**
- Consumes: Plan 1's manifests.
- Produces: `lsp_macros::lsp_method` and `lsp_macros::lsp_resolve_method` (function-like proc macros); internally `trait_method::expand(item: TraitItemFn, kind) -> syn::Result<TokenStream2>`. Task 5 places the first production invocation; Plan 3 stamps 48. **The probe in Step 4 decides the spec's T4-vs-fallback question — its outcome MUST be reported in the task report.**

- [ ] **Step 1: Wire the dependency**

Root `Cargo.toml` — in `[workspace.dependencies]` (after the `tree-sitter-json` line, before the `arch-lint` comment block or adjacent to the member-local note):

```toml
lsp_macros = { path = "macros" }
```

In `[dependencies]` (after `globset`/alphabetical position — `lsp_macros` sorts between `ignore` and `ropey`):

```toml
lsp_macros = { workspace = true }
```

- [ ] **Step 2: Write `macros/src/trait_method.rs`**

```rust
//! `lsp_method!` / `lsp_resolve_method!` — append a `Server`-trait method's
//! default body to a bodiless declaration, leaving every written token
//! (docs, attributes, signature) untouched (`macro-no-rewrite-item`).

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Expr, Ident, TraitItemFn, parse_quote};

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
pub(super) fn expand(mut item: TraitItemFn, kind: Kind) -> syn::Result<TokenStream2> {
    if let Some(default) = item.default.as_ref() {
        return Err(syn::Error::new(
            default
                .brace_token
                .span
                .join()
                .unwrap_or_else(proc_macro2::Span::call_site),
            "expected a bodiless declaration; the macro appends the default body",
        ));
    }
    let body = match kind {
        Kind::NotImplemented => {
            let name = &item.sig.ident;
            parse_quote! { method_not_implemented(stringify!(#name)) }
        }
        Kind::ResolveUnchanged => {
            let ident = last_param_ident(&item)?;
            parse_quote! { async move { Ok(#ident) } }
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
            item.sig
                .paren_token
                .span
                .join()
                .unwrap_or_else(proc_macro2::Span::call_site),
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
```

(``Expr`` is unused here — drop it from the import list if clippy says so; the import shown is the conservative set.)

- [ ] **Step 3: Rewrite `macros/src/lib.rs`**

```rust
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

mod dispatch;
mod request;
mod trait_method;
mod conversion_tests;

use proc_macro::TokenStream;

/// Appends the `METHOD_NOT_FOUND` default body to a bodiless `Server`
/// trait-method declaration.
///
/// The written item — doc comments, attributes, signature — is re-emitted
/// unchanged; only the default body is generated. Use inside `trait Server`
/// for each of the 42 non-resolve request methods:
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
    match syn::parse_macro_input!(input as syn::TraitItemFn) {
        item => match trait_method::expand(item, trait_method::Kind::NotImplemented) {
            Ok(tokens) => tokens.into(),
            Err(error) => error.to_compile_error().into(),
        },
    }
}

/// Appends the resolve default body (`Ok(item)`) to a bodiless `Server`
/// trait-method declaration; the final named parameter is returned
/// unchanged.
#[proc_macro]
pub fn lsp_resolve_method(input: TokenStream) -> TokenStream {
    match syn::parse_macro_input!(input as syn::TraitItemFn) {
        item => match trait_method::expand(item, trait_method::Kind::ResolveUnchanged) {
            Ok(tokens) => tokens.into(),
            Err(error) => error.to_compile_error().into(),
        },
    }
}
```

(`mod dispatch; mod request; mod conversion_tests;` do not exist yet — create them as empty placeholder modules NOW to keep lib.rs final from this task on:

```rust
// placeholders, filled by Tasks 2–4
mod conversion_tests {}
mod dispatch {}
mod request {}
```

and delete the braces when the real module files arrive. Alternatively declare each `mod` in its own task — either way lib.rs compiles after every task.)

- [ ] **Step 4: The trait-position probe (the plan's first gate)**

In the main crate, add a temporary probe test to `src/requests/mod.rs`'s `#[cfg(test)] mod tests` (create the module if absent; **delete the probe before the task's checkpoint** — it exists only to answer the position question):

```rust
    #[test]
    fn nothing() {}

    mod probe {
        lsp_macros::lsp_method! {
            fn probe_only(&self, _state: crate::server::ServerState)
                -> impl std::future::Future<Output = crate::error::ServerResult<()>> + Send;
        }

        impl ProbeHost for () {
            fn method_not_implemented(name: &'static str) {}
        }
        trait ProbeHost {}
    }
```

(The probe only needs to COMPILE — a function-like proc macro invoked in trait-item position. Adapt the scaffolding as needed; what matters is `lsp_method!` sitting inside a `trait` body. `method_not_implemented` does not resolve in the probe — if compilation reaches name resolution you may stub it in the probe module; unresolved-name errors mean the POSITION was accepted, which is the finding.)

Run: `cargo check -p async-language-server`
- **Accepted** (any error is about `method_not_implemented` or types, not "expected item / macros cannot expand here"): T4 confirmed — record in the report, delete the probe, continue.
- **Rejected** (a syntax/expansion error pointing at the invocation inside the trait): the spec's documented fallback applies — T2 (plain hand-written trait methods + exactly one deliberate-parallel-family `.dupes-ignore.toml` entry in Plan 3). Record, delete the probe, and SKIP every `lsp_method!`/`lsp_resolve_method!` placement in this plan's Task 5 and in Plan 3 (the rest of this plan is unaffected).

- [ ] **Step 5: Unit tests (create `macros/src/trait_method.rs` `#[cfg(test)]` block)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

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
        assert!(text.contains("stringify! (hover)"));
        assert!(text.contains("doc =")); // docs survive as attributes
    }

    #[test]
    fn resolve_appends_ok_of_last_param() {
        let item = parse(quote! {
            fn completion_resolve(&self, _state: ServerState, item: CompletionItem)
                -> impl Future<Output = ServerResult<CompletionItem>> + Send;
        });
        let out = expand(item, Kind::ResolveUnchanged).expect("expands");
        assert!(out.to_string().contains("async move { Ok(item) }"));
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
}
```

(Adjust assertion details to what `to_string()` actually renders — e.g. match on `stringify` and the ident rather than exact spacing.)

Run: `cargo test -p lsp_macros`
Expected: 4 passing.

- [ ] **Step 6: Lint, format, doc the crate**

Run: `cargo clippy -p lsp_macros --all-targets -- -D warnings && cargo fmt --check && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p lsp_macros`
Expected: clean (the unused `Expr` import from Step 2, if flagged, is removed).

- [ ] **Step 7: Checkpoint (owner commits)**

Changed files: `Cargo.toml`, `Cargo.lock`, `macros/src/lib.rs`, `macros/src/trait_method.rs`. **The report states the probe verdict.**

---

### Task 2: `#[lsp_request]` — the registration attribute

**Files:**
- Modify: `macros/src/lib.rs` (entrypoint; `mod request;` real)
- Create: `macros/src/request.rs`

**Interfaces:**
- Consumes: Task 1's lib skeleton.
- Produces: `lsp_macros::lsp_request`; internally `request::expand(attr: TokenStream2, item: ItemStruct) -> syn::Result<TokenStream2>` and `request::field_path(expr: &Expr) -> syn::Result<Vec<Ident>>` (Task 3 does not need these; they are this module's tested surface). Task 5 places the first production invocation; Plan 3 stamps 48.

- [ ] **Step 1: Write `macros/src/request.rs`**

```rust
//! `#[lsp_request]` — per-file registration of one LSP request.
//!
//! DSL: `params`/`response` are NameValue metas carrying type paths;
//! every hook is a list meta carrying one field path (document,
//! incoming_position, incoming_range) or one function path (incoming_custom,
//! outgoing, incoming_standalone, outgoing_standalone). Unspecified hooks
//! keep the `Request` trait's delegating defaults.

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{Expr, Ident, ItemStruct, Meta, Path, Type, punctuated::Punctuated};

/// Parsed attribute content for one request.
pub(super) struct RequestSpec {
    params: Type,
    response: Type,
    document: Option<Vec<Ident>>,
    incoming_position: Option<Vec<Ident>>,
    incoming_range: Option<Vec<Ident>>,
    incoming_custom: Option<Path>,
    outgoing: Option<Path>,
    incoming_standalone: Option<Path>,
    outgoing_standalone: Option<Path>,
}

/// Expands `#[lsp_request(...)]` on a unit struct into the struct plus its
/// `impl Request`.
///
/// # Errors
///
/// Spanned errors for: unknown fields, missing `params`/`response`,
/// malformed paths, duplicate hook kinds, generics on the struct.
pub(super) fn expand(attr: TokenStream2, item: ItemStruct) -> syn::Result<TokenStream2> {
    let spec = parse_spec(attr)?;
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "request marker structs are non-generic unit structs",
        ));
    }
    let name = &item.ident;
    let RequestSpec {
        params,
        response,
        document,
        incoming_position,
        incoming_range,
        incoming_custom,
        outgoing,
        incoming_standalone,
        outgoing_standalone,
    } = spec;

    let extract_url = document.map(|segs| {
        quote! {
            fn extract_url(params: &Self::Params) -> Option<::async_lsp::lsp_types::Url> {
                ::core::option::Option::Some(params.#(#segs).*.uri.clone())
            }
        }
    });
    let has_incoming = incoming_position.is_some()
        || incoming_range.is_some()
        || incoming_custom.is_some();
    let standard_incoming = incoming_position
        .map(|segs| {
            quote! { convert_position(state, document, &mut params.#(#segs).*, Direction::Incoming); }
        })
        .into_iter()
        .chain(incoming_range.map(|segs| {
            quote! { convert_range(state, document, &mut params.#(#segs).*, Direction::Incoming); }
        }));
    let custom_incoming = incoming_custom.map(|fun| {
        quote! { #fun(state, document, params); }
    });
    let modify_params = has_incoming.then(|| {
            let standard = standard_incoming;
            let custom = custom_incoming.into_iter();
            quote! {
                fn modify_params(
                    state: &crate::server::ServerState,
                    document: &crate::server::Document,
                    params: &mut Self::Params,
                ) {
                    use crate::requests::conversion::{Direction, convert_position, convert_range};
                    #(#standard)*
                    #(#custom)*
                }
            }
        });
    let modify_response = outgoing.map(|fun| {
        quote! {
            fn modify_response(
                state: &crate::server::ServerState,
                document: &crate::server::Document,
                response: &mut Self::Response,
            ) {
                #fun(state, document, response);
            }
        }
    });
    let modify_params_standalone = incoming_standalone.map(|fun| {
        quote! {
            fn modify_params_standalone(state: &crate::server::ServerState, params: &mut Self::Params) {
                #fun(state, params);
            }
        }
    });
    let modify_response_standalone = outgoing_standalone.map(|fun| {
        quote! {
            fn modify_response_standalone(state: &crate::server::ServerState, response: &mut Self::Response) {
                #fun(state, response);
            }
        }
    });

    Ok(quote! {
        #item

        impl crate::requests::Request for #name {
            type Params = #params;
            type Response = #response;

            #extract_url
            #modify_params
            #modify_response
            #modify_params_standalone
            #modify_response_standalone
        }
    })
}

fn parse_spec(attr: TokenStream2) -> syn::Result<RequestSpec> {
    let metas = syn::parse2::<Punctuated<Meta, syn::Token![,]>>(attr)?;
    let mut params = None;
    let mut response = None;
    let mut document = None;
    let mut incoming_position = None;
    let mut incoming_range = None;
    let mut incoming_custom = None;
    let mut outgoing = None;
    let mut incoming_standalone = None;
    let mut outgoing_standalone = None;

    fn set<T>(slot: &mut Option<T>, value: T, meta: &Meta) -> syn::Result<()> {
        if slot.is_some() {
            Err(syn::Error::new_spanned(meta, "duplicate field"))
        } else {
            *slot = Some(value);
            Ok(())
        }
    }

    for meta in metas {
        let ident = meta
            .path()
            .get_ident()
            .cloned()
            .ok_or_else(|| syn::Error::new_spanned(meta.path(), "expected a single-ident field name"))?;
        match ident.to_string().as_str() {
            "params" => {
                let Meta::NameValue(value) = &meta else {
                    return Err(syn::Error::new_spanned(&meta, "`params` takes a type: params = <type path>"));
                };
                set(&mut params, parse_type(value)?, &meta)?;
            }
            "response" => {
                let Meta::NameValue(value) = &meta else {
                    return Err(syn::Error::new_spanned(&meta, "`response` takes a type: response = <type path>"));
                };
                set(&mut response, parse_type(value)?, &meta)?;
            }
            "document" | "incoming_position" | "incoming_range" => {
                let Meta::List(list) = &meta else {
                    return Err(syn::Error::new_spanned(&meta, format!("`{ident}` takes a parenthesized field path")));
                };
                let expr: Expr = syn::parse2(list.tokens.clone())?;
                let segs = field_path(&expr)?;
                match ident.to_string().as_str() {
                    "document" => set(&mut document, segs, &meta)?,
                    "incoming_position" => set(&mut incoming_position, segs, &meta)?,
                    _ => set(&mut incoming_range, segs, &meta)?,
                }
            }
            "incoming_custom" | "outgoing" | "incoming_standalone" | "outgoing_standalone" => {
                let Meta::List(list) = &meta else {
                    return Err(syn::Error::new_spanned(&meta, format!("`{ident}` takes a parenthesized function path")));
                };
                let path: Path = syn::parse2(list.tokens.clone())?;
                match ident.to_string().as_str() {
                    "incoming_custom" => set(&mut incoming_custom, path, &meta)?,
                    "outgoing" => set(&mut outgoing, path, &meta)?,
                    "incoming_standalone" => set(&mut incoming_standalone, path, &meta)?,
                    _ => set(&mut outgoing_standalone, path, &meta)?,
                }
            }
            other => {
                return Err(syn::Error::new_spanned(&meta, format!("unknown or malformed lsp_request field `{other}`")))
            }
        }
    }

    let params = params.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `params = <type>`"))?;
    let response = response.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `response = <type>`"))?;
    Ok(RequestSpec { params, response, document, incoming_position, incoming_range, incoming_custom, outgoing, incoming_standalone, outgoing_standalone })
}

/// Re-parses a NameValue meta's value tokens as a type: attribute values
/// are expressions to syn (`Option<X>` would parse as comparisons).
fn parse_type(value: &syn::MetaNameValue) -> syn::Result<Type> {
    syn::parse2(value.value.to_token_stream())
}

/// Extracts the dotted identifier chain of a field-path expression
/// (`a.b.c`), erroring spanned on anything else.
pub(super) fn field_path(expr: &Expr) -> syn::Result<Vec<Ident>> {
    match expr {
        Expr::Field(field) => {
            let mut segments = field_path(&field.base)?;
            match &field.member {
                syn::Member::Named(ident) => segments.push(ident.clone()),
                syn::Member::Unnamed(index) => {
                    return Err(syn::Error::new_spanned(index, "expected named fields, not tuple indices"))
                }
            }
            Ok(segments)
        }
        Expr::Path(path) if path.path.segments.len() == 1 && path.path.segments[0].arguments.is_none() => {
            Ok(vec![path.path.segments[0].ident.clone()])
        }
        other => Err(syn::Error::new_spanned(other, "expected a field path like `text_document_position_params.position`")),
    }
}
```

(Two subtleties are load-bearing. `parse_type` re-parses the meta value's tokens as a `Type` because syn reads attribute values as expressions — `Option<Hover>` would otherwise parse as a comparison chain. And the `use crate::requests::conversion::{…}` inside the emitted `modify_params` keeps the generated body's unqualified `convert_position`/`convert_range`/`Direction` references resolving at the call site.)

- [ ] **Step 2: Entrypoint in `macros/src/lib.rs`**

```rust
/// Registers one LSP request on a unit struct, stamping its `impl Request`.
///
/// ```ignore
/// #[lsp_request(
///     params = async_lsp::lsp_types::HoverParams,
///     response = Option<async_lsp::lsp_types::Hover>,
///     document(text_document_position_params.text_document),
///     incoming_position(text_document_position_params.position),
///     outgoing(crate::requests::conversion::modify_outgoing_hover),
/// )]
/// pub struct HoverRequest;
/// ```
#[proc_macro_attribute]
pub fn lsp_request(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(item as syn::ItemStruct);
    match request::expand(attr.into(), item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
```

- [ ] **Step 3: Unit tests (append to `request.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn field_path_extracts_dotted_chain() {
        let expr = syn::parse2(quote! { a.b.c }).expect("parses");
        assert_eq!(
            field_path(&expr).expect("path"),
            ["a", "b", "c"].map(|s| syn::Ident::new(s, proc_macro2::Span::call_site()))
        );
    }

    #[test]
    fn field_path_rejects_calls() {
        let expr = syn::parse2(quote! { a.b(c) }).expect("parses");
        assert!(field_path(&expr).is_err());
    }

    #[test]
    fn minimal_attribute_emits_struct_and_impl() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let out = expand(quote! {
            params = P,
            response = Option<R>,
        }, item).expect("expands");
        let text = out.to_string();
        assert!(text.contains("impl crate :: requests :: Request for X"));
        assert!(text.contains("type Params = P"));
        assert!(text.contains("type Response = Option < R >"));
        assert!(!text.contains("extract_url"));
    }

    #[test]
    fn full_wiring_emits_every_hook() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let out = expand(quote! {
            params = P,
            response = R,
            document(a.b),
            incoming_position(a.c),
            outgoing(f),
            incoming_standalone(g),
            outgoing_standalone(h),
        }, item).expect("expands");
        let text = out.to_string();
        for needle in ["extract_url", "modify_params", "modify_response", "modify_params_standalone", "modify_response_standalone"] {
            assert!(text.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn unknown_field_is_spanned_error() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let err = expand(quote! { params = P, response = R, bogus(x) }, item).expect_err("rejected");
        assert!(err.to_string().contains("unknown or malformed"));
    }

    #[test]
    fn missing_params_is_error() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        assert!(expand(quote! { response = R }, item).is_err());
    }
}
```

Run: `cargo test -p lsp_macros`
Expected: all green (Task 1's four plus these six).

- [ ] **Step 4: Dogfood — a real conversion test through the attribute**

In `src/requests/mod.rs`'s `#[cfg(test)] mod tests` (the module from Task 1; the probe is gone), add:

```rust
    mod dogfood {
        use crate::requests::Request;
        use crate::testing::state_with_documents;

        #[lsp_macros::lsp_request(
            params = async_lsp::lsp_types::HoverParams,
            response = Option<async_lsp::lsp_types::Hover>,
            document(text_document_position_params.text_document),
            incoming_position(text_document_position_params.position),
        )]
        pub(crate) struct DogfoodRequest;

        #[test]
        fn dogfood_request_converts_incoming_position() {
            let (state, _plain, emoji) = state_with_documents();
            let document = state.document(&emoji).expect("tracked");
            let mut params = async_lsp::lsp_types::HoverParams {
                text_document_position_params:
                    async_lsp::lsp_types::TextDocumentPositionParams::new(
                        async_lsp::lsp_types::TextDocumentIdentifier::new(emoji),
                        crate::testing::line_position(0, 2),
                    ),
                work_done_progress_params: Default::default(),
            };
            <DogfoodRequest as Request>::modify_params(&state, &document, &mut params);
            assert_eq!(
                params.text_document_position_params.position,
                crate::testing::line_position(0, 4),
            );
        }
    }
```

Run: `cargo test -p async-language-server dogfood`
Expected: PASS (the emoji fixture converts UTF-16 column 2 → UTF-8 byte column 4 through the attribute-stamped hook — the load-bearing identity).

- [ ] **Step 5: Clippy/fmt/doc + checkpoint**

Run: `cargo clippy -p lsp_macros --all-targets -- -D warnings && cargo fmt --check && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p lsp_macros`

Changed files: `macros/src/lib.rs`, `macros/src/request.rs`, `src/requests/mod.rs` (dogfood test module stays until Plan 3 replaces it with real requests).

---

### Task 3: `lsp_dispatch!` — the dispatch engines

**Files:**
- Modify: `macros/src/lib.rs` (entrypoint)
- Create: `macros/src/dispatch.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1–2 beyond lib wiring.
- Produces: `lsp_macros::lsp_dispatch`; internally `dispatch::DispatchRow { trait_method: Ident, alsp: Ident, request: Path, resolve: bool }` with `syn::parse::Parse`, and emitters `dispatch::method(row) -> TokenStream2`, `dispatch::resolve_method(row) -> TokenStream2`. Task 5 places the first production invocation; Plan 3 stamps the full table. **The emitted bodies are line-for-line today's `implement_method!` / `implement_resolve_method!` expansions — verified by the Task 5 `cargo expand` diff.**

- [ ] **Step 1: Write `macros/src/dispatch.rs`**

```rust
//! `lsp_dispatch!` — stamp the `LanguageServer` dispatch methods for the
//! request table: one row per method, `resolve(...)` rows for the resolve
//! family. The engines are the line-for-line successors of the former
//! `implement_method!` / `implement_resolve_method!` macro_rules bodies.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, Path, Token, parse::Parse};

pub(super) struct DispatchRow {
    /// Our `Server` trait method (called on the server).
    pub(super) trait_method: Ident,
    /// The async-lsp `LanguageServer` method (the generated fn's name).
    pub(super) alsp: Ident,
    /// The request marker type (full path).
    pub(super) request: Path,
    /// Resolve-family row.
    pub(super) resolve: bool,
}

impl Parse for DispatchRow {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let trait_method: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;
        let alsp: Ident = input.parse()?;
        let _: Token![@] = input.parse()?;
        let request: Path = input.parse()?;
        Ok(DispatchRow { trait_method, alsp, request, resolve: false })
    }
}

struct ResolveRow(DispatchRow);

impl Parse for ResolveRow {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let kw: Ident = input.parse()?;
        if kw != "resolve" {
            return Err(syn::Error::new_spanned(kw, "expected `resolve`"));
        }
        let content;
        syn::parenthesized!(content in input);
        let mut row: DispatchRow = content.parse()?;
        row.resolve = true;
        Ok(ResolveRow(row))
    }
}

pub(super) fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let rows = syn::parse2::<DispatchTable>(input)?.0;
    let methods = rows.iter().map(|row| {
        if row.resolve { resolve_method(row) } else { method(row) }
    });
    Ok(quote! { #(#methods)* })
}

struct DispatchTable(Vec<DispatchRow>);

impl Parse for DispatchTable {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut rows = Vec::new();
        while !input.is_empty() {
            let row = if is_resolve_row(input) {
                input.parse::<ResolveRow>()?.0
            } else {
                input.parse::<DispatchRow>()?
            };
            rows.push(row);
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            } else {
                break;
            }
        }
        Ok(DispatchTable(rows))
    }
}

/// Probes whether the next tokens are the ident `resolve` followed by a
/// parenthesized group — without consuming anything on `false` (fork and
/// advance the fork only).
fn is_resolve_row(input: &syn::parse::ParseStream<'_>) -> bool {
    let fork = input.fork();
    let Ok(ident) = fork.parse::<Ident>() else {
        return false;
    };
    if ident != "resolve" {
        return false;
    }
    fork.parse::<proc_macro2::Group>().is_ok()
}

/// The URL-anchored engine (42 normal rows) — today's `implement_method!`.
pub(super) fn method(row: &DispatchRow) -> TokenStream2 {
    let DispatchRow { trait_method, alsp, request, .. } = row;
    quote! {
        fn #alsp(
            &mut self,
            mut params: <#request as crate::requests::Request>::Params,
        ) -> BoxFuture<
            'static,
            Result<<#request as crate::requests::Request>::Response, Self::Error>,
        > {
            let server = Arc::clone(&self.server);
            let state = self.state.clone();
            Box::pin(async move {
                // 1. Try to extract the URL from the params for document tracking
                let url: Option<Url> =
                    <#request as crate::requests::Request>::extract_url(&params);
                let mut ver: Option<i32> = None;

                // 2. If we got an URL, track the document version
                if let Some(url) = url.as_ref()
                    && let Some(doc) = state.document(url)
                {
                    ver.replace(doc.version());
                }

                // 3. Call the "modify params" callback against the request's
                //    conversion document: the tracked snapshot for a tracked
                //    URL, a disk snapshot for an untracked file URL, or the
                //    sole tracked document for URL-less requests
                let params_doc = conversion_document(&state, url.as_ref());
                if let Some(doc) = params_doc.as_ref() {
                    <#request as crate::requests::Request>::modify_params(
                        &state,
                        doc,
                        &mut params,
                    );
                }

                // 4. Call the user-defined language server function
                let mut result = server
                    .#trait_method(state.clone(), params)
                    .await?;

                // 5. Check our document again, if we had one originally. If the
                //    version changed, our result is stale, and we should try again
                if let Some(url) = url.as_ref()
                    && let Some(doc) = state.document(url)
                    && ver.is_some_and(|v| v != doc.version())
                {
                    return Err(ResponseError::new(
                        ErrorCode::CONTENT_MODIFIED,
                        "document was modified during processing",
                    ));
                }

                // 6. Run the final "modify response" callback against a freshly
                //    resolved conversion document; when none resolves, the
                //    standalone hook runs state-driven conversions instead of
                //    skipping them.
                match conversion_document(&state, url.as_ref()) {
                    Some(doc) => {
                        <#request as crate::requests::Request>::modify_response(
                            &state,
                            &doc,
                            &mut result,
                        );
                    }
                    None => {
                        <#request as crate::requests::Request>::modify_response_standalone(
                            &state,
                            &mut result,
                        );
                    }
                }

                Ok(result)
            })
        }
    }
}

/// The sole-document engine (6 resolve rows) — today's `implement_resolve_method!`.
pub(super) fn resolve_method(row: &DispatchRow) -> TokenStream2 {
    let DispatchRow { trait_method, alsp, request, .. } = row;
    quote! {
        fn #alsp(
            &mut self,
            mut params: <#request as crate::requests::Request>::Params,
        ) -> BoxFuture<
            'static,
            Result<<#request as crate::requests::Request>::Response, Self::Error>,
        > {
            let server = Arc::clone(&self.server);
            let state = self.state.clone();
            Box::pin(async move {
                // Resolve requests carry no text-document URL: convert against
                // the sole tracked document, if the server tracks exactly one;
                // with no sole document, the standalone hooks run state-driven
                // conversions instead of skipping them.
                let sole = conversion_document(&state, None);
                match sole.as_ref() {
                    Some(document) => {
                        convert_resolve_item::<#request, _>(
                            &state,
                            Some(document),
                            &mut params,
                            Direction::Incoming,
                        );
                    }
                    None => {
                        <#request as crate::requests::Request>::modify_params_standalone(
                            &state,
                            &mut params,
                        );
                    }
                }
                let mut result = server.#trait_method(state.clone(), params).await?;
                match sole.as_ref() {
                    Some(document) => {
                        convert_resolve_item::<#request, _>(
                            &state,
                            Some(document),
                            &mut result,
                            Direction::Outgoing,
                        );
                    }
                    None => {
                        <#request as crate::requests::Request>::modify_response_standalone(
                            &state,
                            &mut result,
                        );
                    }
                }
                Ok(result)
            })
        }
    }
}
```

(The `is_resolve_row` fork-probe is the load-bearing detail: it must not consume tokens when returning `false`, so plain rows still parse. A row whose first ident is literally `resolve` cannot occur — the five name-divergent methods are `rename_prepare`, `document_format`, `document_range_format`, `document_diagnostics`, `link`, none colliding.)

- [ ] **Step 2: Entrypoint in `macros/src/lib.rs`**

```rust
/// Stamps `LanguageServer` dispatch methods from the request table.
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
/// rows wrap the same triple in `resolve(...)`.
#[proc_macro]
pub fn lsp_dispatch(input: TokenStream) -> TokenStream {
    match dispatch::expand(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
```

- [ ] **Step 3: Unit tests (append to `dispatch.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn row(tokens: proc_macro2::TokenStream) -> DispatchRow {
        syn::parse2(tokens).expect("row parses")
    }

    #[test]
    fn parses_plain_row() {
        let r = row(quote! { hover: hover @ crate::requests::HoverRequest });
        assert_eq!(r.trait_method, "hover");
        assert_eq!(r.alsp, "hover");
        assert_eq!(r.request.to_string(), "crate :: requests :: HoverRequest");
        assert!(!r.resolve);
    }

    #[test]
    fn parses_diverging_names() {
        let r = row(quote! { rename_prepare: prepare_rename @ crate::requests::RenamePrepareRequest });
        assert_eq!(r.trait_method, "rename_prepare");
        assert_eq!(r.alsp, "prepare_rename");
    }

    #[test]
    fn method_emits_engine_skeleton() {
        let r = row(quote! { hover: hover @ R });
        let text = method(&r).to_string();
        for needle in [
            "fn hover",
            "extract_url",
            "conversion_document",
            "CONTENT_MODIFIED",
            "modify_response_standalone",
            ".hover (state . clone () , params)",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn resolve_engine_uses_sole_document_path() {
        let mut r = row(quote! { completion_resolve: completion_resolve @ R });
        r.resolve = true;
        let text = resolve_method(&r).to_string();
        assert!(text.contains("convert_resolve_item"));
        assert!(text.contains("Direction :: Incoming"));
        assert!(!text.contains("CONTENT_MODIFIED"));
    }

    #[test]
    fn table_parses_mixed_rows_and_trailing_comma() {
        let table: DispatchTable = syn::parse2(quote! {
            hover: hover @ A,
            resolve(r: r @ B),
        }).expect("table parses");
        assert_eq!(table.0.len(), 2);
        assert!(table.0[1].resolve);
    }

    #[test]
    fn rejects_row_missing_at() {
        assert!(syn::parse2::<DispatchRow>(quote! { hover: hover A }).is_err());
    }
}
```

(Exact `to_string()` fragment assertions may need spacing adjustments — `A :: B` vs `A::B`; assert on stable substrings like `extract_url` and `CONTENT_MODIFIED` first, tighten the rest to what is actually rendered.)

Run: `cargo test -p lsp_macros`
Expected: all green.

- [ ] **Step 4: Clippy/fmt/doc + checkpoint**

Run: `cargo clippy -p lsp_macros --all-targets -- -D warnings && cargo fmt --check && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p lsp_macros`

Changed files: `macros/src/lib.rs`, `macros/src/dispatch.rs`.

---

### Task 4: `conversion_tests!` — the test harness moves

**Files:**
- Modify: `macros/src/lib.rs` (entrypoint)
- Create: `macros/src/conversion_tests.rs`
- Modify: `src/testing.rs` (the `macro_rules!` definition replaced by a re-export)

**Interfaces:**
- Consumes: nothing new.
- Produces: `lsp_macros::conversion_tests` with TODAY'S call syntax and TODAY'S expansion (all ~40 existing call sites stay byte-identical). The re-export in `src/testing.rs` keeps `use crate::testing::conversion_tests;` working unchanged.

- [ ] **Step 1: Write `macros/src/conversion_tests.rs`**

```rust
//! `conversion_tests!` — stamp one `#[test]` per row for a request's
//! conversion hooks. The W0 table harness; grammar and expansion are the
//! 2026-09-01 `macro_rules!` verbatim, with `$crate` replaced by
//! call-site `crate::` paths.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Expr, Ident, Token, Type, parse::Parse};

struct TestRow {
    name: Ident,
    request: Type,
    params: Expr,
    incoming: Option<(Expr, Expr)>,
    response: Option<(Expr, Expr, Expr)>,
}

impl Parse for TestRow {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;
        let request: Type = input.parse()?;
        let content;
        syn::braced!(content in input);
        // `params : Expr` — syn cannot know the key; parse ident, colon, expr.
        let params_key: Ident = content.parse()?;
        if params_key != "params" {
            return Err(syn::Error::new_spanned(params_key, "expected `params`"));
        }
        let _: Token![:] = content.parse()?;
        let params: Expr = content.parse()?;
        let mut incoming = None;
        let mut response = None;
        while content.peek(Token![,]) {
            let _: Token![,] = content.parse()?;
            if !content.peek(Ident) { break; }
            let key: Ident = content.parse()?;
            let _: Token![:] = content.parse()?;
            match key.to_string().as_str() {
                "incoming" => {
                    let incoming_expr: Expr = content.parse()?;
                    let _: Token![,] = content.parse()?;
                    let expects_key: Ident = content.parse()?;
                    if expects_key != "expects" {
                        return Err(syn::Error::new_spanned(expects_key, "expected `expects` after `incoming`"));
                    }
                    let _: Token![:] = content.parse()?;
                    incoming = Some((incoming_expr, content.parse()?));
                }
                "response" => {
                    let response_expr: Expr = content.parse()?;
                    let _: Token![,] = content.parse()?;
                    let outgoing_key: Ident = content.parse()?;
                    if outgoing_key != "outgoing" {
                        return Err(syn::Error::new_spanned(outgoing_key, "expected `outgoing` after `response`"));
                    }
                    let _: Token![:] = content.parse()?;
                    let outgoing_expr: Expr = content.parse()?;
                    let _: Token![,] = content.parse()?;
                    let returns_key: Ident = content.parse()?;
                    if returns_key != "returns" {
                        return Err(syn::Error::new_spanned(returns_key, "expected `returns` after `outgoing`"));
                    }
                    let _: Token![:] = content.parse()?;
                    response = Some((response_expr, outgoing_expr, content.parse()?));
                }
                other => return Err(syn::Error::new_spanned(key, format!("unknown row field `{other}`"))),
            }
        }
        Ok(TestRow { name, request, params, incoming, response })
    }
}

struct TestTable(Vec<TestRow>);

impl Parse for TestTable {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut rows = Vec::new();
        while !input.is_empty() {
            rows.push(input.parse()?);
            if input.peek(Token![,]) { let _: Token![,] = input.parse()?; }
        }
        Ok(TestTable(rows))
    }
}

pub(super) fn expand(input: TokenStream2) -> syn::Result<TokenStream2> {
    let table = syn::parse2::<TestTable>(input)?;
    let tests = table.0.iter().map(|row| {
        let TestRow { name, request, params, incoming, response } = row;
        let incoming = incoming.map(|(extract, expects)| {
            quote! {
                crate::testing::assert_converted_position(
                    &params,
                    #extract,
                    #expects,
                    "incoming position must be converted to the UTF-8 byte column",
                );
            }
        });
        let response = response.map(|(build, extract, returns)| {
            quote! {
                let mut response = (#build)(_plain.clone(), emoji.clone());
                <#request as crate::requests::Request>::modify_response(&state, &document, &mut response);
                crate::testing::assert_converted_position(
                    &response,
                    #extract,
                    #returns,
                    "outgoing position must be converted to the client encoding",
                );
            }
        });
        quote! {
            #[test]
            fn #name() {
                let (state, _plain, emoji) = crate::testing::state_with_documents();
                let document = state.document(&emoji).expect("emoji document is tracked");
                let mut params = (#params)(emoji.clone());
                <#request as crate::requests::Request>::modify_params(&state, &document, &mut params);
                #incoming
                #response
            }
        }
    });
    Ok(quote! { #(#tests)* })
}
```

- [ ] **Step 2: Entrypoint in `macros/src/lib.rs`**

```rust
/// Stamps one `#[test]` per row for a [`crate::requests::Request`]'s
/// conversion hooks — the table-driven W0 harness.
///
/// Row grammar (both the `incoming`/`expects` pair and the
/// `response`/`outgoing`/`returns` triple are optional) — identical to the
/// harness this crate used as `macro_rules!` since 2026-09-01; see the
/// testing rule's "Adding a test" section for the fixture semantics
/// (`params` builds against the emoji document in the client encoding,
/// `expects` asserts the UTF-8 byte column after `modify_params`).
#[proc_macro]
pub fn conversion_tests(input: TokenStream) -> TokenStream {
    match conversion_tests::expand(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
```

- [ ] **Step 3: Swap the definition for a re-export in `src/testing.rs`**

Delete the whole `macro_rules! conversion_tests { … }` block and its `pub(crate) use conversion_tests;` line; in its place (same position, after `assert_converted_position`):

```rust
/// The W0 conversion-test harness, now a procedural macro in the
/// workspace `lsp_macros` crate; re-exported here so every call site's
/// `use crate::testing::conversion_tests;` keeps working.
pub(crate) use lsp_macros::conversion_tests;
```

- [ ] **Step 4: Integration dogfood — the entire existing suite**

Run: `cargo test && cargo test --no-default-features && cargo test --all-features`
Expected: green in all three configurations, identical test counts to Plan 1's baseline — every existing `conversion_tests!` call site now expands through the proc macro.

- [ ] **Step 5: Unit tests (append to `conversion_tests.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_row() {
        let table: TestTable = syn::parse2(quote::quote! {
            t: R { params: |uri| P::new(uri) }
        }).expect("parses");
        assert_eq!(table.0.len(), 1);
        assert!(table.0[0].incoming.is_none());
        assert!(table.0[0].response.is_none());
    }

    #[test]
    fn parses_full_row() {
        let table: TestTable = syn::parse2(quote::quote! {
            t: R { params: p, incoming: i, expects: e, response: r, outgoing: o, returns: x, }
        }).expect("parses");
        assert!(table.0[0].incoming.is_some());
        assert!(table.0[0].response.is_some());
    }

    #[test]
    fn emits_test_fn_with_fixtures() {
        let out = expand(quote::quote! {
            t: R { params: |uri| P::new(uri) }
        }).expect("expands");
        let text = out.to_string();
        assert!(text.contains("# [test]"));
        assert!(text.contains("state_with_documents"));
        assert!(text.contains("modify_params"));
    }

    #[test]
    fn unknown_field_is_error() {
        let err = syn::parse2::<TestTable>(quote::quote! {
            t: R { params: p, bogus: b }
        }).expect_err("rejected");
        assert!(err.to_string().contains("unknown row field"));
    }
}
```

Run: `cargo test -p lsp_macros`
Expected: green.

- [ ] **Step 6: Clippy/fmt/doc + checkpoint**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`

Changed files: `macros/src/lib.rs`, `macros/src/conversion_tests.rs`, `src/testing.rs`.

---

### Task 5: The hover vertical slice

**Files:**
- Modify: `src/requests/registry.rs` (delete the hover row)
- Rewrite: `src/requests/hover.rs` (struct + attribute; tests keep their rows)
- Modify: `src/server/server_trait.rs` (the `lsp_method!` block)
- Modify: `src/server/with_state/mod.rs` (the one-row `lsp_dispatch!` invocation)
- Modify: `src/requests/mod.rs` (re-export; remove the dogfood module)

**Interfaces:**
- Consumes: all four macros from Tasks 1–4; the probe verdict (if the probe REJECTED trait-position macros, this task writes hover's trait method by hand per the fallback and still uses `#[lsp_request]` + `lsp_dispatch!`).
- Produces: the exact three-place pattern Plan 3 repeats 47 times; `crate::requests::HoverRequest` as the first renamed marker struct.

- [ ] **Step 1: Delete the hover row from `registry.rs`**

Remove lines 22–29 of `src/requests/registry.rs` (the `hover: hover @ Hover { … }` row inside `generated_methods!`). Nothing else in the file changes.

- [ ] **Step 2: Rewrite `src/requests/hover.rs`**

```rust
#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::HoverParams,
    response = Option<async_lsp::lsp_types::Hover>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_hover),
)]
pub(crate) struct HoverRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        HoverContents, MarkupContent, MarkupKind, TextDocumentIdentifier,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };

    use crate::requests::HoverRequest;
    use crate::testing::{conversion_tests, line_position, same_line};

    conversion_tests! {
        hover_incoming_utf16_becomes_utf8: HoverRequest {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
        }
        hover_outgoing_utf8_becomes_utf16: HoverRequest {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(async_lsp::lsp_types::Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: "x".into(),
                }),
                range: Some(same_line(0, 4, 4)),
            }),
            outgoing: |r| r.as_ref().expect("hover present").range.expect("range present").start,
            returns: line_position(0, 2),
        }
    }
}
```

(The test rows are today's byte-for-byte, with the two `Hover` occurrences renamed to `HoverRequest`.)

- [ ] **Step 3: The trait method in `server_trait.rs`**

Add to the imports at the top: `use lsp_macros::lsp_method;`. Inside `pub trait Server`, immediately BEFORE the three registry invocations, place:

```rust
    lsp_method! {
        /// Handles `textDocument/hover` requests from the client.
        ///
        /// Returns hover contents for the position in `params`, or `None` when there is nothing to show. Positions and ranges are UTF-8. Requires a hover provider in [`Server::server_capabilities`].
        fn hover(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::HoverParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::Hover>>> + Send;
    }
```

(The doc string is the registry row's `doc:` value verbatim. If the Task 1 probe REJECTED trait-position macros, write the method by hand with the same doc and signature and the body `method_not_implemented(stringify!(hover))` — and note the fallback in the report.)

- [ ] **Step 4: The dispatch row in `with_state/mod.rs`**

Add to the imports: `use lsp_macros::lsp_dispatch;`. After the three registry invocations at the bottom of the `impl LanguageServer` block:

```rust
    lsp_dispatch! {
        hover: hover @ crate::requests::HoverRequest,
    }
```

- [ ] **Step 5: Re-export and cleanup in `src/requests/mod.rs`**

- Add `pub(crate) use hover::HoverRequest;` to the re-export list (alphabetical: after `DocumentLinkResolve`… place it among the `pub(crate) use` group where `hover` sorts).
- Delete the Task 2 dogfood module (`mod dogfood { … }` and the empty `#[test] fn nothing` scaffold from Task 1).

- [ ] **Step 6: Sweep remaining `Hover` references**

Run: `grep -rn '\bHover\b' src/ examples/`
Expected: only `HoverParams`/`HoverContents`/`Hover`-suffixed lsp_types names and comments remain; any bare `Hover` type reference (e.g. in `with_state/tests.rs` wire fixtures) becomes `HoverRequest`.

- [ ] **Step 7: Expansion equivalence (one-off)**

Run: `cargo expand -p async-language-server server::with_state > /tmp/after.rs` (and, from the pre-task tree, the same into `/tmp/before.rs`).
Expected: the dispatch method diff is empty modulo `Hover` → `HoverRequest` and attribute-vs-registry provenance of the `impl Request` block. Record the result in the report.

- [ ] **Step 8: Full battery**

```bash
cargo build --all-targets
cargo test
cargo test --no-default-features
cargo test --all-features
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```
Expected: green everywhere; the two hover conversion tests pass through the attribute-stamped impl.

- [ ] **Step 9: Checkpoint (owner commits)**

Changed files: `src/requests/registry.rs`, `src/requests/hover.rs`, `src/server/server_trait.rs`, `src/server/with_state/mod.rs`, `src/requests/mod.rs` (plus any Step 6 fix-ups).

---

### Task 6: Spike D3 — the informational rust-analyzer report

**Files:**
- No code. The task's deliverable is the checklist verdicts in the task report (the implementer records what the OWNER observes; if the implementer is a subagent, the controller collects the owner's answers first — this task is owner-paced).

**Interfaces:**
- Consumes: Task 5's migrated `hover.rs`, `server_trait.rs`, `with_state/mod.rs`.
- Produces: the DX expectations record; the D6 form confirmation (stay (b) unless findings flip it — flipping requires a follow-up design note, not a silent change).

- [ ] **Step 1: The owner opens the three files in their editor (Zed, rust-analyzer) and checks:**

1. `src/requests/hover.rs`, inside `#[lsp_request( … )]`: go-to-definition on `async_lsp::lsp_types::HoverParams`; go-to-definition and find-references on `crate::requests::conversion::modify_outgoing_hover`; completion after typing `incoming_`.
2. `src/server/server_trait.rs`, inside `lsp_method! { … }`: go-to-definition on `HoverParams` and on `ServerState`; hover rendering of the `///` docs.
3. `src/server/with_state/mod.rs`, inside `lsp_dispatch! { … }`: go-to-definition on `crate::requests::HoverRequest`.
4. `src/requests/hover.rs`, inside `conversion_tests! { … }`: go-to-definition on `line_position` / `state_with_documents` (via the emitted code or directly).
5. Error quality: temporarily rename `modify_outgoing_hover` to `modify_outgoing_hovr` in the attribute — does the error point at the attribute argument?

- [ ] **Step 2: Record verdicts**

For each of the five: works / partial / absent, one line each, in the task report. Note rust-analyzer/Zed version if handy. Migration posture does not change (decision 7); the record informs D6 and future DX expectations.

- [ ] **Step 3: Checkpoint**

No files changed (unless the D6 flip is triggered — then a design note in the report first).

---

## Self-Review (performed at plan writing)

- **Spec coverage:** probe + T4/fallback = Task 1; the five macros = Tasks 1–4; dogfood `hover.rs` + `cargo expand` equivalence = Task 5; spike D3 informational = Task 6. The remaining spec items (47-request sweep, trait stamping ×48, registry deletion, normative docs, completeness wire test, dupes re-run) are Plan 3 by the sequencing table. ✓
- **Placeholder scan:** no sketches or TBDs remain; the two fiddly spots (`parse_spec`'s field loop, `DispatchTable::parse`'s resolve-row probe) are written out in full. ✓
- **Type consistency:** `RequestSpec` fields match the attribute DSL table; `DispatchRow` field names match both emitters and the Task 5 invocation; `TestRow` matches the harness grammar; the engine bodies are transcriptions of the current `implement_method!`/`implement_resolve_method!` sources. ✓
