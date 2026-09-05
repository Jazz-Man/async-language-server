//! `conversion_tests!` — stamp one `#[test]` per row for a request's
//! conversion hooks. The W0 table harness; grammar and expansion are the
//! 2026-09-01 `macro_rules!` verbatim, with `$crate` replaced by call-site
//! `crate::` paths.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, Token, Type,
    parse::{Parse, ParseStream},
};

/// One row of the table: the request under test plus its fixture closures.
struct TestRow {
    /// The stamped test's name.
    name: Ident,
    /// The request marker type (full path).
    request: Type,
    /// `params` — builds params against the emoji document.
    params: Expr,
    /// `incoming`/`expects` — the position extractor and the UTF-8 (byte
    /// column) position it must equal after `modify_params`.
    incoming: Option<(Expr, Expr)>,
    /// `response`/`outgoing`/`returns` — the response builder, the position
    /// extractor, and the client-encoding position it must equal after
    /// `modify_response`.
    response: Option<(Expr, Expr, Expr)>,
}

impl Parse for TestRow {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let request: Type = input.parse()?;
        let content;
        syn::braced!(content in input);
        let params: Expr = keyed_expr(&content, "params")?;
        let mut incoming = None;
        let mut response = None;
        while content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            if content.is_empty() {
                break; // trailing comma
            }
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "incoming" => {
                    if incoming.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "duplicate row field `incoming`",
                        ));
                    }
                    let incoming_expr: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let expects = keyed_expr(&content, "expects")?;
                    incoming = Some((incoming_expr, expects));
                }
                "response" => {
                    if response.is_some() {
                        return Err(syn::Error::new_spanned(
                            key,
                            "duplicate row field `response`",
                        ));
                    }
                    let response_expr: Expr = content.parse()?;
                    content.parse::<Token![,]>()?;
                    let outgoing = keyed_expr(&content, "outgoing")?;
                    content.parse::<Token![,]>()?;
                    let returns = keyed_expr(&content, "returns")?;
                    response = Some((response_expr, outgoing, returns));
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        key,
                        format!("unknown row field `{other}`"),
                    ));
                }
            }
        }
        if !content.is_empty() {
            return Err(content.error("unexpected tokens after the row fields"));
        }
        Ok(Self {
            name,
            request,
            params,
            incoming,
            response,
        })
    }
}

/// Parses `key : expr` for a known `key`, spanned-erroring on any other —
/// the one production every row field shares.
fn keyed_expr(content: ParseStream, key: &str) -> syn::Result<Expr> {
    let parsed: Ident = content.parse()?;
    if parsed != key {
        return Err(syn::Error::new_spanned(parsed, format!("expected `{key}`")));
    }
    content.parse::<Token![:]>()?;
    content.parse()
}

/// The whole invocation: back-to-back rows, no separator — the rows'
/// `name : Type { ... }` shape self-delimits.
struct TestTable(Vec<TestRow>);

impl Parse for TestTable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut rows = Vec::new();
        while !input.is_empty() {
            rows.push(input.parse()?);
        }
        Ok(Self(rows))
    }
}

/// Expands the table into one `#[test]` fn per row — the former
/// `macro_rules!` body verbatim, with `$crate` as call-site `crate`.
///
/// # Errors
///
/// Spanned errors for malformed rows: a missing first `params` field, an
/// unknown or duplicated row field, a missing `expects`/`outgoing`/
/// `returns` partner, or stray tokens inside the braces.
pub(super) fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let rows = syn::parse2::<TestTable>(input)?.0;
    let tests = rows.iter().map(|row| {
        let TestRow {
            name,
            request,
            params,
            incoming,
            response,
        } = row;
        let incoming = incoming.as_ref().map(|(extract, expects)| {
            quote! {
                crate::testing::assert_converted_position(
                    &params,
                    #extract,
                    #expects,
                    "incoming position must be converted to the UTF-8 byte column",
                );
            }
        });
        let response = response.as_ref().map(|(build, extract, returns)| {
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

/// Entry point of the macro: expands the invocation or maps the spanned
/// error to a compile error.
pub(super) fn entry(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parses_minimal_row() {
        let table: TestTable = syn::parse2(quote! {
            t: R { params: |uri| P::new(uri) }
        })
        .expect("parses");
        assert_eq!(table.0.len(), 1);
        assert!(table.0[0].incoming.is_none());
        assert!(table.0[0].response.is_none());
    }

    #[test]
    fn parses_full_row() {
        let table: TestTable = syn::parse2(quote! {
            t: R {
                params: p,
                incoming: i,
                expects: e,
                response: r,
                outgoing: o,
                returns: x,
            }
        })
        .expect("parses");
        assert!(table.0[0].incoming.is_some());
        assert!(table.0[0].response.is_some());
    }

    #[test]
    fn emits_test_fn_with_fixtures() {
        let out = expand(quote! {
            t: R { params: |uri| P::new(uri) }
        })
        .expect("expands");
        let text = out.to_string();
        assert!(text.contains("# [test]"));
        assert!(text.contains("state_with_documents"));
        assert!(text.contains("modify_params"));
    }

    #[test]
    fn rejects_unknown_duplicate_and_stray_row_tokens() {
        let cases: [(&str, &str); 4] = [
            ("t: R { params: p, bogus: b }", "unknown row field"),
            (
                "t: R { params: p, incoming: i, expects: e, incoming: i2, expects: e2 }",
                "duplicate row field",
            ),
            ("t: R { params: p, 42 }", "expected identifier"),
            ("t: R { params: p 42 }", "unexpected tokens"),
        ];
        for (i, (input, needle)) in cases.into_iter().enumerate() {
            let err = expand(input.parse().expect("tokens")).expect_err("rejected");
            assert!(
                err.to_string().contains(needle),
                "case {i}: {needle:?} missing from {err}"
            );
        }
    }
}
