//! `lsp_dispatch!` — stamp the `LanguageServer` dispatch methods for the
//! request table: one row per method, `resolve(...)` rows for the resolve
//! family. The engines are the line-for-line successors of the former
//! `implement_method!` / `implement_resolve_method!` `macro_rules` bodies.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Ident, Path, Token, parenthesized,
    parse::{Parse, ParseStream},
};

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

/// The engine for one row: the row kind's core in the shared dispatch
/// wrapper. The URL-anchored core (42 normal rows) is today's
/// `implement_method!`, the sole-document core (6 resolve rows) today's
/// `implement_resolve_method!` — both line-for-line.
fn engine(row: &DispatchRow) -> TokenStream {
    let DispatchRow {
        trait_method,
        alsp,
        request,
        resolve,
    } = row;
    let core = if *resolve {
        quote! {
            // Resolve requests carry no text-document URL: convert against the
            // sole tracked document, if the server tracks exactly one; with no
            // sole document, the standalone hooks run state-driven conversions
            // instead of skipping them.
            let sole = conversion_document(&state, None);
            match sole.as_ref() {
                Some(document) => {
                    convert_resolve_item::<#request, _>(
                        &state, Some(document), &mut params, Direction::Incoming,
                    );
                }
                None => {
                    <#request as crate::requests::Request>::modify_params_standalone(
                        &state, &mut params,
                    );
                }
            }
            let mut result = server.#trait_method(state.clone(), params).await?;
            match sole.as_ref() {
                Some(document) => {
                    convert_resolve_item::<#request, _>(
                        &state, Some(document), &mut result, Direction::Outgoing,
                    );
                }
                None => {
                    <#request as crate::requests::Request>::modify_response_standalone(
                        &state, &mut result,
                    );
                }
            }
            Ok(result)
        }
    } else {
        quote! {
            // 1. Try to extract the URL from the params for document tracking
            let url: Option<Url> =
                <#request as crate::requests::Request>::extract_url(&params);
            let mut ver: Option<i32> = None;

            // 2. If we got an URL, track the document version
            if let Some(url) = url.as_ref() && let Some(doc) = state.document(url) {
                ver.replace(doc.version());
            }

            // 3. Call the "modify params" callback against the request's
            //    conversion document: the tracked snapshot for a tracked URL,
            //    a disk snapshot for an untracked file URL, or the sole
            //    tracked document for URL-less requests
            let params_doc = conversion_document(&state, url.as_ref());
            if let Some(doc) = params_doc.as_ref() {
                <#request as crate::requests::Request>::modify_params(&state, doc, &mut params,);
            }

            // 4. Call the user-defined language server function
            let mut result = server.#trait_method(state.clone(), params).await?;

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
                    <#request as crate::requests::Request>::modify_response(&state, &doc, &mut result,);
                }
                None => {
                    <#request as crate::requests::Request>::modify_response_standalone(
                        &state, &mut result,
                    );
                }
            }

            Ok(result)
        }
    };
    wrapped(alsp, request, &core)
}

/// The shared dispatch-method wrapper around a core: signature, server and
/// state capture, pinned async block — verbatim from the `macro_rules`
/// engines.
fn wrapped(alsp: &Ident, request: &Path, core: &TokenStream) -> TokenStream {
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
            Box::pin(async move { #core })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    #[test]
    fn parses_plain_row() {
        let r: DispatchRow = syn::parse2(quote! { hover: hover @ crate::requests::HoverRequest })
            .expect("row parses");
        assert_eq!(r.trait_method, "hover");
        assert_eq!(r.alsp, "hover");
        assert_eq!(
            r.request.to_token_stream().to_string(),
            "crate :: requests :: HoverRequest"
        );
        assert!(!r.resolve);
    }

    #[test]
    fn parses_diverging_names() {
        let r: DispatchRow = syn::parse2(
            quote! { rename_prepare: prepare_rename @ crate::requests::RenamePrepareRequest },
        )
        .expect("row parses");
        assert_eq!(r.trait_method, "rename_prepare");
        assert_eq!(r.alsp, "prepare_rename");
    }

    #[test]
    fn engine_emits_url_anchored_skeleton() {
        let r: DispatchRow = syn::parse2(quote! { hover: hover @ R }).expect("row parses");
        let text = engine(&r).to_string();
        for needle in [
            "fn hover",
            "extract_url",
            "conversion_document",
            "CONTENT_MODIFIED",
            "modify_response_standalone",
            ". hover (state . clone () , params)",
        ] {
            assert!(text.contains(needle), "missing {needle:?} from {text}");
        }
    }

    #[test]
    fn engine_emits_sole_document_path_for_resolve_rows() {
        let mut r: DispatchRow =
            syn::parse2(quote! { completion_resolve: completion_resolve @ R }).expect("row parses");
        r.resolve = true;
        let text = engine(&r).to_string();
        assert!(text.contains("convert_resolve_item"));
        assert!(text.contains("Direction :: Incoming"));
        assert!(!text.contains("CONTENT_MODIFIED"));
    }

    #[test]
    fn table_parses_mixed_rows_and_trailing_comma() {
        let table: DispatchTable = syn::parse2(quote! {
            hover: hover @ A,
            resolve(r: r @ B),
        })
        .expect("table parses");
        assert_eq!(table.0.len(), 2);
        assert!(table.0[1].resolve);
    }

    #[test]
    fn rejects_row_missing_at() {
        assert!(syn::parse2::<DispatchRow>(quote! { hover: hover A }).is_err());
    }
}
