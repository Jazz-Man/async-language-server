//! `conversion_tests!` — stamp one `#[rstest]` case-table per request's
//! conversion hooks. The W0 table harness: the table fn drives
//! `modify_params`/`modify_response` through the `crate::testing::utf16_state`
//! fixture (injected via its fully-qualified `#[from]` path, so the stamped
//! table needs nothing in the invoking module's scope), each row becomes a
//! `#[case::name]` carrying only data (a typed row struct mirrors the row
//! grammar), and the emitted code uses call-site `crate::` paths.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Token, Type};

/// One row of the table: the request under test plus its fixture closures.
struct TestRow {
    /// The stamped case's name.
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

/// Converts a CamelCase type stem to its snake-cased name:
/// `CallHierarchyPrepare` → `call_hierarchy_prepare`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.char_indices() {
        if ch.is_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// The table's names: the request type's last path segment, minus a
/// trailing `Request`, yields the snake-cased table fn prefix and the
/// CamelCase row-struct name (`HoverRequest` → `hover_conversion_round_trips`
/// + `HoverConversionRow`).
fn table_names(request: &Type) -> syn::Result<(Ident, Ident)> {
    let segment = match request {
        Type::Path(path) => path.path.segments.last(),
        _ => None,
    };
    let segment = segment
        .map(|last| last.ident.to_string())
        .ok_or_else(|| syn::Error::new_spanned(request, "request must be a path type"))?;
    let stem = segment.strip_suffix("Request").unwrap_or(&segment);
    let prefix = snake_case(stem);
    Ok((
        format_ident!("{prefix}_conversion_round_trips"),
        format_ident!("{stem}ConversionRow"),
    ))
}

/// The rows' shared request type, spanned-erroring if any row names a
/// different one: the table fn and row struct are stamped from one request,
/// so a mixed table has no shape.
fn uniform_request(rows: &[TestRow]) -> syn::Result<&Type> {
    let first = rows
        .first()
        .ok_or_else(|| syn::Error::new(Span::call_site(), "empty conversion table"))?;
    let request = &first.request;
    let request_name = request.to_token_stream().to_string();
    for row in &rows[1..] {
        if row.request.to_token_stream().to_string() != request_name {
            return Err(syn::Error::new_spanned(
                &row.request,
                "all rows in a table must name the same request type",
            ));
        }
    }
    Ok(request)
}

/// Stamps one `#[case::name(...)]` attribute per row: the row struct
/// literal carrying the row's closures and expected positions, `None`
/// where the optional hook pairs are absent.
fn case_attrs(rows: &[TestRow], row_struct: &Ident) -> TokenStream {
    let cases = rows.iter().map(|row| {
        let name = &row.name;
        let params = &row.params;
        let (incoming, expects) = match &row.incoming {
            Some((extract, expected)) => (quote! { Some(#extract) }, quote! { Some(#expected) }),
            None => (quote! { None }, quote! { None }),
        };
        let (response, outgoing, returns) = match &row.response {
            Some((build, extract, expected)) => (
                quote! { Some(#build) },
                quote! { Some(#extract) },
                quote! { Some(#expected) },
            ),
            None => (quote! { None }, quote! { None }, quote! { None }),
        };
        quote! {
            #[case::#name(#row_struct {
                params: #params,
                incoming: #incoming,
                expects: #expects,
                response: #response,
                outgoing: #outgoing,
                returns: #returns,
            })]
        }
    });
    quote! { #(#cases)* }
}

/// The typed row-struct definition: one field per row-grammar field, types
/// derived from the request's `Request` associated types — `params` and
/// `response` as fn pointers from `Url` arguments, the extractors as fn
/// pointers from shared references, the optional columns as `Option`.
fn row_struct_def(request: &Type, row_struct: &Ident) -> TokenStream {
    let params_type = quote! { fn(async_lsp::lsp_types::Url) -> <#request as crate::lsp_requests::Request>::Params };
    let response_type = quote! { Option<fn(async_lsp::lsp_types::Url, async_lsp::lsp_types::Url) -> <#request as crate::lsp_requests::Request>::Response> };
    let incoming_type = quote! { Option<fn(&<#request as crate::lsp_requests::Request>::Params) -> async_lsp::lsp_types::Position> };
    let outgoing_type = quote! { Option<fn(&<#request as crate::lsp_requests::Request>::Response) -> async_lsp::lsp_types::Position> };
    let position_type = quote! { Option<async_lsp::lsp_types::Position> };
    quote! {
        struct #row_struct {
            params: #params_type,
            incoming: #incoming_type,
            expects: #position_type,
            response: #response_type,
            outgoing: #outgoing_type,
            returns: #position_type,
        }
    }
}

/// Expands the table into one `#[rstest]` fn driving the request's hooks
/// through the injected `crate::testing::utf16_state` fixture, with one
/// `#[case::name]` per row carrying a typed row struct — the emitted code
/// uses call-site `crate` paths.
///
/// # Errors
///
/// Spanned errors for malformed rows: a missing first `params` field, an
/// unknown or duplicated row field, a missing `expects`/`outgoing`/
/// `returns` partner, stray tokens inside the braces, an empty table, a
/// non-path request type, or rows naming different request types.
fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let rows = syn::parse2::<TestTable>(input)?.0;
    let request = uniform_request(&rows)?;
    let (table_fn, row_struct) = table_names(request)?;
    let cases = case_attrs(&rows, &row_struct);
    let row_struct_def = row_struct_def(request, &row_struct);

    Ok(quote! {
        #row_struct_def

        #[rstest::rstest]
        #cases
        fn #table_fn(
            #[from(crate::testing::utf16_state)] utf16_state: (crate::server::ServerState, async_lsp::lsp_types::Url, async_lsp::lsp_types::Url),
            #[case] row: #row_struct,
        ) {
            let (state, _plain, emoji) = utf16_state;
            let #row_struct { params, incoming, expects, response, outgoing, returns } = row;
            let document = state.document(&emoji).expect("emoji document is tracked");
            let mut params = (params)(emoji.clone());
            <#request as crate::lsp_requests::Request>::modify_params(&state, &document, &mut params);
            if let (Some(incoming), Some(expects)) = (incoming, expects) {
                crate::testing::assert_converted_position(
                    &params,
                    incoming,
                    expects,
                    "incoming position must be converted to the UTF-8 byte column",
                );
            }
            if let (Some(response), Some(outgoing), Some(returns)) = (response, outgoing, returns) {
                let mut response = (response)(_plain.clone(), emoji.clone());
                <#request as crate::lsp_requests::Request>::modify_response(&state, &document, &mut response);
                crate::testing::assert_converted_position(
                    &response,
                    outgoing,
                    returns,
                    "outgoing position must be converted to the client encoding",
                );
            }
        }
    })
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
    use rstest::rstest;

    /// The optional hook pairs fill exactly when written: a `params`-only
    /// row leaves both `None`, the all-fields row fills both.
    #[rstest]
    #[case::minimal_row("t: R { params: |uri| P::new(uri) }", false, false)]
    #[case::full_row(
        "t: R { params: p, incoming: i, expects: e, response: r, outgoing: o, returns: x }",
        true,
        true
    )]
    fn parses_row_with_optional_hook_pairs(
        #[case] input: &str,
        #[case] incoming: bool,
        #[case] response: bool,
    ) {
        let table: TestTable = syn::parse2(input.parse().expect("tokens")).expect("parses");
        assert_eq!(table.0.len(), 1);
        assert_eq!(table.0[0].incoming.is_some(), incoming);
        assert_eq!(table.0[0].response.is_some(), response);
    }

    /// The emission is one `#[rstest]` table fn per request: a typed row
    /// struct named from the request, one `#[case::name]` per row, the
    /// `utf16_state` fixture injected through its fully-qualified `#[from]`
    /// path (the stamped table needs no imports in the invoking module),
    /// and the conversion script — never a plain `#[test]`.
    #[rstest]
    #[case::simple_request(
        "t: R { params: |uri| P::new(uri) }",
        "r_conversion_round_trips",
        "RConversionRow"
    )]
    #[case::multiword_request(
        "t: crate::lsp_requests::CallHierarchyPrepareRequest { params: |uri| P::new(uri) }",
        "call_hierarchy_prepare_conversion_round_trips",
        "CallHierarchyPrepareConversionRow"
    )]
    fn emits_rstest_table_with_typed_row_struct(
        #[case] input: &str,
        #[case] table_fn: &str,
        #[case] row_struct: &str,
    ) {
        let out = expand(input.parse().expect("tokens")).expect("expands");
        let text = out.to_string();
        assert!(text.contains("# [rstest :: rstest]"));
        assert!(text.contains("# [case :: t ("));
        assert!(text.contains(row_struct));
        assert!(text.contains(table_fn));
        assert!(text.contains("# [from (crate :: testing :: utf16_state)]"));
        assert!(text.contains("crate :: server :: ServerState"));
        assert!(text.contains("modify_params"));
        assert!(text.contains("modify_response"));
        assert!(!text.contains("# [test]"));
    }

    /// Malformed rows reject with a spanned error naming the defect class.
    #[rstest]
    #[case::empty_table("", "empty conversion table")]
    #[case::unknown_row_field("t: R { params: p, bogus: b }", "unknown row field")]
    #[case::duplicate_row_field(
        "t: R { params: p, incoming: i, expects: e, incoming: i2, expects: e2 }",
        "duplicate row field"
    )]
    #[case::stray_int_tokens("t: R { params: p, 42 }", "expected identifier")]
    #[case::missing_separator("t: R { params: p 42 }", "unexpected tokens")]
    fn rejects_malformed_rows(#[case] input: &str, #[case] needle: &str) {
        let err = expand(input.parse().expect("tokens")).expect_err("rejected");
        assert!(
            err.to_string().contains(needle),
            "{needle:?} missing from {err}",
        );
    }

    /// Rows naming different request types reject: the table fn and row
    /// struct are stamped from one request, so a mixed table has no shape.
    #[rstest]
    fn rejects_mixed_request_types() {
        let err = expand(quote! {
            first: R { params: |uri| P::new(uri) }
            second: S { params: |uri| P::new(uri) }
        })
        .expect_err("rejected");
        let text = err.to_string();
        assert!(
            text.contains("same request type"),
            "uniformity error missing from {text}",
        );
    }
}
