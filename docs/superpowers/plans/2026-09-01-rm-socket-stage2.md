# rm-socket stage 2 (the removal) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every socket touchpoint the census mapped is gone; `serve(server)` is the stdio-only public entry over the kept `pub(crate) run_over_streams` seam; the W2 duplex tier becomes the single wire tier, split per concern; docs/dupes match; the four census axes re-sweep clean.

**Architecture:** Removal first (transport.rs, TcpConnect, tokio `net`, deprecation marks, `tests/lsp_wire.rs`), then the pure-motion split of `src/server/tests.rs` into `testing.rs` + `tests/`, then config/docs, then the four-axis verification sweep. Every change traces to a census row.

**Tech Stack:** tokio (`io-std`, `rt`; `io-util` placement decided empirically), async-lsp via the futures-trait seam, cargo-dupes for the config gate.

**Spec:** `docs/superpowers/specs/2026-09-01-rm-socket-stage2-design.md`
**Census:** `docs/superpowers/research/2026-09-01-socket-usage-census.md`

## Global Constraints

- Owner's rule, verbatim scope: **nothing socket-related survives — business logic, tests, examples, dependencies, configs, prose.**
- Public surface NET shrinks: `Transport`, `LspTransportRead`, `LspTransportWrite`, `ServerError::TcpConnect` out; nothing new public in. `serve`'s new signature is `pub async fn serve<S>(server: S) -> ServerResult<()>` where `S: Server + Clone + Send + Sync + 'static`.
- The tests split is PURE MOTION: assertion values, test names, probe orderings, the async-lsp#30 tripwire's two 250 ms bounded absence-checks and `server.abort()` cleanup migrate verbatim; only module paths and imports change.
- Full battery in all three feature configurations green at every task boundary; `cargo dupes check` exit 0 by Task 6; the deprecation `-D warnings` transient ENDS in Task 1.
- **Git is read-only for agents** (hook): tasks end with a suggested commit message; the owner commits. Never `git add`/`git commit`.
- **Navigation is LSP-first**; grep only for literal text. English artifacts. No lint suppression (no `#[allow]` anywhere, including `#[allow(deprecated)]`).
- Census anchors were verified at stage-1 time; if drifted, re-anchor by symbol/test name.

## Motion rules for Tasks 2–4 (repeated here because every task needs them)

R1. `use crate::server::{Server, serve::run_over_streams};` becomes `use crate::server::{Server, testing::*};` — actually per-file explicit lists: each concern file imports from `crate::server::testing` exactly what it uses (harness items), plus `crate::server::{Server, ServerState, ServerResult}`-shaped items its server impls need; NEVER a glob.
R2. The four test servers move as follows: `EchoServer` + `echo_hover` → `testing.rs` (shared); `GatedServer` → `robustness.rs` as `pub(crate)` (shared with `staleness.rs` via `use super::robustness::GatedServer;`); `PanickingServer` + `no_hover` → `robustness.rs`; `ConfigurableServer` → `termination.rs`. Bodies verbatim.
R3. Shared helpers (`WIRE_TIMEOUT`, `bounded`, `spawn_wire_server`, `RawClient` + its impl, `is_response_to`, `trim_crlf`, `did_open`, `hover_params`, the duplex adapters `FuturesReadHalf`/`FuturesWriteHalf`) move to `testing.rs` verbatim, now `pub(crate)` where cross-file.
R4. Test bodies: zero textual change beyond (a) the `spawn_wire_server` name stays, (b) imports, (c) `GatedServer`/`PanickingServer`/`ConfigurableServer` references resolving to their new homes.
R5. `tests/mod.rs` contains ONLY `mod` declarations — no tests, no helpers.

---

### Task 1: Production removal — transport.rs, TcpConnect, serve(server), marks, manifest

**Files:**
- Delete: `src/transport.rs`
- Modify: `src/server/serve.rs`, `src/server/mod.rs`, `src/error.rs`, `examples/minimal.rs`, `examples/tree_sitter.rs`, `Cargo.toml`
- Delete: `tests/lsp_wire.rs`

