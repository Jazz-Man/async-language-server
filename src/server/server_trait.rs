use async_lsp::{
    ErrorCode,
    lsp_types::{ClientCapabilities, ServerCapabilities, ServerInfo},
};

use crate::{
    documents::DocumentMatcher,
    error::{ServerError, ServerResult},
    server::{ServerOptions, ServerState},
};

/// Stamps `Server` trait methods for registry rows (normal methods default
/// to `METHOD_NOT_FOUND`; hook fields are matched and discarded here).
macro_rules! registry_trait_methods {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
            $(document: $($dseg:ident).+,)?
            $(incoming: position at $($pseg:ident).+,)?
            $(incoming: range at $($rseg:ident).+,)?
            $(outgoing: $outgoing:ident,)?
        }
    )*) => {
        $(
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                _params: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                method_not_implemented(stringify!($trait_name))
            }
        )*
    };
}

/// Stamps `Server` trait methods for resolve rows (default: item unchanged).
macro_rules! registry_trait_resolve_methods {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
        }
    )*) => {
        $(
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                item: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                async move { Ok(item) }
            }
        )*
    };
}

/// The main entrypoint to LSP functionality for a server.
///
/// All of the LSP methods in this trait are optional - if implemented,
/// the respective capabilities must also be registered using the
/// `server_capabilities` function.
///
/// The only exception to this rule are the `*_resolve` methods, which
/// default to doing nothing, and simply resolving the item as-is.
///
/// Handlers report failures by returning `Err(ServerError)`; the wrapper
/// converts them to LSP error responses.
pub trait Server {
    /// Returns the server name and version reported to the client during initialization.
    #[must_use]
    fn server_info() -> Option<ServerInfo> {
        None
    }

    /// Returns the options configuring this crate's behavior, read once during initialization.
    fn server_options(&self) -> ServerOptions {
        ServerOptions::default()
    }

    /// Returns the capabilities to advertise to the client during initialization.
    ///
    /// Merge the capabilities required by the implemented [`Server`] methods
    /// into a [`ServerCapabilities`] value. Returning `None` advertises only
    /// the crate's defaults.
    #[must_use]
    fn server_capabilities(_client_capabilities: ClientCapabilities) -> Option<ServerCapabilities> {
        None
    }

    /// Returns the matchers that associate documents with languages and grammars.
    ///
    /// See [`DocumentMatcher`] for how documents are matched.
    #[must_use]
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![]
    }

    crate::requests::registry::generated_methods!(registry_trait_methods);
    crate::requests::registry::custom_methods!(registry_trait_methods);
    crate::requests::registry::resolve_methods!(registry_trait_resolve_methods);
}

fn method_not_implemented<T>(name: &'static str) -> std::future::Ready<Result<T, ServerError>> {
    std::future::ready(Err(ServerError::rpc(
        ErrorCode::METHOD_NOT_FOUND,
        format!("LSP method '{name}' has not been implemented"),
    )))
}
