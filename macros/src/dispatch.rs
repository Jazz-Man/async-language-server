//! `lsp_dispatch!` — stamp the `LanguageServer` dispatch methods for the
//! request table: one row per method, `resolve(...)` rows for the resolve
//! family. Two engines: the URL-anchored one snapshots the document
//! version, converts params and response, and rejects stale results; the
//! sole-document one converts resolve params and results against the
//! single tracked document.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Path, Token, parenthesized};

/// One dispatch-table row: the triple linking our `Server` trait method to
/// the async-lsp `LanguageServer` method and the request marker type.
struct DispatchRow {
    /// Our `Server` trait method (called on the server).
    trait_method: Ident,
    /// The async-lsp `LanguageServer` method (the generated fn's name).
    alsp: Ident,
    /// The request marker type (full path).
    request: Path,
    /// Resolve-family row.
    resolve: bool,
}

impl Parse for DispatchRow {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let trait_method: Ident = input.parse()?;
        let _: Token![:] = input.parse()?;
        let alsp: Ident = input.parse()?;
        let _: Token![@] = input.parse()?;
        let request: Path = input.parse()?;
        Ok(DispatchRow {
            trait_method,
            alsp,
            request,
            resolve: false,
        })
    }
}

/// A `resolve(...)` row — the same triple, marked for the resolve engine.
struct ResolveRow(DispatchRow);

impl Parse for ResolveRow {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kw: Ident = input.parse()?;
        if kw != "resolve" {
            return Err(syn::Error::new_spanned(kw, "expected `resolve`"));
        }
        let content;
        parenthesized!(content in input);
        let mut row: DispatchRow = content.parse()?;
        row.resolve = true;
        Ok(ResolveRow(row))
    }
}

/// The whole invocation: comma-separated rows, trailing comma allowed.
struct DispatchTable(Vec<DispatchRow>);

impl Parse for DispatchTable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
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

