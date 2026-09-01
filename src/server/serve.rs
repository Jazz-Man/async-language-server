use std::{
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
};

use async_lsp::{
    client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    panic::CatchUnwindLayer, router::Router, server::LifecycleLayer,
};
use futures::{AsyncRead, AsyncWrite};
use tokio::io::{AsyncRead as _, AsyncWrite as _, ReadBuf, Stdin, Stdout};
use tower::ServiceBuilder;

#[cfg(feature = "tracing")]
use async_lsp::tracing::TracingLayer;

use crate::{
    error::ServerResult,
    server::{LanguageServerWithState, Server},
};

const MAX_CONCURRENT_REQUESTS: NonZeroUsize = match NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};

/// Serves a language server over the process standard input and output.
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
/// If the server encounters an I/O error while running.
pub async fn serve<S>(server: S) -> ServerResult<()>
where
    S: Server + Clone,
    S: Send + Sync + 'static,
{
    run_over_streams(
        server,
        StdinAdapter(tokio::io::stdin()),
        StdoutAdapter(tokio::io::stdout()),
    )
    .await
}

/// Runs the real middleware stack (lifecycle, tracing, concurrency,
/// panic catching, client-process monitor) over arbitrary futures-trait
/// byte streams.
///
/// `serve()` runs it over the process stdio; the wire-tier tests
/// (`src/server/tests.rs`) drive the same stack over in-memory duplex
/// pipes, so the tested stack can never drift from the shipped one.
pub(crate) async fn run_over_streams<S, R, W>(server: S, reader: R, writer: W) -> ServerResult<()>
where
    S: Server + Clone + Send + Sync + 'static,
    R: AsyncRead,
    W: AsyncWrite,
{
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

/// Bridges tokio's stdin to the futures `AsyncRead` the loop speaks.
struct StdinAdapter(Stdin);

impl AsyncRead for StdinAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut self.get_mut().0).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        }
    }
}

/// Bridges tokio's stdout to the futures `AsyncWrite` the loop speaks.
struct StdoutAdapter(Stdout);

impl AsyncWrite for StdoutAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}
