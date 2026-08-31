use std::path::PathBuf;

use async_lsp::ResponseError;
use thiserror::Error;

type BoxDynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use async_lsp::ErrorCode as ServerErrorCode;

/// Convenience `Result` alias for operations that can fail with a [`ServerError`].
pub type ServerResult<T> = Result<T, ServerError>;

/// Failures of [`RangeExt`](crate::text_utils::RangeExt) operations.
///
/// A leaf-utility error without protocol semantics: it never crosses the
/// wire itself and is mapped by the caller at their own boundary
/// (absorbable into [`ServerError::Other`] by boxing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RangeError {
    /// The position lies beyond the end of the range.
    #[error("position lies beyond the end of the range")]
    PositionOutOfRange,
    /// The subrange start lies after its end.
    #[error("subrange start lies after its end")]
    StartAfterEnd,
    /// `shrink` was called on a range that spans multiple lines.
    #[error("shrink requires a single-line range")]
    NotSingleLine,
    /// The delimiter is not a single-byte UTF-8 character.
    #[error("delimiter {delimiter:?} is not a single-byte UTF-8 character")]
    DelimiterNotSingleByte {
        /// The offending delimiter.
        delimiter: char,
    },
    /// The text is not the exact text of the range.
    #[error("text length {text_len} does not match range length {range_len}")]
    TextRangeMismatch {
        /// Length of the text in bytes.
        text_len: usize,
        /// Length of the range.
        range_len: usize,
    },
}

/// Failures of [`Document::query`](crate::server::Document::query).
///
/// A leaf-utility error without protocol semantics, like [`RangeError`]:
/// it never crosses the wire itself and is mapped by the caller at their
/// own boundary.
#[cfg(feature = "tree-sitter")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QueryError {
    /// The document has no tree-sitter language or parsed tree attached.
    #[error("document has no tree-sitter language or parsed tree")]
    NoTree,
    /// The query string failed to compile.
    #[error("invalid tree-sitter query")]
    InvalidQuery {
        /// The underlying compilation error.
        #[source]
        error: tree_sitter::QueryError,
    },
}

/// An error that can occur while running a language server.
///
/// # Examples
///
/// ```
/// use async_language_server::server::ServerError;
///
/// let error = ServerError::TcpConnect {
///     port: 9999,
///     error: std::io::Error::other("connection refused"),
/// };
/// assert_eq!(error.to_string(), "failed to connect to port 9999");
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerError {
    /// Failed to connect a socket to the given TCP port.
    #[error("failed to connect to port {port}")]
    TcpConnect {
        /// The port that was being connected to.
        port: u16,
        /// The underlying connect error.
        #[source]
        error: std::io::Error,
    },
    /// A file path could not be represented as a `file://` URL.
    #[error("invalid file path '{path}'")]
    InvalidFilePath {
        /// The path that could not be converted.
        path: PathBuf,
    },
    /// JSON-RPC error sent to or received from the client.
    #[error("json-rpc error {code}: {message}")]
    Rpc {
        /// The JSON-RPC error code.
        code: ServerErrorCode,
        /// The JSON-RPC error message.
        message: String,
    },
    /// Error raised by the underlying async-lsp machinery.
    #[error("{0}")]
    Lsp(#[from] async_lsp::Error),
    /// I/O error raised by a transport or a file read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// An error that does not fit any other variant; the boxed error provides
    /// the `Display` message and is exposed as the `source()` chain node.
    #[error("{0}")]
    Other(#[from] BoxDynError),
}

impl ServerError {
    /// Creates a JSON-RPC error with the given code and message.
    #[must_use]
    pub fn rpc(code: ServerErrorCode, message: String) -> Self {
        ServerError::Rpc { code, message }
    }
}

impl From<ServerError> for ResponseError {
    fn from(value: ServerError) -> Self {
        match value {
            ServerError::Rpc { code, message } => ResponseError::new(code, message),
            other => ResponseError::new(ServerErrorCode::INTERNAL_ERROR, other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use async_lsp::{ErrorCode, ResponseError};

    use super::ServerError;

    #[test]
    fn tcp_connect_preserves_its_source() {
        let error = ServerError::TcpConnect {
            port: 9999,
            error: std::io::Error::other("connection refused"),
        };

        assert_eq!(error.to_string(), "failed to connect to port 9999");
        assert_eq!(error.source().unwrap().to_string(), "connection refused");
    }

    #[test]
    fn io_errors_preserve_their_source() {
        let error = ServerError::Io(std::io::Error::other("disk gone"));

        assert_eq!(error.to_string(), "disk gone");
        assert_eq!(error.source().unwrap().to_string(), "disk gone");
    }

    #[test]
    fn other_preserves_its_boxed_source() {
        let error = ServerError::Other(Box::new(std::io::Error::other("boom")));

        assert_eq!(error.to_string(), "boom");
        assert_eq!(error.source().unwrap().to_string(), "boom");
    }

    #[test]
    fn rpc_errors_map_to_their_own_code() {
        let response =
            ResponseError::from(ServerError::rpc(ErrorCode::METHOD_NOT_FOUND, "nope".into()));

        assert_eq!(response.code, ErrorCode::METHOD_NOT_FOUND);
        assert_eq!(response.message, "nope");
    }

    #[test]
    fn other_errors_map_to_internal_error() {
        let response =
            ResponseError::from(ServerError::Other(Box::new(std::io::Error::other("boom"))));

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "boom");
    }

    #[test]
    fn lsp_errors_map_to_internal_error() {
        let response = ResponseError::from(ServerError::Lsp(async_lsp::Error::Eof));

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
    }

    #[test]
    fn invalid_file_path_maps_to_internal_error() {
        let response = ResponseError::from(ServerError::InvalidFilePath {
            path: std::path::PathBuf::from("/bad"),
        });

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "invalid file path '/bad'");
    }

    #[test]
    fn tcp_connect_maps_to_internal_error() {
        let response = ResponseError::from(ServerError::TcpConnect {
            port: 9999,
            error: std::io::Error::other("connection refused"),
        });

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "failed to connect to port 9999");
    }

    #[test]
    fn io_errors_map_to_internal_error() {
        let response = ResponseError::from(ServerError::Io(std::io::Error::other("disk gone")));

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "disk gone");
    }
}