**Interfaces:**
- Produces (Tasks 2–5 consume): `pub async fn serve<S>(server: S) -> ServerResult<()>` (stdio-only); `pub(crate) async fn run_over_streams<S, R, W>` unchanged; `src/server/tests.rs` still present and passing until Task 2 touches it.

- [ ] **Step 1: Reshape `src/server/serve.rs`.** New content (the adapters are the Stdio arms of the deleted `LspTransportRead`/`LspTransportWrite`, reshaped as private structs):

```rust
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use async_lsp::{
    client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    panic::CatchUnwindLayer, router::Router, server::LifecycleLayer,
};
use futures::{AsyncRead, AsyncWrite};
use tokio::io::{AsyncRead as _, AsyncWrite as _, ReadBuf, Stdin, Stdout};
use tower::ServiceBuilder;

#[cfg(feature = "tracing")]
use async_lsp::tracing::TracingLayer;

use crate::{
    error::ServerResult,
    server::{LanguageServerWithState, Server},
};

const MAX_CONCURRENT_REQUESTS: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};

/// Serves a language server over the process standard input and output.
///
/// The server must be clonable, and shareable across threads.
///
/// This will automatically attach middleware for:
///
/// - Tracing metadata for each request
/// - Maximum concurrency of 8 in-flight LSP requests at a time
/// - Catching panics and safely returning internal server error statuses
/// - Client process monitoring and automatic server shutdown when client exits
///
/// # Examples
///
/// A stdio server cannot run inside a doctest, so this example only compiles:
///
/// ```no_run
/// use async_language_server::server::serve;
/// # #[derive(Clone)]
/// # struct MyServer;
/// # impl async_language_server::server::Server for MyServer {}
/// # #[tokio::main]
/// # async fn main() -> async_language_server::server::ServerResult<()> {
/// serve(MyServer).await
/// # }
/// ```
///
/// # Errors
///
/// If the server encounters an I/O error while running.
pub async fn serve<S>(server: S) -> ServerResult<()>
where
    S: Server + Clone,
    S: Send + Sync + 'static,
{
    run_over_streams(server, StdinAdapter(tokio::io::stdin()), StdoutAdapter(tokio::io::stdout())).await
}

/// Runs the real middleware stack (lifecycle, tracing, concurrency,
/// panic catching, client-process monitor) over arbitrary futures-trait
/// byte streams.
///
/// `serve()` runs it over the process stdio; the wire-tier tests
/// (`src/server/tests/`) drive the same stack over in-memory duplex
/// pipes, so the tested stack can never drift from the shipped one.
pub(crate) async fn run_over_streams<S, R, W>(server: S, reader: R, writer: W) -> ServerResult<()>
where
    S: Server + Clone + Send + Sync + 'static,
    R: AsyncRead,
    W: AsyncWrite,
{
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let builder = ServiceBuilder::new().layer(LifecycleLayer::default());

        #[cfg(feature = "tracing")]
        let builder = builder.layer(TracingLayer::default());

        builder
            .layer(ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS))
            .layer(CatchUnwindLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(Router::from_language_server(LanguageServerWithState::new(
                client,
                server.clone(),
            )))
    });

    server
        .run_buffered(reader, writer)
        .await
        .map_err(Into::into)
}

/// Bridges tokio's stdin to the futures `AsyncRead` the loop speaks.
struct StdinAdapter(Stdin);

impl AsyncRead for StdinAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut self.get_mut().0).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        }
    }
}

/// Bridges tokio's stdout to the futures `AsyncWrite` the loop speaks.
struct StdoutAdapter(Stdout);

impl AsyncWrite for StdoutAdapter {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}
```

(Adjust trivia to the file's real current state — e.g. keep the existing `MAX_CONCURRENT_REQUESTS` block verbatim if it differs — but the shape above is the target: no `Transport`, no `#[deprecated]`, stdio adapters private, `run_over_streams` doc rewritten per the spec.)

- [ ] **Step 2: `src/server/mod.rs`** — drop `pub use crate::transport::Transport;` (and the `LspTransportRead/Write` re-exports if present in the list); the `#[cfg(test)] mod tests;` line STAYS.

- [ ] **Step 3: `src/error.rs`** — delete the `TcpConnect` variant (with its `#[deprecated]`), the two `tcp_connect_*` unit tests, and the doctest that constructs it; replace the enum-level `# Examples` doctest with an `InvalidFilePath` resident:

