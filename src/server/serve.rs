use crate::error::ServerResult;
use crate::server::{LanguageServerWithState, Server};
use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::router::Router;
use async_lsp::server::LifecycleLayer;
use async_lsp::tracing::TracingLayer;
use futures::{AsyncRead, AsyncWrite};
use tower::ServiceBuilder;

/// Serves a language server over the process standard input and output.
///
/// The server must be clonable, and shareable across threads.
///
/// The standard input and output are locked as non-blocking pipes through
/// async-lsp's `PipeStdin`/`PipeStdout`: this entry point exists on unix
/// only, and nothing may leave bytes in the std buffered stdin/stdout (the
/// `print!` family) alongside the server.
///
/// This will automatically attach middleware for:
///
/// - Tracing metadata for each request
/// - In-flight LSP requests bounded by the CPU core count
/// - Catching panics and safely returning internal server error statuses
/// - Client process monitoring and automatic server shutdown when client exits
///
/// # Examples
///
/// A stdio server cannot run inside a doctest, so this example only compiles:
///
/// ```no_run
/// use async_language_server::server::serve;
/// # #[derive(Clone)]
/// # struct MyServer;
/// # impl async_language_server::server::Server for MyServer {}
/// # #[tokio::main]
/// # async fn main() -> async_language_server::server::ServerResult<()> {
/// serve(MyServer).await
/// # }
/// ```
///
/// # Errors
///
/// If the standard input or output cannot be locked as a pipe-like channel
/// (for example, when redirected to a regular file), or the server
/// encounters an I/O error while running.
pub async fn serve<S>(server: S) -> ServerResult<()>
where
    S: Server + Clone,
    S: Send + Sync + 'static,
{
    let (stdin, stdout) = (
        async_lsp::stdio::PipeStdin::lock_tokio()?,
        async_lsp::stdio::PipeStdout::lock_tokio()?,
    );
    run_over_streams(server, stdin, stdout).await
}

/// Runs the real middleware stack (lifecycle, tracing, concurrency,
/// panic catching, client-process monitor) over arbitrary futures-trait
/// byte streams.
///
/// `serve()` runs it over the process stdio pipes; the wire-tier tests
/// (`src/server/tests/`) drive the same stack over in-memory duplex
/// pipes, so the tested stack can never drift from the shipped one.
pub(crate) async fn run_over_streams<S, R, W>(server: S, reader: R, writer: W) -> ServerResult<()>
where
    S: Server + Clone + Send + Sync + 'static,
    R: AsyncRead,
    W: AsyncWrite,
{
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let builder = ServiceBuilder::new()
            .layer(LifecycleLayer::default())
            .layer(TracingLayer::default());

        builder
            .layer(ConcurrencyLayer::default())
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
