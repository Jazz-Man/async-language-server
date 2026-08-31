#![doc = include_str!("../README.md")]

pub use async_lsp::lsp_types;

#[cfg(feature = "tree-sitter")]
pub use tree_sitter;

mod documents;
mod error;
mod requests;
mod transport;
mod workspace;

// The single shared test-support home for every inline test module
// (`src/testing.rs`, scopeless like `src/error.rs`).
#[cfg(test)]
pub(crate) mod testing;

pub mod oneshot;
pub mod text_utils;

#[cfg(feature = "tree-sitter")]
pub mod tree_sitter_utils;

pub mod server;
