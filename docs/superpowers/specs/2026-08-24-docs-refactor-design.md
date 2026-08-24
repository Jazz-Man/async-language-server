# Documentation Refactor Design: async-language-server

Date: 2026-08-24
Branch: `feature/doc`
Strategy: A (staged, 4 phases)

## Context and Problem

The audit (LSP `documentSymbol` across all public items, a full read of `src/`, and the `doc-*` rules from rust-skills) found:

- **Style**: ~95% of docs use `/** ... */` block comments with 4-space indented bodies (`src/server_trait.rs:26`, `src/document.rs:20`, `src/serve.rs:19`, etc.). Valid rustdoc, but not idiomatic; the standard is `///` (rust-skills `doc-all-public`). Styles are already mixed: `src/document.rs:291` (`DocumentQueryCapture`) and `src/server_options.rs:30` (`WorkspaceDiagnostics`) use `///`, everything else uses `/** */`.
- **Module docs**: no `//!` anywhere — neither in the crate root nor in the public modules `server`, `text_utils`, `oneshot`, `tree_sitter_utils` (violates `doc-module-inner`).
- **Coverage gaps**: public fields `Position::line/col` (`src/text_utils/position.rs:11`), `DocumentDiagnostics::{uri,version,report}` (`src/oneshot/workspace_diagnostics.rs:98`); enum variants of `Transport` (`src/transport.rs:28`), `ServerError` (`src/result.rs:13`), `LspTransportRead/Write`; `Server` trait methods have no individual docs (only group `//` comments, `src/server_trait.rs:53`); converters `Position::from_lsp/into_ts...`, `Encoding::as_str/from_lsp...`.
- **Sections**: `# Errors` exists on only 3 functions; `Document::node_text` describes its panic in prose instead of a `# Panics` section (`src/document.rs:163`); `RangeExt` impls panic via `assert!` without `# Panics`; `#![allow(clippy::missing_panics_doc)]` in `serve.rs:1` and `transport.rs:1` suppresses the lint instead of satisfying it.
- **Examples**: no `# Examples` anywhere; doctests disabled (`doctest = false`, `Cargo.toml:12`).
- **Metadata**: Cargo.toml lacks `description`, `repository`, `rust-version`, `readme`; the crate root does not include the README.
- **Links**: mixed intra-doc link styles — full path `[`Document::text_reader`]` vs bare `[`node_at_position`]` (`src/document.rs:189`).

## User Decisions

1. Scope: **full refactor + examples** (doctests, README as crate docs, Cargo metadata).
2. Code examples live in `examples/`; **do not extend the README** (only the `include_str!` hookup).
3. Enforcement: **full** — clippy `-D warnings`, `fmt --check`, `cargo doc` with `-D warnings` in CI.
4. Both examples: `minimal.rs` and `tree_sitter.rs` (with a grammar dev-dependency).
5. Standing process rule: rust-skills as acceptance criteria + LSP as the verification tool (saved to memory).

## Design

### 1. Style and Structure

- All doc comments become `///`, module docs become `//!`. The block style disappears completely.
- The first sentence of every doc block is a short summary (~15 words) that reads on its own in the rustdoc module index (`doc-first-sentence`). Then a blank line, then details in prose.
- Canonical section order (`doc-canonical-sections`): summary → details → `# Examples` → `# Errors` → `# Panics` → `# Safety`; only the applicable ones.
- Intra-doc links use full paths only: [`Document::text_reader`], [`ServerState::document`].
- `#[cfg]` attributes are placed after the doc comment (currently reversed at `src/document.rs:284`, `src/document_matcher.rs:31`).
- Module `//!` docs for: `server` (architecture overview + links to key types), `text_utils`, `oneshot`, `tree_sitter_utils`. Crate root gets `#![doc = include_str!("../README.md")]` (safe: the README contains no Rust blocks that would run as doctests).
- Docs stay in English.

### 2. Coverage and Contracts

