#![doc = include_str!("../README.md")]

pub use async_lsp::lsp_types;

#[cfg(feature = "tree-sitter")]
pub use tree_sitter;

mod document;
mod document_matcher;
mod error;
mod requests;
mod serve;
mod server_options;
mod server_state;
mod server_trait;
mod server_with_state;
mod transport;
mod workspace_diagnostics;
mod workspace_walker;

pub mod oneshot;
pub mod text_utils;

#[cfg(feature = "tree-sitter")]
pub mod tree_sitter_utils;

pub mod server {
    //! High-level API for implementing language servers.
    //!
    //! Implement [`Server`] with only the methods you need, configure
    //! [`ServerOptions`] and [`DocumentMatcher`]s, then run it with [`serve`]
    //! over a [`Transport`]. Each request receives a [`ServerState`], which
    //! tracks open documents as [`Document`] snapshots.
    //!
    //! All [`Server`] methods work with UTF-8 positions regardless of the
    //! encoding negotiated with the client — conversions between UTF-8,
    //! UTF-16, and UTF-32 are handled internally.

    pub use crate::document::{Document, DocumentReader};
    pub use crate::document_matcher::DocumentMatcher;
    pub use crate::error::{ServerError, ServerErrorCode, ServerResult};
    pub use crate::serve::serve;
    pub use crate::server_options::{
        ConfigurationKey, ServerOptions, WorkspaceDiagnostics, WorkspaceDiagnosticsSetting,
    };
    pub use crate::server_state::ServerState;
    pub use crate::server_trait::Server;
    pub use crate::transport::Transport;

    #[cfg(feature = "tree-sitter")]
    pub use crate::document::DocumentQueryCapture;
}
