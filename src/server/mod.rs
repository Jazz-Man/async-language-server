//! High-level API for implementing language servers.
//!
//! Implement [`Server`] with only the methods you need, configure
//! [`ServerOptions`] and [`DocumentMatcher`]s, then run it with [`serve`].
//! Each request receives a [`ServerState`], which tracks open documents as
//! [`Document`] snapshots.
//!
//! All [`Server`] methods work with UTF-8 positions regardless of the
//! encoding negotiated with the client — conversions between UTF-8,
//! UTF-16, and UTF-32 are handled internally.

mod options;
mod serve;
mod server_trait;
mod state;
mod with_state;

#[cfg(test)]
mod tests;

pub use self::options::{
    ConfigurationKey, ServerOptions, WorkspaceDiagnostics, WorkspaceDiagnosticsSetting,
};
pub use self::serve::serve;
pub use self::server_trait::Server;
pub use self::state::ServerState;
pub use crate::documents::DocumentMatcher;
pub use crate::documents::{Document, DocumentReader};
pub use crate::error::{RangeError, ServerError, ServerErrorCode, ServerResult};

#[cfg(feature = "tree-sitter")]
pub use crate::documents::DocumentQueryCapture;

pub(crate) use self::with_state::LanguageServerWithState;
