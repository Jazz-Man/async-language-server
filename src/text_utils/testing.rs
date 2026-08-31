//! Shared LSP test fixtures for inline test modules across the crate.
//!
//! Declared as a `#[cfg(test)]` module in `text_utils/mod.rs`; never
//! compiled into non-test builds. Lives in `text_utils` — the leaf of the
//! layer graph — so both `requests` and `server` tests can import it.
//!
//! The byte- and tree-sitter `r()` helpers are deliberately not here: they
//! share a name with the LSP `r()` but differ in types, so their test
//! modules keep local definitions.

use async_lsp::lsp_types::{Position, Range, Url};

/// Builds an LSP [`Position`] with the given line and character.
pub(crate) const fn p(line: u32, character: u32) -> Position {
    Position { line, character }
}

/// Builds an LSP [`Range`] spanning from `start` to `end`.
pub(crate) const fn r(start: Position, end: Position) -> Range {
    Range { start, end }
}

/// Builds a `file:///tmp/{path}` document URL.
pub(crate) fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}
