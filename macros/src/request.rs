//! `#[lsp_request]` — per-file registration of one LSP request.
//!
//! DSL: `params`/`response` take wire types (`params = <type path>`); every
//! hook is a parenthesized list carrying one field path (`document`,
//! `incoming_position`, `incoming_range`) or one function path
//! (`incoming_custom`, `outgoing`, `incoming_standalone`,
//! `outgoing_standalone`). Unspecified hooks keep the `Request` trait's
//! delegating defaults.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, ItemStruct, Path, Token, Type, parenthesized,
    parse::{Parse, ParseStream},
};

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

/// Expands `#[lsp_request(...)]` on a non-generic struct into the struct
/// plus its `impl Request`.
///
/// # Errors
///
/// Spanned errors for: unknown fields, missing `params`/`response`,
/// malformed paths, duplicate hook kinds, generics on the struct.
pub(super) fn expand(attr: TokenStream, item: &ItemStruct) -> syn::Result<TokenStream> {
    let spec = parse_spec(attr)?;
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "request marker structs are non-generic unit structs",
        ));
    }
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
    let name = &item.ident;
    let extract_url = document.map(|segments| extract_url_fn(&segments));
    let modify_params = modify_params_fn(incoming_position, incoming_range, incoming_custom);
    let modify_response = outgoing.map(|fun| modify_response_fn(&fun));
    let modify_params_standalone = incoming_standalone.map(|fun| modify_params_standalone_fn(&fun));
    let modify_response_standalone =
        outgoing_standalone.map(|fun| modify_response_standalone_fn(&fun));

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