```rust
/// ```
/// use async_language_server::server::ServerError;
///
/// let error = ServerError::InvalidFilePath {
///     path: std::path::PathBuf::from("/nonexistent"),
/// };
/// assert_eq!(error.to_string(), "invalid file path '/nonexistent'");
/// ```
```

Remove the now-unused `TcpConnect` doc line from the variant list; keep `PathBuf` import.

- [ ] **Step 4: Delete `src/transport.rs`** and its `mod transport;` in `src/lib.rs`.

- [ ] **Step 5: Examples** — `examples/minimal.rs` and `examples/tree_sitter.rs`: `serve(Transport::Stdio, server)` → `serve(server)`; drop the `Transport` imports.

- [ ] **Step 6: Manifest** — `Cargo.toml` main tokio features: `["io-std", "io-util", "net", "rt"]` → remove `net`. Then the empirical `io-util` branch: run `cargo build` (NO `--all-targets`); if green, ALSO remove `io-util` from main and ADD it to the dev tokio feature list; if red, keep `io-util` in main and add the one-line reason comment (`# io-util: the futures-trait adapters' ReadBuf bridging needs it in production`). Report which branch held.

- [ ] **Step 7: Delete `tests/lsp_wire.rs`.**

- [ ] **Step 8: Verify** — the transient ENDS: `cargo clippy --all-targets -- -D warnings` green; full battery: `cargo test` (lib 143 + arch 1 + doctests 13 — lsp_wire gone), `cargo test --no-default-features` (111-ish lib + 1 + 13), `cargo test --all-features`; `cargo fmt --check`; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`; `cargo test --lib server::tests` → 14 passed (W2 untouched, `spawn_wire_server`/adapters still resolve — they live in `src/server/tests.rs`, which still has its own duplex adapters and does NOT import the new Stdio adapters).

- [ ] **Step 9: Report for commit**

Suggested: `feat!: remove sockets — serve(server) is stdio-only over the internal stream seam (breaking: Transport/TcpConnect removed)`

---

### Task 2: Harness extraction — src/server/testing.rs + tests/ skeleton + lifecycle

**Files:**
- Create: `src/server/testing.rs`, `src/server/tests/mod.rs`, `src/server/tests/lifecycle.rs`
- Modify: `src/server/mod.rs` (add `#[cfg(test)] pub(crate) mod testing;` — placement next to the existing declarations), `src/server/tests.rs` → DELETED (all remaining content moves in this and the following tasks — see R-rules)

**Interfaces:**
- Produces (Tasks 3–5 consume): `crate::server::testing::{WIRE_TIMEOUT, bounded, spawn_wire_server, RawClient, is_response_to, trim_crlf, echo_hover, did_open, hover_params, EchoServer, FuturesReadHalf, FuturesWriteHalf}` — `pub(crate)`, bodies verbatim from today's `src/server/tests.rs`.

- [ ] **Step 1: Create `src/server/testing.rs`** — move verbatim from `src/server/tests.rs`: the module doc (rewritten to name `testing` as the harness home), `WIRE_TIMEOUT`, `bounded`, `FuturesReadHalf`/`FuturesWriteHalf` (+ their imports), `RawClient` with its full impl, `is_response_to`, `trim_crlf`, `EchoServer` + `echo_hover`, `spawn_wire_server`, `did_open`, `hover_params`. Items consumed by other files become `pub(crate)`; private helpers (`is_response_to`, `trim_crlf` if only used inside `RawClient`) stay private.

- [ ] **Step 2: Create `src/server/tests/mod.rs`**:

```rust
//! The wire tier: black-box tests driving the real middleware stack over
//! in-memory duplex pipes through the internal `run_over_streams` seam.

mod conversion;
mod dispatch;
mod lifecycle;
mod robustness;
mod staleness;
mod termination;
```

(The mod lines for files landing in Tasks 3–5 are ADDED to this file by those tasks; create now only the ones whose files exist this task — i.e. start with `mod lifecycle;` alone and let each later task append its line. The listing above is the final state.)

