# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Library crate (no binary) that wraps `async-lsp` to make language servers with less boilerplate: async-lsp pipe-based stdio transport, ropey-based incremental document sync, automatic position-encoding negotiation (UTF-8/16/32), and optional tree-sitter integration. Personal project, version 0.0.0, not published to crates.io — consumed as a git dependency or fork. Public API lives under `async_language_server::server::*`, with `lsp_types` re-exported at the crate root.

## Commands

- Local verification is `make battery` (CI parity); `make help` lists all targets
- CI (`.github/workflows/ci.yml`) runs on push/PR to `main` and via `workflow_dispatch` on any branch: a `checks` job (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`) plus a two-leg test matrix (`cargo test --workspace --all-features` and `--no-default-features`) in parallel, and a `dylint` job (`cargo dylint --all -- --all-targets`, stock Trail of Bits + `perfectionist` suites via pinned nightly) that the release job gates on, with Cargo caching (`rust-cache`, saves on `main` only); on pushes to `main`, a release job tags (`v` prefix; `#major`/`#minor`/`#patch`/`#none` commit-message tokens) and publishes a GitHub Release with changelog. The `default` test leg returns when a second, non-default feature exists (today `default` ≡ `--all-features`)
- `cargo nextest run <filter>` — run a single test (case rows and target scoping need `-E` selectors — see `.claude/rules/testing.md`); tests are inline `#[cfg(test)] mod tests` blocks inside each `src/` module, or sibling `tests.rs` files for the larger modules
- `make clippy` — `clippy::all` is `deny` in `Cargo.toml` `[workspace.lints.clippy]`, so default lints are hard errors; `pedantic`/`cargo` deny, with an explicit allow list there
- `make dylint` — nightly-pinned dylint pass running the stock Trail of Bits suites + the third-party `perfectionist` suite (configured in `dylint.toml`); part of the battery
- `make deny` — cargo-deny dependency-hygiene policies over `deny.toml` (advisories, duplicate bans, licenses, sources); on demand, not in the battery — deny-level findings gate, warnings advise
- `make fmt` — rustfmt and clippy are pinned via `rust-toolchain.toml`
- Feature gates matter: the default feature is `tree-sitter` (`tracing` is permanent, not a feature). Changes touching `#[cfg(feature = "tree-sitter")]` paths should also be checked with `make test-no-default-features`
- Doctests are enabled and run as part of `make test` and `make test-no-default-features` in every feature configuration CI runs; keep them free of tree-sitter-gated API

## Architecture

Two layers around async-lsp:

1. **User layer — `Server` trait** (`src/server/server_trait.rs`): implementors override async methods (`hover`, `completion`, `definition`, `document_diagnostics`, ...). All optional; unimplemented ones return `METHOD_NOT_FOUND`. `serve()` (`src/server/serve.rs`) wires the implementor into async-lsp's `MainLoop` behind a tower middleware stack (lifecycle, tracing, a concurrency limit derived from the CPU core count, panic catching, client-process monitor) over the process stdio.

2. **Plumbing — `LanguageServerWithState`** (`src/server/with_state/mod.rs`, initialize flow in `src/server/with_state/initialize.rs`): implements async-lsp's `LanguageServer`. Handles `initialize` (position-encoding negotiation, capability merging, workspace folders) and all document notifications, then forwards requests to the `Server` trait.

### The UTF-8 invariant (central design)

`Server` trait methods always receive and produce **UTF-8** positions, no matter which encoding was negotiated with the client (preference order UTF-8 > UTF-32 > UTF-16, `POSITION_ENCODING_PREFERRED_ORDER` in `src/server/with_state/mod.rs`). Translation lives in `src/lsp_requests/`: each LSP request is a marker struct under `#[lsp_request(...)]` in its own file there, with `extract_url` / `modify_params` (client encoding → UTF-8, before the handler) and `modify_response` (UTF-8 → client encoding, after); the shared `convert_*` / `modify_outgoing_*` helpers live in `src/lsp_requests/conversion.rs`. Positions in responses are converted against the document the position refers to, falling back to the request's document when that URL isn't tracked.

The `lsp_dispatch!` table glues each async-lsp method to a `Server` method through the request's hooks, plus staleness detection: it snapshots the document version before the handler runs and returns `CONTENT_MODIFIED` if the version changed by response time, so clients retry.

**Adding a new LSP method touches three places**: the `#[lsp_request(...)]` struct in a dedicated file under `src/lsp_requests/`, the `lsp_method!`/`lsp_resolve_method!` block for the trait method in `src/server/server_trait.rs`, and one row in the `lsp_dispatch!` table in `src/server/with_state/mod.rs`.

### State & documents

`ServerState` (`src/server/state/mod.rs`) is a cheaply-clonable interior-mutable handle: `DashMap` of documents, workspace roots, negotiated encoding, matchers. `Document` (`src/documents/document.rs`) is a cheap-`Clone` snapshot handle over an `Arc<DocumentInner>` (construction-immutable identity shared across copy-on-write generations — clones keep their snapshot), wrapping a `ropey::Rope` plus an optional tree-sitter `Language`/`Tree` under the feature.

- `didChange` applies incremental edits to the Rope and, with tree-sitter, `tree.edit()` + incremental reparse. If incremental application fails, it falls back to reloading the whole file from disk — notification handlers must stay synchronous per the LSP spec and async-lsp, hence the `std::fs` reads (noted in comments there).
- Documents carry an origin: `Open` (from the editor) or `Workspace` (loaded from disk). Open documents win over disk state; closing an open document keeps a disk snapshot only when workspace diagnostics are enabled for it.

### Matching & workspace scanning

`DocumentMatcher` (`src/documents/matcher.rs`) associates documents with a named matcher via URL globs and/or language-id strings, optionally carrying a tree-sitter grammar (language-per-document architecture). `WorkspaceWalker` (`src/workspace/walker.rs`) scans roots with the `ignore` crate in parallel (`build_parallel`, entries sorted after collection so output stays deterministic) — respects `.gitignore` by default, skips hidden files. Custom ignore file names (`ServerOptions::with_ignore_filenames`) and a global ignore file (`with_global_ignore_file`) feed the walker's exclusions, and events on the configured names or `.gitignore` invalidate the walk cache.

### Workspace diagnostics

`src/workspace/diagnostics.rs` implements the `workspace/diagnostic` request: walks roots, loads matching files as `Workspace` documents, runs per-document diagnostics through the same `Server` method, merges related-document reports. Per-document work goes through a width-bounded batch engine (`src/workspace/parallel.rs`'s `for_each_bounded`) defaulting to the CPU core count and narrowable via `ServerOptions::with_diagnostics_parallelism`. Exposure is set via `ServerOptions::with_workspace_diagnostics` — `Disabled` / `Enabled` / `Configurable(setting)`, where the setting is read from client configuration (`initializationOptions`, `workspace/configuration` requests, `didChangeConfiguration`, dynamic registration — each gated on client capabilities).

### `oneshot` module

`oneshot::workspace_diagnostics()` runs a `Server` over files on disk with no LSP client or transport — it drives `LanguageServerWithState` directly with a closed `ClientSocket`. CLI-style batch diagnostics.

### `text_utils`

`Encoding`, `position_to_encoding`, `Position`, and `RangeExt` (split/expand/shrink over byte, LSP, and tree-sitter ranges) — the machinery behind the transparent encoding conversion.

## Conventions

- All written documents and artifacts (specs, plans, code and doc comments, commit messages) are in English only.
- Public docs use `///` doc comments with `# Errors` sections on fallible functions, `# Panics` where a panic path exists, and `# Examples` doctests on doctest-friendly API; `missing_docs` is enabled in `[workspace.lints.rust]`
- Tests are inline per module — `#[cfg(test)] mod tests` blocks, or sibling `tests.rs` files for the larger modules; test conventions (rstest, fixtures, temp workspaces) are owned by `.claude/rules/testing.md`.
- Rust edition 2024 — let-chains (`if let ... && ...`) are used freely.