/// Expands the dispatch table into one `LanguageServer` method per row.
///
/// # Errors
///
/// Spanned errors for malformed rows: a missing `:` or `@`, a malformed
/// request path, or trailing tokens after the table.
pub(super) fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let rows = syn::parse2::<DispatchTable>(input)?.0;
    let methods = rows.iter().map(engine);
    Ok(quote! { #(#methods)* })
}

/// Probes whether the next tokens are the ident `resolve` followed by a
/// parenthesized group — without consuming anything on `false` (fork and
/// advance the fork only).
fn is_resolve_row(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    let Ok(kw) = fork.parse::<Ident>() else {
        return false;
    };
    kw == "resolve" && fork.peek(syn::token::Paren)
}

/// The engine for one row: the capability gate prepended to the row
/// kind's core, all in the shared dispatch wrapper.
fn engine(row: &DispatchRow) -> TokenStream {
    let DispatchRow {
        trait_method,
        alsp,
        request,
        resolve,
    } = row;
    let gate = quote! {
        // 0. Capability gate: a method absent from the final
        //    ServerCapabilities never activates — reject before any
        //    conversion or handler runs (spec W4). The type-hierarchy
        //    trio is exempt upstream; lifecycle methods never pass
        //    through here.
        if !state.dispatch_allowed(stringify!(#trait_method)) {
            state.warn_once_unadvertised(stringify!(#trait_method));
            return Err(ResponseError::new(
                ErrorCode::METHOD_NOT_FOUND,
                concat!(
                    stringify!(#trait_method),
                    " is not advertised in the server capabilities",
                ),
            ));
        }
    };
    let core = if *resolve {
        sole_document_core(trait_method, request)
    } else {
        url_anchored_core(trait_method, request)
    };
    let gated = quote! { #gate #core };
    wrapped(alsp, request, &gated)
}

/// Which conversion a [`blocking_arm`] hop wraps: the standalone hooks run
/// state-driven conversions against `state` alone, while
/// `convert_resolve_item` routes through the anchored hooks — whose trait
/// defaults delegate to the standalone pair — against the sole tracked
/// document.
#[derive(Clone, Copy)]
enum Conversion {
    /// A standalone hook (`modify_params_standalone` /
    /// `modify_response_standalone`), state-driven.
    Standalone {
        /// The hook method name.
        hook: &'static str,
    },
    /// `convert_resolve_item` against the sole tracked document, in the
    /// named `Direction` variant (`Incoming` before the handler,
    /// `Outgoing` after).
    SoleResolve {
        /// The `Direction` variant name.
        direction: &'static str,
    },
    /// An anchored hook (`modify_params` / `modify_response`) against the
    /// request's conversion document. Reachable for marked URL-less
    /// requests too — `conversion_document` then resolves the sole
    /// tracked document — and the anchored defaults delegate to the
    /// standalone pair, so the delegation must not smuggle the disk read
    /// back onto the executor thread.
    Anchored {
        /// The hook method name.
        hook: &'static str,
    },
}

/// The three token-stream pieces a [`blocking_arm`] hop is built from:
/// the prelude (clones inserted before the closure), the blocking call
/// (inside the closure), and the direct call (the plain inline branch).
fn conversion_calls(
    request: &Path,
    field: &Ident,
    conversion: Conversion,
) -> (TokenStream, TokenStream, TokenStream) {
    match conversion {
        Conversion::Standalone { hook } => {
            let hook = Ident::new(hook, proc_macro2::Span::call_site());
            (
                quote! {},
                quote! {
                    <#request as crate::lsp_requests::Request>::#hook(
                        &state_for_pool,
                        &mut #field,
                    );
                },
                quote! {
                    <#request as crate::lsp_requests::Request>::#hook(
                        &state,
                        &mut #field,
                    );
                },
            )
        }
        Conversion::SoleResolve { direction } => {
            let direction = Ident::new(direction, proc_macro2::Span::call_site());
            (
                quote! { let document_for_pool = document.clone(); },
                quote! {
                    convert_resolve_item::<#request, _>(
                        &state_for_pool,
                        Some(&document_for_pool),
                        &mut #field,
                        Direction::#direction,
                    );
                },
                quote! {
                    convert_resolve_item::<#request, _>(
                        &state, Some(document), &mut #field, Direction::#direction,
                    );
                },
            )
        }
        Conversion::Anchored { hook } => {
            let hook = Ident::new(hook, proc_macro2::Span::call_site());
            (
                // The conversion document, cloned for the pool — never
                // re-resolved (the response step reuses the request's
                // document on purpose).
                quote! { let document_for_pool = doc.clone(); },
                quote! {
                    <#request as crate::lsp_requests::Request>::#hook(
                        &state_for_pool,
                        &document_for_pool,
                        &mut #field,
                    );
                },
                quote! {
                    <#request as crate::lsp_requests::Request>::#hook(
                        &state, doc, &mut #field,
                    );
                },
            )
        }
    }
}

/// The conditional blocking-pool hop for a disk-reading conversion:
/// `field` is the value moved through `spawn_blocking` (`params` or
/// `result`). The blocking branch clones the captures the conversion
/// needs — always `state`, plus the conversion document for
/// [`Conversion::SoleResolve`] and [`Conversion::Anchored`] — moves the
/// value into the closure, and restores it from the closure's return;
/// join failures map to `INTERNAL_ERROR`. The direct branch is the plain
/// inline call, and unmarked requests (`STANDALONE_READS_DISK == false`)
/// never hop.
fn blocking_arm(
    request: &Path,
    trait_method: &Ident,
    field: &str,
    conversion: Conversion,
) -> TokenStream {
    let field = Ident::new(field, proc_macro2::Span::call_site());
    let (prelude, blocking_call, direct_call) = conversion_calls(request, &field, conversion);
    quote! {
        if <#request as crate::lsp_requests::Request>::STANDALONE_READS_DISK {
            let state_for_pool = state.clone();
            #prelude
            #field = tokio::task::spawn_blocking(move || {
                let mut #field = #field;
                #blocking_call
                #field
            })
            .await
            .map_err(|join_error| {
                ResponseError::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!(
                        "{} conversion failed: {join_error}",
                        stringify!(#trait_method),
                    ),
                )
            })?;
        } else {
            #direct_call
        }
    }
}