- [ ] **Step 3: Create `src/server/tests/lifecycle.rs`** — move VERBATIM (R1–R5): `initialize_negotiates_position_encoding_end_to_end`, `requests_before_initialize_are_rejected`, `double_initialize_is_rejected`, `requests_after_shutdown_are_rejected`. Import list explicit per R1.

- [ ] **Step 4: Delete `src/server/tests.rs`** — its remaining tests are NOT lost: they move in Tasks 3–5. This step therefore lands LAST in the task, after Steps 1–3 compile. To keep the tree compiling at every intermediate moment, do the moves in the order: create testing.rs + tests/{mod,lifecycle}.rs with `#[cfg(test)] mod tests;` repointed — concretely: replace the `#[cfg(test)] mod tests;` declaration in `src/server/mod.rs` with `#[cfg(test)] mod tests;` pointing at the NEW directory (the declaration text is identical — the file system decides), and only then `git`-delete the old file… since agents cannot run git: simply `rm` the old `src/server/tests.rs` after the new files exist and the remaining four concern files from Tasks 3–5 do not exist yet — which would break the build because `tests/mod.rs` references them. THEREFORE the practical order within this task: extract testing.rs, create ALL SIX concern files in this task with their full content moved verbatim from `src/server/tests.rs` (lifecycle.rs per Step 3; conversion/dispatch/staleness/robustness/termination as specified in Tasks 3–5 of this plan — the task implementer performs those file moves as part of reaching a compiling tree), create tests/mod.rs with all six mod lines, THEN delete src/server/tests.rs. The plan keeps the per-file specifications in Tasks 3–5 as the REVIEW CONTRACT for what each file must contain; the physical move happens here to keep every commit boundary compiling.

