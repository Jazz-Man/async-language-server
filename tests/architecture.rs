//! Architecture checks via `arch-lint`.
//!
//! The macro expands to a `#[test]` function, so the checks ride every
//! `cargo test` run. Built-in rules come from the `recommended` preset;
//! layer rules and per-item suppressions (comment form, mandatory reason)
//! live in `arch-lint.toml` at the crate root.

arch_lint::check!(preset = "recommended"); // expands to a #[test] function