/// The sole-document core (6 `resolve(...)` rows): converts against the
/// single tracked document, falling back to the standalone hooks when none
/// is sole. Both arms hop to the blocking pool for marked requests: the
/// `None` arms call the standalone hooks directly, and the `Some` arms go
/// through `convert_resolve_item`, whose anchored-hook defaults delegate
/// to the standalone pair — the delegation must not smuggle the disk read
/// back onto the executor thread.
fn sole_document_core(trait_method: &Ident, request: &Path) -> TokenStream {
    let params_arm = blocking_arm(
        request,
        trait_method,
        "params",
        Conversion::Standalone {
            hook: "modify_params_standalone",
        },
    );
    let response_arm = blocking_arm(
        request,
        trait_method,
        "result",
        Conversion::Standalone {
            hook: "modify_response_standalone",
        },
    );
    let anchored_params_arm = blocking_arm(
        request,
        trait_method,
        "params",
        Conversion::SoleResolve {
            direction: "Incoming",
        },
    );
    let anchored_response_arm = blocking_arm(
        request,
        trait_method,
        "result",
        Conversion::SoleResolve {
            direction: "Outgoing",
        },
    );
    quote! {
        // Resolve requests carry no text-document URL: convert against the
        // sole tracked document, if the server tracks exactly one; with no
        // sole document, the standalone hooks run state-driven conversions
        // instead of skipping them.
        let sole = state.sole_document();
        match sole.as_ref() {
            Some(document) => { #anchored_params_arm }
            None => { #params_arm }
        }
        let mut result = match server.#trait_method(state.clone(), params).await {
            Ok(result) => result,
            Err(error) => {
                state.warn_once_default(stringify!(#trait_method), &error);
                return Err(error.into());
            }
        };
        match sole.as_ref() {
            Some(document) => { #anchored_response_arm }
            None => { #response_arm }
        }
        Ok(result)
    }
}

/// The URL-anchored core (42 normal rows): snapshots the document version,
/// converts params and response against the conversion document, and
/// rejects stale results with `CONTENT_MODIFIED`.
fn url_anchored_core(trait_method: &Ident, request: &Path) -> TokenStream {
    let response_arm = blocking_arm(
        request,
        trait_method,
        "result",
        Conversion::Standalone {
            hook: "modify_response_standalone",
        },
    );
    let anchored_params_arm = blocking_arm(
        request,
        trait_method,
        "params",
        Conversion::Anchored {
            hook: "modify_params",
        },
    );
    let anchored_response_arm = blocking_arm(
        request,
        trait_method,
        "result",
        Conversion::Anchored {
            hook: "modify_response",
        },
    );
    quote! {
        // 1. Try to extract the URL from the params for document tracking
        let url: Option<Url> =
            <#request as crate::lsp_requests::Request>::extract_url(&params);
        // 1.5 Off-executor fallback prime: an untracked file URL gets its
        //     disk snapshot read once per (URL, stamp) on the blocking
        //     pool, so conversions never read disk on the executor thread.
        if let Some(untracked) = url
            .as_ref()
            .filter(|url| state.document(url).is_none())
        {
            state.prime_conversion_fallback(untracked.clone()).await;
        }
        // 2. Version probe (clone-free) and one conversion document
        //    for the whole request.
        let ver: Option<i32> =
            url.as_ref().and_then(|url| state.document_version(url));
        let params_doc = conversion_document(&state, url.as_ref());
        if let Some(doc) = params_doc.as_ref() {
            #anchored_params_arm
        }

        // 3. Call the user-defined language server function. A default
        //    error on an advertised method draws its single warning
        //    before the error proceeds unchanged to the wire.
        let mut result = match server.#trait_method(state.clone(), params).await {
            Ok(result) => result,
            Err(error) => {
                state.warn_once_default(stringify!(#trait_method), &error);
                return Err(error.into());
            }
        };

        // 4. Staleness probe against the same clone-free version.
        if let Some(url) = url.as_ref()
            && state.document_version(url).is_some_and(|v| Some(v) != ver)
        {
            return Err(ResponseError::new(
                ErrorCode::CONTENT_MODIFIED,
                "document was modified during processing",
            ));
        }

        // 5. The staleness probe passed, so the conversion document is
        //    still valid for the response — reuse it instead of
        //    re-resolving (one snapshot and at most one disk read per
        //    request).
        match params_doc.as_ref() {
            Some(doc) => { #anchored_response_arm }
            None => { #response_arm }
        }

        Ok(result)
    }
}

/// The shared dispatch-method wrapper around a core: signature, server and
/// state capture, pinned async block.
fn wrapped(alsp: &Ident, request: &Path, core: &TokenStream) -> TokenStream {
    quote! {
        fn #alsp(
            &mut self,
            mut params: <#request as crate::lsp_requests::Request>::Params,
        ) -> BoxFuture<
            'static,
            Result<<#request as crate::lsp_requests::Request>::Response, Self::Error>,
        > {
            let server = Arc::clone(&self.server);
            let state = self.state.clone();
            Box::pin(async move { #core })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};
    use rstest::rstest;

    /// A row parses into the full triple with `resolve` unset — matching
    /// names and name-diverging rows alike.
    #[rstest]
    #[case::plain_row(
        "hover: hover @ crate::lsp_requests::HoverRequest",
        "hover",
        "hover",
        "crate :: lsp_requests :: HoverRequest"
    )]
    #[case::diverging_names(
        "rename_prepare: prepare_rename @ crate::lsp_requests::RenamePrepareRequest",
        "rename_prepare",
        "prepare_rename",
        "crate :: lsp_requests :: RenamePrepareRequest"
    )]
    fn parses_dispatch_row(
        #[case] input: &str,
        #[case] trait_method: &str,
        #[case] alsp: &str,
        #[case] request: &str,
    ) {
        let r: DispatchRow = syn::parse2(input.parse().expect("tokens")).expect("row parses");
        assert_eq!(r.trait_method, trait_method);
        assert_eq!(r.alsp, alsp);
        assert_eq!(r.request.to_token_stream().to_string(), request);
        assert!(!r.resolve);
    }

    #[rstest]
    fn engine_emits_url_anchored_skeleton() {
        let r: DispatchRow = syn::parse2(quote! { hover: hover @ R }).expect("row parses");
        let text = engine(&r).to_string();
        for needle in [
            "fn hover",
            "dispatch_allowed",
            "METHOD_NOT_FOUND",
            "extract_url",
            "document_version",
            "CONTENT_MODIFIED",
            "modify_response_standalone",
            "warn_once_default",
            "prime_conversion_fallback",
            "STANDALONE_READS_DISK",
            // The anchored Some-arms hop too, cloning the conversion
            // document into the closure instead of re-resolving it.
            "document_for_pool",
            ". hover (state . clone () , params)",
        ] {
            assert!(text.contains(needle), "missing {needle:?} from {text}");
        }
        assert_eq!(
            text.matches("conversion_document").count(),
            1,
            "the response step must reuse the request's conversion document",
        );
    }

    #[rstest]
    fn engine_emits_sole_document_path_for_resolve_rows() {
        let mut r: DispatchRow =
            syn::parse2(quote! { completion_resolve: completion_resolve @ R }).expect("row parses");
        r.resolve = true;
        let text = engine(&r).to_string();
        assert!(text.contains("dispatch_allowed"));
        assert!(text.contains("convert_resolve_item"));
        assert!(text.contains("Direction :: Incoming"));
        assert!(text.contains("sole_document"));
        assert!(text.contains("STANDALONE_READS_DISK"));
        assert!(text.contains("warn_once_default"));
        assert!(!text.contains("CONTENT_MODIFIED"));
    }

    #[rstest]
    fn table_parses_mixed_rows_and_trailing_comma() {
        let table: DispatchTable = syn::parse2(quote! {
            hover: hover @ A,
            resolve(r: r @ B),
        })
        .expect("table parses");
        assert_eq!(table.0.len(), 2);
        assert!(table.0[1].resolve);
    }

    #[rstest]
    fn rejects_row_missing_at() {
        assert!(syn::parse2::<DispatchRow>(quote! { hover: hover A }).is_err());
    }
}
