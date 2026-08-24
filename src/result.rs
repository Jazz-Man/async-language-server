#![allow(clippy::needless_pass_by_value)]

use async_lsp::ResponseError;
use thiserror::Error;

type BoxDynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use async_lsp::ErrorCode as ServerErrorCode;

/// Convenience `Result` alias for operations that can fail with a [`ServerError`].
pub type ServerResult<T> = Result<T, ServerError>;

/// An error that can occur while running a language server.
///
/// # Examples
///
/// ```
/// use async_language_server::server::ServerError;
///
/// let error = ServerError::TcpConnect(9999);
/// assert_eq!(error.to_string(), "Failed to connect to port 9999");
///
/// let error = ServerError::from("boom");
/// assert_eq!(error.to_string(), "Uncategorized error: boom");
/// ```
#[derive(Debug, Error)]
pub enum ServerError {
    /// Failed to connect to the given TCP port.
    #[error("Failed to connect to port {0}")]
    TcpConnect(u16),
    /// Error that does not fit any other variant.
    #[error("Uncategorized error: {0}")]
    Unknown(String),
    /// JSON-RPC error sent to or received from the client.
    #[error("JSON RPC error: {0}")]
    Rpc(ServerErrorCode, String),
    /// Error raised by the underlying async-lsp machinery.
    #[error(transparent)]
    Lsp(#[from] async_lsp::Error),
    /// I/O error raised by a transport or a file read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ServerError {
    /// Wraps an arbitrary error as a [`ServerError::Unknown`].
    pub fn unknown(error: impl Into<BoxDynError>) -> Self {
        ServerError::Unknown(error.into().to_string())
    }

    /// Creates a JSON-RPC error with the given code and message.
    pub fn rpc(code: ServerErrorCode, message: impl ToString) -> Self {
        ServerError::Rpc(code, message.to_string())
    }
}

// From string-like errors to ServerError

impl From<String> for ServerError {
    fn from(error: String) -> Self {
        ServerError::Unknown(error)
    }
}

impl From<&String> for ServerError {
    fn from(error: &String) -> Self {
        ServerError::Unknown(error.clone())
    }
}

impl From<&str> for ServerError {
    fn from(error: &str) -> Self {
        ServerError::Unknown(error.to_string())
    }
}

impl From<BoxDynError> for ServerError {
    fn from(error: BoxDynError) -> Self {
        ServerError::Unknown(error.to_string())
    }
}

// From ServerError to the lsp ResponseError

impl From<ServerError> for ResponseError {
    fn from(value: ServerError) -> Self {
        if let ServerError::Rpc(code, message) = value {
            ResponseError::new(code, message)
        } else if let ServerError::Unknown(message) = value {
            ResponseError::new(ServerErrorCode::UNKNOWN_ERROR_CODE, message)
        } else {
            ResponseError::new(ServerErrorCode::INTERNAL_ERROR, value.to_string())
        }
    }
}
