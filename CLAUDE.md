# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Library crate (no binary) that wraps `async-lsp` to make language servers with less boilerplate: async-lsp pipe-based stdio transport, ropey-based incremental document sync, automatic position-encoding negotiation (UTF-8/16/32), and optional tree-sitter integration. Personal project, version 0.0.0, not published to crates.io — consumed as a git dependency or fork. Public API lives under `async_language_server::server::*`, with `lsp_types` re-exported at the crate root.

## Commands

- Local verification is `make battery` (CI parity); `make help` lists all targets
- CI (`.github/workflows/ci.yml`) runs on push/PR to `main` and via `workflow_dispatch` on any branch: a `checks` job (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`) plus a two-leg test matrix (`cargo test --workspace --all-features` and `--no-default-features`) in parallel, and a `dylint` job (`cargo dylint --all -- --all-targets`, stock Trail of Bits + `perfectionist` suites via pinned nightly) that the release job gates on, with Cargo caching (`rust-cache`, saves on `main` only); on pushes to `main`, a release job tags (`v` prefix; `#major`/`#minor`/`#patch`/`#none` commit-message tokens) and publishes a GitHub Release with changelog. The `default` test leg returns when a second, non-default feature exists (today `default` ≡ `--all-features`)
- `cargo nextest run <filter>` — run a single test; tests are inline `#[cfg(test)] mod tests` blocks inside each `src/` module, or sibling `tests.rs` files for the larger modules
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

`DocumentMatcher` (`src/documents/matcher.rs`) associates documents with a named matcher via URL globs and/or language-id strings, optionally carrying a tree-sitter grammar (language-per-document architecture). `WorkspaceWalker` (`src/workspace/walker.rs`) scans roots with the `ignore` crate in parallel (`build_parallel`, entries sorted after collection so output stays deterministic) — respects `.gitignore` by default, skips hidden files.

### Workspace diagnostics

`src/workspace/diagnostics.rs` implements the `workspace/diagnostic` request: walks roots, loads matching files as `Workspace` documents, runs per-document diagnostics through the same `Server` method, merges related-document reports. Per-document work goes through a width-bounded batch engine (`src/workspace/parallel.rs`'s `for_each_bounded`) defaulting to the CPU core count and narrowable via `ServerOptions::with_diagnostics_parallelism`. Exposure is set via `ServerOptions::with_workspace_diagnostics` — `Disabled` / `Enabled` / `Configurable(setting)`, where the setting is read from client configuration (`initializationOptions`, `workspace/configuration` requests, `didChangeConfiguration`, dynamic registration — each gated on client capabilities).

### `oneshot` module

`oneshot::workspace_diagnostics()` runs a `Server` over files on disk with no LSP client or transport — it drives `LanguageServerWithState` directly with a closed `ClientSocket`. CLI-style batch diagnostics.

### `text_utils`

`Encoding`, `position_to_encoding`, `Position`, and `RangeExt` (split/expand/shrink over byte, LSP, and tree-sitter ranges) — the machinery behind the transparent encoding conversion.

## Conventions

- All written documents and artifacts (specs, plans, code and doc comments, commit messages) are in English only.
- Public docs use `///` doc comments with `# Errors` sections on fallible functions, `# Panics` where a panic path exists, and `# Examples` doctests on doctest-friendly API; `missing_docs` is enabled in `[workspace.lints.rust]`
- Tests are inline per module — `#[cfg(test)] mod tests` blocks, or sibling `tests.rs` files for the larger modules — and create real temp workspaces on disk (millisecond-unique names under `std::env::temp_dir()`).
- Rust edition 2024 — let-chains (`if let ... && ...`) are used freely.

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
rtk uv run <cmd>        # Compact uv project command output
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->
