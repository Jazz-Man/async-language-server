mod document;
mod matcher;

#[cfg(feature = "tree-sitter")]
pub use document::DocumentQueryCapture;
pub use document::{Document, DocumentReader};
pub use matcher::DocumentMatcher;
pub(crate) use matcher::DocumentMatchers;