/// Emits `extract_url` reading the document URL at the given field path.
fn extract_url_fn(segments: &[Ident]) -> TokenStream {
    quote! {
        fn extract_url(params: &Self::Params) -> Option<::async_lsp::lsp_types::Url> {
            ::core::option::Option::Some(params.#(#segments).*.uri.clone())
        }
    }
}

/// Emits `modify_params` converting exactly the wired incoming hooks — the
/// generated import lists only converters the body calls, since an unused
/// `use` would fail the crate's `-D warnings` battery.
fn modify_params_fn(
    position: Option<Vec<Ident>>,
    range: Option<Vec<Ident>>,
    custom: Option<Path>,
) -> Option<TokenStream> {
    let mut conversions = Vec::new();
    let mut converters = Vec::new();
    if let Some(segments) = position {
        converters.push(quote! { convert_position });
        conversions.push(
            quote! { convert_position(state, document, &mut params.#(#segments).*, Direction::Incoming); },
        );
    }
    if let Some(segments) = range {
        converters.push(quote! { convert_range });
        conversions.push(
            quote! { convert_range(state, document, &mut params.#(#segments).*, Direction::Incoming); },
        );
    }
    let custom = custom.map(|fun| quote! { #fun(state, document, params); });
    let has_incoming = !conversions.is_empty() || custom.is_some();
    has_incoming.then(|| {
        let imports = if conversions.is_empty() {
            quote! {}
        } else {
            quote! { use crate::requests::conversion::{Direction, #(#converters),*}; }
        };
        let custom = custom.into_iter();
        quote! {
            fn modify_params(
                state: &crate::server::ServerState,
                document: &crate::server::Document,
                params: &mut Self::Params,
            ) {
                #imports
                #(#conversions)*
                #(#custom)*
            }
        }
    })
}

/// Emits `modify_response` delegating to the outgoing hook function.
fn modify_response_fn(fun: &Path) -> TokenStream {
    quote! {
        fn modify_response(
            state: &crate::server::ServerState,
            document: &crate::server::Document,
            response: &mut Self::Response,
        ) {
            #fun(state, document, response);
        }
    }
}

/// Emits `modify_params_standalone` delegating to the hook function.
fn modify_params_standalone_fn(fun: &Path) -> TokenStream {
    quote! {
        fn modify_params_standalone(state: &crate::server::ServerState, params: &mut Self::Params) {
            #fun(state, params);
        }
    }
}

/// Emits `modify_response_standalone` delegating to the hook function.
fn modify_response_standalone_fn(fun: &Path) -> TokenStream {
    quote! {
        fn modify_response_standalone(state: &crate::server::ServerState, response: &mut Self::Response) {
            #fun(state, response);
        }
    }
}

/// The attribute grammar: comma-separated entries, each either
/// `name = <type>` or `name(<tokens>)` (trailing comma allowed).
///
/// Parsed with a dedicated grammar rather than [`syn::Meta`] because
/// attribute values are expressions to syn — `Option<Hover>` reads as a
/// comparison chain and fails ("comparison operators cannot be chained");
/// here the value is parsed as a [`Type`] directly, no re-parse needed.
struct Entries(Vec<Entry>);

/// One attribute entry.
enum Entry {
    /// `name = <type>` — the `params`/`response` wire types.
    Type {
        /// Field name as written.
        name: Ident,
        /// The type on the right of `=` (boxed to keep the variants
        /// size-balanced).
        ty: Box<Type>,
    },
    /// `name(<tokens>)` — a hook, kept raw until the field name selects
    /// the field-path or function-path shape.
    List {
        /// Field name as written.
        name: Ident,
        /// The parenthesized tokens.
        tokens: TokenStream,
    },
}

impl Parse for Entries {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let entry = if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                Entry::Type {
                    name,
                    ty: Box::new(input.parse()?),
                }
            } else {
                let content;
                parenthesized!(content in input);
                Entry::List {
                    name,
                    tokens: content.parse()?,
                }
            };
            entries.push(entry);
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self(entries))
    }
}

/// The nine attribute fields before `params`/`response` are required.
#[derive(Default)]
struct PartialSpec {
    params: Option<Type>,
    response: Option<Type>,
    document: Option<Vec<Ident>>,
    incoming_position: Option<Vec<Ident>>,
    incoming_range: Option<Vec<Ident>>,
    incoming_custom: Option<Path>,
    outgoing: Option<Path>,
    incoming_standalone: Option<Path>,
    outgoing_standalone: Option<Path>,
}

/// The seven hooks and the shape their list payload takes.
const HOOKS: &[(&str, Shape)] = &[
    ("document", Shape::FieldPath),
    ("incoming_position", Shape::FieldPath),
    ("incoming_range", Shape::FieldPath),
    ("incoming_custom", Shape::FunctionPath),
    ("outgoing", Shape::FunctionPath),
    ("incoming_standalone", Shape::FunctionPath),
    ("outgoing_standalone", Shape::FunctionPath),
];

/// The payload shape of a hook list.
#[derive(Clone, Copy)]
enum Shape {
    /// `name(field.path)` — parsed as a dotted field path.
    FieldPath,
    /// `name(path::to::function)` — parsed as a [`Path`].
    FunctionPath,
}

/// Stores `value` in `slot`, erroring spanned on a duplicate field.
fn set<T>(slot: &mut Option<T>, value: T, name: &Ident) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(
            name,
            format!("duplicate field `{name}`"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_spec(attr: TokenStream) -> syn::Result<RequestSpec> {
    let entries = syn::parse2::<Entries>(attr)?.0;
    let mut partial = PartialSpec::default();
    for entry in entries {
        match entry {
            Entry::Type { name, ty } => apply_type(&mut partial, &name, *ty)?,
            Entry::List { name, tokens } => apply_hook(&mut partial, &name, tokens)?,
        }
    }
    let params = partial.params.ok_or_else(|| {
        syn::Error::new(proc_macro2::Span::call_site(), "missing `params = <type>`")
    })?;
    let response = partial.response.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing `response = <type>`",
        )
    })?;
    Ok(RequestSpec {
        params,
        response,
        document: partial.document,
        incoming_position: partial.incoming_position,
        incoming_range: partial.incoming_range,
        incoming_custom: partial.incoming_custom,
        outgoing: partial.outgoing,
        incoming_standalone: partial.incoming_standalone,
        outgoing_standalone: partial.outgoing_standalone,
    })
}

/// Applies a `name = <type>` entry.
///
/// # Errors
///
/// Spanned errors for hook names given as types, unknown fields, and
/// duplicate `params`/`response`.
fn apply_type(spec: &mut PartialSpec, name: &Ident, ty: Type) -> syn::Result<()> {
    match name.to_string().as_str() {
        "params" => set(&mut spec.params, ty, name),
        "response" => set(&mut spec.response, ty, name),
        _ if hook_shape(name).is_some() => Err(syn::Error::new_spanned(
            name,
            format!("`{name}` takes a parenthesized list: {name}(...)"),
        )),
        _ => Err(unknown_field(name)),
    }
}

/// Applies a `name(<tokens>)` entry: the payload shape comes from the
/// [`HOOKS`] table, the slot from the field name.
///
/// # Errors
///
/// Spanned errors for unknown fields, `params`/`response` given as lists,
/// and malformed or duplicate hook payloads.
fn apply_hook(spec: &mut PartialSpec, name: &Ident, tokens: TokenStream) -> syn::Result<()> {
    let Some(shape) = hook_shape(name) else {
        return match name.to_string().as_str() {
            "params" | "response" => Err(syn::Error::new_spanned(
                name,
                format!("`{name}` takes a type: {name} = <type path>"),
            )),
            _ => Err(unknown_field(name)),
        };
    };
    match shape {
        Shape::FieldPath => {
            let expr: Expr = syn::parse2(tokens)?;
            let segments = field_path(&expr)?;
            match name.to_string().as_str() {
                "document" => set(&mut spec.document, segments, name),
                "incoming_position" => set(&mut spec.incoming_position, segments, name),
                _ => set(&mut spec.incoming_range, segments, name),
            }
        }
        Shape::FunctionPath => {
            let path: Path = syn::parse2(tokens)?;
            match name.to_string().as_str() {
                "incoming_custom" => set(&mut spec.incoming_custom, path, name),
                "outgoing" => set(&mut spec.outgoing, path, name),
                "incoming_standalone" => set(&mut spec.incoming_standalone, path, name),
                _ => set(&mut spec.outgoing_standalone, path, name),
            }
        }
    }
}

/// The payload shape the named field takes when written as a list.
fn hook_shape(name: &Ident) -> Option<Shape> {
    let name = name.to_string();
    HOOKS
        .iter()
        .find(|(hook, _)| *hook == name)
        .map(|(_, shape)| *shape)
}

fn unknown_field(name: &Ident) -> syn::Error {
    syn::Error::new_spanned(
        name,
        format!("unknown or malformed lsp_request field `{name}`"),
    )
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
                    return Err(syn::Error::new_spanned(
                        index,
                        "expected named fields, not tuple indices",
                    ));
                }
            }
            Ok(segments)
        }
        Expr::Path(path)
            if path.path.segments.len() == 1 && path.path.segments[0].arguments.is_none() =>
        {
            Ok(vec![path.path.segments[0].ident.clone()])
        }
        other => Err(syn::Error::new_spanned(
            other,
            "expected a field path like `text_document_position_params.position`",
        )),
    }
}

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
    fn field_path_rejects_tuple_index() {
        let expr = syn::parse2(quote! { a.0 }).expect("parses");
        let err = field_path(&expr).expect_err("rejected");
        assert!(err.to_string().contains("named fields"));
    }

    #[test]
    fn minimal_attribute_emits_struct_and_impl() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let out = expand(
            quote! {
                params = P,
                response = Option<R>,
            },
            &item,
        )
        .expect("expands");
        let text = out.to_string();
        assert!(text.contains("impl crate :: requests :: Request for X"));
        assert!(text.contains("type Params = P"));
        assert!(text.contains("type Response = Option < R >"));
        assert!(!text.contains("extract_url"));
    }

    #[test]
    fn full_wiring_emits_every_hook() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let out = expand(
            quote! {
                params = P,
                response = R,
                document(a.b),
                incoming_position(a.c),
                outgoing(f),
                incoming_standalone(g),
                outgoing_standalone(h),
            },
            &item,
        )
        .expect("expands");
        let text = out.to_string();
        for needle in [
            "extract_url",
            "modify_params",
            "modify_response",
            "modify_params_standalone",
            "modify_response_standalone",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn incoming_position_only_omits_unused_converters() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        let out = expand(
            quote! {
                params = P,
                response = R,
                incoming_position(a.b),
            },
            &item,
        )
        .expect("expands");
        let text = out.to_string();
        assert!(text.contains("convert_position"));
        assert!(text.contains("Direction"));
        assert!(!text.contains("convert_range"));
    }

    /// Parses the unit-struct fixture, expands `attr` against it, and
    /// returns the expected error.
    fn expansion_error(attr: proc_macro2::TokenStream) -> syn::Error {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        expand(attr, &item).expect_err("rejected")
    }

    #[test]
    fn unknown_field_is_spanned_error() {
        let err = expansion_error(quote! { params = P, response = R, bogus(x) });
        assert!(err.to_string().contains("unknown or malformed"));
    }

    #[test]
    fn missing_params_is_error() {
        let item = syn::parse2(quote! { pub struct X; }).expect("struct");
        assert!(expand(quote! { response = R }, &item).is_err());
    }

    #[test]
    fn duplicate_field_is_error() {
        let err = expansion_error(quote! { params = P, params = Q, response = R });
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn generics_are_rejected() {
        let item = syn::parse2(quote! { pub struct X<T>; }).expect("struct");
        let err = expand(quote! { params = P, response = R }, &item).expect_err("rejected");
        assert!(err.to_string().contains("non-generic"));
    }
}
