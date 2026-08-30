use std::num::NonZeroUsize;

use async_lsp::{
    client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    panic::CatchUnwindLayer, router::Router, server::LifecycleLayer,
};
use tower::ServiceBuilder;

#[cfg(feature = "tracing")]
use async_lsp::tracing::TracingLayer;

use crate::{
    error::ServerResult,
    server::{LanguageServerWithState, Server},
    transport::Transport,
};

const MAX_CONCURRENT_REQUESTS: NonZeroUsize = match NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};

/// Serves a language server over the given transport.
///
/// The server must be clonable, and shareable across threads.
///
/// This will automatically attach middleware for:
///
/// - Tracing metadata for each request
/// - Maximum concurrency of 8 in-flight LSP requests at a time
/// - Catching panics and safely returning internal server error statuses
/// - Client process monitoring and automatic server shutdown when client exits
///
/// # Examples
///
/// A stdio server cannot run inside a doctest, so this example only compiles:
///
/// ```no_run
/// use async_language_server::server::{Transport, serve};
/// # #[derive(Clone)]
/// # struct MyServer;
/// # impl async_language_server::server::Server for MyServer {}
/// # #[tokio::main]
/// # async fn main() -> async_language_server::server::ServerResult<()> {
/// serve(Transport::Stdio, MyServer).await
/// # }
/// ```
///
/// # Errors
///
/// - If the transport uses a socket and it could not connect
/// - If the server encounters an I/O error while running
pub async fn serve<S>(transport: Transport, server: S) -> ServerResult<()>
where
    S: Server + Clone,
    S: Send + Sync + 'static,
{
    let (reader, writer) = transport.into_read_write().await?;

    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let builder = ServiceBuilder::new().layer(LifecycleLayer::default());

        #[cfg(feature = "tracing")]
        let builder = builder.layer(TracingLayer::default());

        builder
            .layer(ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS))
            .layer(CatchUnwindLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(Router::from_language_server(LanguageServerWithState::new(
                client,
                server.clone(),
            )))
    });

    server
        .run_buffered(reader, writer)
        .await
        .map_err(Into::into)
}
