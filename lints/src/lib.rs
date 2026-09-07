//! `strict-lints`: portable dylint lints shared across the owner's Rust
//! projects. See `lints/README.md` for adoption.

#![feature(rustc_private)]
// `rustc_private` crates come from the sysroot (`rustc-dev` component of the
// pinned nightly), not crates.io — see `lints/rust-toolchain.toml`.
extern crate rustc_lint;
extern crate rustc_session;

dylint_linting::dylint_library!();

/// Registers this library's lints with the compiler (empty until Task 2).
///
/// The dylint driver resolves this symbol by name in the built library, hence
/// the `#[unsafe(no_mangle)]` (the pattern from `dylint_linting`'s docs).
#[unsafe(no_mangle)]
pub fn register_lints(_sess: &rustc_session::Session, _lint_store: &mut rustc_lint::LintStore) {}
