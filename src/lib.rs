#![doc = include_str!("../README.md")]

pub use async_lsp::lsp_types;

#[cfg(feature = "tree-sitter")]
pub use tree_sitter;

mod documents;
mod error;
mod requests;
mod transport;
mod workspace;

pub mod oneshot;
pub mod text_utils;

#[cfg(feature = "tree-sitter")]
pub mod tree_sitter_utils;

pub mod server;