- [ ] **Step 5: Verify** — `cargo test --lib server::tests` → 14 passed (all six files' tests, now under new paths); full battery ×3 green; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --test architecture` (arch-lint sees the new files — requests→server unchanged; if a scope complaint fires on `testing.rs`, place it per the error and report — do not suppress).

- [ ] **Step 6: Report for commit**

Suggested: `refactor: split the wire monolith — testing.rs harness + per-concern test files (pure motion)`

---

### Task 3: review contract — conversion.rs + dispatch.rs

**Files (already physically created in Task 2; this task is the reviewer's checklist and the implementer's mapping):**
- `src/server/tests/conversion.rs` must contain, verbatim per R1–R5: `utf16_positions_round_trip_through_real_serialization`, `incremental_did_change_applies_over_the_wire` (+ its `did_open` import from testing).
- `src/server/tests/dispatch.rs` must contain: `unwired_methods_return_method_not_found` with the `-32602`-before-`-32601` comment block intact.
- `tests/mod.rs` gains `mod conversion;` and `mod dispatch;` (already present if Task 2 created all six).

**Interfaces:** consumes `crate::server::testing` per R1.

- [ ] **Step 1: Verify** — `cargo test --lib server::tests::conversion` + `::dispatch` → 3 passed total; grep the two files for accidental body drift against `git show HEAD~1:src/server/tests.rs` if needed.

- [ ] **Step 2: Report for commit** (folded into Task 2's commit if executed together)

---

### Task 4: review contract — staleness.rs + robustness.rs

**Files (physically created in Task 2):**
- `src/server/tests/staleness.rs`: `stale_document_answers_content_modified_then_succeeds_on_retry` verbatim, probe ordering intact; imports `GatedServer` from `super::robustness`.
- `src/server/tests/robustness.rs`: `GatedServer` (pub(crate)) + `PanickingServer` + `no_hover` definitions; `panicking_handler_returns_structured_error`; `at_most_eight_requests_run_concurrently` — the async-lsp#30 tripwire block (comment with PR link + restoration instructions, two 250 ms bounded absence-checks, `server.abort()` cleanup) VERBATIM; `cancel_request_answers_request_cancelled`; `malformed_header_closes_the_connection`.

**Interfaces:** `pub(crate) struct GatedServer` in robustness.rs consumed by staleness.rs via `use super::robustness::GatedServer;`.

- [ ] **Step 1: Verify** — `cargo test --lib server::tests::staleness` + `::robustness` → 5 passed; tripwire stable (~0.5 s).

- [ ] **Step 2: Report for commit** (folded into Task 2's if executed together)

---

### Task 5: review contract — termination.rs

**Files (physically created in Task 2):**
- `src/server/tests/termination.rs`: `ConfigurableServer` definition (capabilities override verbatim); `shutdown_exit_terminates_the_server_loop_cleanly` (serve-handle join asserted, EOF drain, no trailing bytes); `workspace_configuration_request_is_served_mid_request` (temp workspace via `crate::testing::temp_workspace`, tokio::fs calls, pending-stash interplay intact).

- [ ] **Step 1: Verify** — `cargo test --lib server::tests::termination` → 2 passed.

- [ ] **Step 2: Report for commit** (folded into Task 2's if executed together)

---

### Task 6: Dupes config, docs, and the four-axis verification sweep

**Files:**
- Modify: `.dupes-ignore.toml`, `.claude/rules/testing.md`, `.claude/rules/structure.md`, `CLAUDE.md`, `README.md` (as census rows dictate)

**Interfaces:** none.

- [ ] **Step 1: Dupes** — `cargo dupes cleanup`; then `cargo dupes check; echo exit=$?` → 0. Delete the dead `.dupes-ignore.toml` entry naming `LspTransportWrite` forwarding (census row `.dupes-ignore.toml:71`). If the split surfaced NEW groups (e.g. two concern files normalizing together), add reasoned entries — never loosen thresholds. Report final stats.

- [ ] **Step 2: Steering doc** — `.claude/rules/testing.md`: tier table becomes two rows (W0 unit inline; wire = `src/server/tests/` over duplex through the internal seam); harness inventory maps `src/server/testing.rs`; W3 row and the W2/W3 deferred-duplication note die; tripwire paragraph path → `src/server/tests/robustness.rs`; `-32602`/fixpoint notes keep content, updated paths.

- [ ] **Step 3: structure.md + CLAUDE.md + README** — per census update-text rows: `serve(Transport::Stdio, …)` phrasings become `serve(…)`; the Transport mentions in the serve/transport descriptions go; `error-handling.md` verify (expected: no TcpConnect mention — the spec's empirical check).

- [ ] **Step 4: Four-axis sweep (the cycle's final proof)** —
  (a) LSP findReferences on `Transport`/`LspTransportRead`/`LspTransportWrite`/`ServerError::TcpConnect` → zero live references; (b) `grep -rn "tokio::net\|TcpStream\|TcpListener\|SocketAddr" src/ examples/ tests/` → empty; `grep -n "socket2" Cargo.lock` → absent (if present after the manifest change: `cargo update -p tokio`, never a full `cargo update`); (c) docs-axis grep → zero socket/tcp rows outside `docs/superpowers/` history; (d) `grep -rn "deprecated.*sockets" src/` → empty (all marks gone). Record all four outputs in the report.

- [ ] **Step 5: Full battery** — all six commands green in all three configs; final counts reported (lib 143 + arch 1 + 13 doctests; no lsp_wire target).

- [ ] **Step 6: Report for commit**

Suggested: `chore: dupes config and docs match the stdio-only reality; census axes sweep clean`

---

## Self-Review (done at plan time)

- **Spec coverage**: production § (Task 1: transport.rs delete, serve(server), adapters move, marks off, TcpConnect + doctest resident, manifest net/ + empirical io-util); tests § (Task 1 kills lsp_wire; Tasks 2–5 the pure-motion split with the exact 14-test mapping 4+3+5+2); dupes § (Task 6 Step 1); docs § (Steps 2–3); verification § (Step 4's four axes + Step 5 battery). Out-of-scope respected.
- **Placeholders**: none — Task 1's serve.rs is complete target code; Tasks 2–5 carry the motion rules + per-file membership contracts (the proven T1/T4 pattern — bodies specified by verbatim-motion rule, not re-pasted); Task 6's doc edits name exact rows/entries.
- **Type consistency**: `serve<S>(server: S)` / `run_over_streams<S, R, W>` signatures consistent across Task 1 and the testing.rs doc; `spawn_wire_server` name unchanged; `GatedServer` pub(crate) in robustness consumed by staleness — one definition, one import path.
