//! Procedural macros for `async-language-server`.
//!
//! Workspace-internal build plumbing — not part of the crate's public
//! surface. The macros arrive in the next plan: `#[lsp_request]`
//! (per-file request registration), `lsp_dispatch!` (dispatch entries for
//! the async-lsp `LanguageServer` impl), `lsp_method!` /
//! `lsp_resolve_method!` (`Server`-trait default bodies), and
//! `conversion_tests!` (W0 conversion-test stamping).
//!
//! Crate rules: input errors are reported as span-accurate
//! [`syn::Error`]s on the offending token — never `panic!`, `expect`, or
//! `unwrap` (`macro-proc-error-spans`); emitted code references call-site
//! paths, which resolve inside the main crate only.
