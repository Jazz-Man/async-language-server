use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, ItemStruct, LitStr, Meta, MetaNameValue, Path, Type, parse_macro_input};

#[proc_macro_attribute]
pub fn lsp_request(attr: TokenStream, item: TokenStream) -> TokenStream {
    // 1. Парсимо саму структуру (наприклад, `pub struct HoverRequest;`)
    let input = parse_macro_input!(item as ItemStruct);
    let struct_name = &input.ident;

    // 2. Парсимо аргументи атрибута
    let meta = syn::parse::Parser::parse2(
        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
        attr.into(),
    )
    .expect("Failed to parse lsp_request attributes");

    let mut method = None;
    let mut params_ty = None;
    let mut response_ty = None;
    let mut outgoing_fn = None;

    for m in meta {
        if let Meta::NameValue(MetaNameValue { path, value, .. }) = m {
            if path.is_ident("method") {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) = value
                {
                    method = Some(lit.value());
                }
            } else if path.is_ident("params") {
                // Парсимо тип з виразу (оскільки в атрибутах syn читає це як Expr)
                params_ty = Some(syn::parse2::<Type>(value.to_token_stream()).unwrap());
            } else if path.is_ident("response") {
                response_ty = Some(syn::parse2::<Type>(value.to_token_stream()).unwrap());
            } else if path.is_ident("outgoing") {
                if let syn::Expr::Path(expr_path) = value {
                    outgoing_fn = Some(expr_path.path);
                }
            }
        }
    }

    let method_lit = method.expect("Missing 'method' in lsp_request");
    let params = params_ty.expect("Missing 'params' in lsp_request");
    let response = response_ty.expect("Missing 'response' in lsp_request");

    // Генеруємо фінальний код
    let expanded = if let Some(outgoing) = outgoing_fn {
        quote! {
            // Зберігаємо оригінальну структуру (IDE ідеально її розуміє)
            #input

            // Генеруємо реалізацію трейту
            impl crate::requests::Request for #struct_name {
                type Params = #params;
                type Response = #response;

                fn method() -> &'static str {
                    #method_lit
                }

                fn modify_response(
                    state: &crate::server::ServerState,
                    document: &crate::server::Document,
                    response: &mut Self::Response,
                ) {
                    // Це звичайний виклик функції за шляхом.
                    // Go To Definition працюватиме на 100%.
                    #outgoing(state, document, response);
                }
            }

            // Опціонально: автоматична реєстрація
            // crate::requests::registry::register::<#struct_name>();
        }
    } else {
        // Якщо outgoing не вказано, генеруємо impl без цієї функції (або з дефолтною)
        quote! {
            #input
            impl crate::requests::Request for #struct_name {
                type Params = #params;
                type Response = #response;
                fn method() -> &'static str { #method_lit }
            }
        }
    };

    TokenStream::from(expanded)
}
