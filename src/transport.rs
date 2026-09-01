use std::{
    fmt,
    io::Result,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};
use tokio::{
    io::{AsyncRead as _, AsyncWrite as _, ReadBuf, Stdin, Stdout},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};

use crate::error::{ServerError, ServerResult};

/// Transport implementation for sockets and stdio.
///
/// # Examples
///
/// ```
/// use async_language_server::server::Transport;
///
/// assert_eq!(Transport::Stdio.to_string(), "Stdio");
/// assert_eq!(Transport::Socket(9999).to_string(), "Socket(9999)");
/// ```
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
#[deprecated(
    note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md"
)]
pub enum Transport {
    /// Connects to a TCP socket on the given port of `127.0.0.1`.
    Socket(u16),
    /// Uses the process standard input and output.
    #[default]
    Stdio,
}

impl Transport {
    /// Creates the reader and writer for the transport.
    ///
    /// # Errors
    ///
    /// - If the `Socket` transport is used and connecting to
    ///   `127.0.0.1:{port}` fails.
    ///
    /// # Panics
    ///
    /// Panics on a `Transport` variant other than [`Transport::Socket`] and
    /// [`Transport::Stdio`]. There is no such variant today; the branch
    /// exists so a future variant fails loudly instead of silently.
    pub async fn into_read_write(self) -> ServerResult<(LspTransportRead, LspTransportWrite)> {
        if let Self::Socket(port) = self {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));

            let stream = TcpStream::connect(addr)
                .await
                .map_err(|error| ServerError::TcpConnect { port, error })?;

            let (stream_read, stream_write) = stream.into_split();

            Ok((
                LspTransportRead::Socket(stream_read),
                LspTransportWrite::Socket(stream_write),
            ))
        } else if let Self::Stdio = self {
            Ok((
                LspTransportRead::Stdio(tokio::io::stdin()),
                LspTransportWrite::Stdio(tokio::io::stdout()),
            ))
        } else {
            unreachable!()
        }
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio => write!(f, "Stdio"),
            Self::Socket(p) => write!(f, "Socket({p})"),
        }
    }
}

/// The read half of an LSP transport.
#[derive(Debug)]
#[deprecated(
    note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md"
)]
pub enum LspTransportRead {
    /// Read half of a connected [`Transport::Socket`].
    Socket(OwnedReadHalf),
    /// Read half of [`Transport::Stdio`].
    Stdio(Stdin),
}

impl AsyncRead for LspTransportRead {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);

        let poll_result = match self.get_mut() {
            Self::Socket(s) => Pin::new(s).poll_read(cx, &mut read_buf),
            Self::Stdio(s) => Pin::new(s).poll_read(cx, &mut read_buf),
        };

        match poll_result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        }
    }
}

/// The write half of an LSP transport.
#[derive(Debug)]
#[deprecated(
    note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md"
)]
pub enum LspTransportWrite {
    /// Write half of a connected [`Transport::Socket`].
    Socket(OwnedWriteHalf),
    /// Write half of [`Transport::Stdio`].
    Stdio(Stdout),
}

impl AsyncWrite for LspTransportWrite {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        match self.get_mut() {
            Self::Socket(s) => Pin::new(s).poll_write(cx, buf),
            Self::Stdio(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        match self.get_mut() {
            Self::Socket(s) => Pin::new(s).poll_flush(cx),
            Self::Stdio(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        match self.get_mut() {
            Self::Socket(s) => Pin::new(s).poll_shutdown(cx),
            Self::Stdio(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}