- Close every coverage gap from the audit (fields, variants, converters — full list above).
- Every `Server` trait method gets an individual doc: when the LSP client invokes it, what to return, and the requirement to register the matching capability in `server_capabilities`.
- `# Panics` wherever `assert!` or panics exist: `Document::node_text`, all `RangeExt` methods. Remove `#![allow(clippy::missing_panics_doc)]` from `serve.rs` (no panics); in `transport.rs`, document the dead `unreachable!()` branch (`src/transport.rs:62`) with a `# Panics` section instead of the allow.
- `# Errors` on every public function returning `Result`. For `Option`-returning functions (`Document::query`), describe the `None` conditions in prose.

### 3. Doctests and `examples/`

- Remove `doctest = false` from `[lib]`.
- Live `# Examples` on doctest-friendly API:
  - `text_utils`: `Position`, `Encoding`, `position_to_encoding`, `RangeExt` — deterministic and stateless.
  - `server::DocumentMatcher` (builder chain), `ServerOptions::with_workspace_diagnostics`, `Transport` (Display), `ServerError`.
  - `oneshot::workspace_diagnostics` — a full run with a minimal test `Server` inside the doctest; setup hidden with `# `, errors handled with `?` (`doc-hidden-setup`, `doc-question-mark`).
  - `serve()` — a ```` ```no_run ```` example (a stdio server cannot run inside a doctest).
- `examples/minimal.rs`: `impl Server` with `document_diagnostics` (diagnostics for over-long lines), `Transport::Stdio`, `serve()`.
- `examples/tree_sitter.rs`: a server using `DocumentMatcher::with_lang_grammar` and the `tree-sitter-json` grammar (new dev-dependency).

### 4. Cargo.toml + CI

- Cargo.toml: add `description`, `repository`, `readme = "README.md"`, `rust-version` (determined empirically; the code uses let-chains, so at least 1.88; the exact value is verified by building during implementation). Remove the `cargo_common_metadata` allow.
- `[lints.rust] missing_docs = "warn"`.
- CI (`rust.yml`): `cargo build --all-targets` → `cargo test` in three configurations (default / `--no-default-features` / `--all-features`) → `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo doc --no-deps` with `RUSTDOCFLAGS="-D warnings"`.

### 5. Verification (rust-skills + LSP built into the process)

- rust-skills as acceptance criteria: phase 3 is accepted against all 15 `doc-*` rules; the CI phase against `lint-rustfmt-check`, `lint-static-verification`, `lint-missing-docs`; the final review against `doc-question-mark`/`doc-hidden-setup` for every example.
- LSP as the control tool: after phase 2 — `documentSymbol` over all public modules, cross-checking the symbol inventory against the `missing_docs` report (two independent checks of the same list); `hover` on key public items; `findReferences` to confirm intra-doc links point at live symbols.
- After every phase: `cargo build`, `cargo test` (default + `--no-default-features`), `cargo doc --no-deps` with `-D warnings`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.

## Phases (Strategy A)

| Phase | Content | Commit |
|---|---|---|
| 1 | Mechanical conversion `/** */` → `///` with no text changes; `#[cfg]` moved after docs | pure formatting, reviewed with `git diff -w` |
| 2 | Enable `missing_docs`; `//!` module docs; crate root + README; close all coverage gaps | lint-driven work list |
| 3 | Content quality: first sentences, `# Errors`/`# Panics`, link unification, removal of `#![allow]` | acceptance against `doc-*` |
| 4 | doctests, `# Examples`, `examples/` (2 files + dev-dep), Cargo metadata, CI | full CI matrix |

## Out of Scope

- Behavior changes (only doc comments, Cargo.toml, CI; exceptions — removal of doc-related `#![allow]`, the dev-dependency for the example).
- Extending the README.
- Internal implementation `//` comments.
- Restructuring modules or the public API.

## Risks

- Enabling doctests increases `cargo test` time; moderate, examples are short.
- The `tree-sitter-json` dev-dependency adds CI build time; acceptable, the example demonstrates the crate's main feature.
- `rust-version` may turn out higher than 1.88 due to other dependencies; determined by an actual build check.
