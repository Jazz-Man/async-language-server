# Upstream stdio transport — Design

**Cycle opened:** 2026-09-06 · **Design finalized:** 2026-09-06 · **Branch:** `feature/macros`
**Inputs:** session research against async-lsp 0.2.4 sources (`src/stdio.rs`, `src/lib.rs`,
`examples/{server_trait,server_builder,inspector}.rs`, `tests/{stdio,unit_test}.rs`,
`Cargo.toml`) and this repo (`src/server/serve.rs`, `src/server/testing.rs`,
`src/server/tests/*`, `src/error.rs`, `Cargo.toml`, CI matrix, docs).
**Status:** presented section-by-section and approved in brainstorm (owner, 2026-09-06).

## Goal

Replace the hand-rolled stdio transport with async-lsp 0.2.4's pipe transport:
`serve()` locks `PipeStdin`/`PipeStdout` and hands them to `run_over_streams`
unchanged. The tokio↔futures adapters (`TokioReader`/`TokioWriter`) are deleted from
the repository entirely — production gets the futures traits directly from upstream's
pipe types, the wire-test harness bridges duplex pipes through `tokio-util`'s compat.
Zero public API change: `serve`'s signature is identical, the adapters were
`pub(crate)`, and the examples compile untouched.

The gain beyond deduplication: `tokio::io::stdin()` delegates reads to blocking
threads (stated in upstream's stdio module docs); `PipeStdin` is true asynchronous
I/O on the process fd.

## Owner decisions log

1. **Unix-only.** Owner platforms are macOS plus occasional Linux; other platforms
   may be added later if ever genuinely needed (owner's emphasis: "possibly").
   No `#[cfg(not(unix))]` fallback
   and no tokio-util compat fallback in production — that path would re-add the
   adapters and a dependency for a platform nobody builds. The crate therefore
   compiles on unix only from this cycle on (CI already runs ubuntu-latest only).
2. **`run_over_streams` stays** — `pub(crate)`, signature unchanged. It is not a
   transport: it is the server constructor (the tower middleware stack around
   `MainLoop::new_server` + `Router::from_language_server(LanguageServerWithState)`),
   and the seam the wire-tier suite drives over `tokio::io::duplex`. Upstream has
   no equivalent (both upstream server examples hand-write their stacks).
3. **tokio-util (option A).** The test harness bridges tokio duplex → futures
   traits via `tokio-util`'s `compat` — the same crate async-lsp's own
   `tests/unit_test.rs` uses for the identical job — instead of keeping our
   adapters as `cfg(test)` code (option B, rejected).
4. **`examples/inspector.rs` noted, not adopted.** It is a man-in-the-middle
   tracing proxy for debugging a spawned server binary. Rejected as a test
   building block: our raw JSON-RPC client is deliberately async-lsp-free
   (isolated from async-lsp client-path bugs; an editor is a raw byte stream,
   not an async-lsp loop), it requires a child-process binary (we are a library
   crate), its `Inspect` middleware asserts nothing our `TracingLayer` does not
   already log, and the `forward` feature would be enabled for nothing. It
   remains usable as-is from an async-lsp checkout for manual traffic
   inspection of downstream servers.

## Verified findings (research evidence)

- `async_lsp::stdio` (`#[cfg(all(feature = "stdio", unix))]`): `PipeStdin::lock_tokio()`
  and `PipeStdout::lock_tokio()` lock process stdin/stdout, require a pipe-like fd
  (FIFO, socket, or character device — a regular file fails), set `O_NONBLOCK`,
  register `AsyncFd`, and return types implementing `futures::AsyncRead` /
  `futures::AsyncWrite` directly. They feed `run_buffered` with zero adapters.
- Feature gates: `stdio = ["dep:rustix", "rustix?/fs", "tokio?/net"]`; the
  `lock_tokio` impls additionally live under async-lsp's implicit `tokio`
  optional-dependency feature. Our workspace dependency currently enables only
  `["client-monitor", "omni-trait", "tracing"]` with `default-features = false`.
- Upstream test coverage: `tests/stdio.rs` exercises the pipe transport
  thoroughly (parent/child over real pipes — lock semantics, non-blocking
  reads, drop releases the locks); inline lib tests cover closed sockets and
  `AnyEvent`. Nothing upstream covers our layer: capability negotiation,
  dispatch rows, encoding conversion, staleness, stack composition. Our 19
  wire tests stay; deleting them because "upstream is tested" would leave our
  own wiring untested.
- Adapter consumers (grep-verified): `serve()` and `spawn_wire_server` in
  `src/server/testing.rs` only.
- Upstream caveat to carry into docs: nothing may leave bytes in std's buffered
  stdin/stdout (`print!` and friends) — already warned by the `print_stdout`
  clippy lint.

## Design

### Architecture

`serve()` becomes a thin stdio wrapper: lock both pipe channels, run the server
constructor over them. `run_over_streams` keeps its generic
`AsyncRead`/`AsyncWrite` bounds — satisfied natively by `PipeStd*` in production
and by `tokio-util`-bridged duplex halves in tests. No trait-bridging code
remains anywhere in the repository.

### Components

1. `Cargo.toml`
   - `[workspace.dependencies]`: async-lsp features become
     `["client-monitor", "omni-trait", "stdio", "tokio", "tracing"]`;
     add `tokio-util = "0.7"`.
   - `[dev-dependencies]`: `tokio-util = { workspace = true, features = ["compat"] }`.
   - `Cargo.lock` grows `rustix/fs` usage and the unified tokio gains the `net`
     feature (via `tokio?/net`) — accepted, not fought.
2. `src/server/serve.rs`
   ```rust
   pub async fn serve<S>(server: S) -> ServerResult<()>
   where
       S: Server + Clone,
       S: Send + Sync + 'static,
   {
       let (stdin, stdout) = (
           async_lsp::stdio::PipeStdin::lock_tokio()?,
           async_lsp::stdio::PipeStdout::lock_tokio()?,
       );
       run_over_streams(server, stdin, stdout).await
   }
   ```
   - Delete `TokioReader`/`TokioWriter` and their now-unused imports
     (`std::pin::Pin`, `std::task::{Context, Poll}`, `tokio::io::ReadBuf`);
     `futures::{AsyncRead, AsyncWrite}` stay as `run_over_streams` bounds.
   - `serve`'s `# Errors` doc gains: the stdin/stdout fd may fail to lock as a
     pipe-like channel (for example when redirected to a regular file); unix-only;
     the std-buffer caveat.
3. `src/server/testing.rs` — `spawn_wire_server`, same shape as upstream's
   `tests/unit_test.rs:92-99`:
   ```rust
   use futures::AsyncReadExt as _;              // .split() on the compat stream
   use tokio_util::compat::TokioAsyncReadCompatExt as _;

   let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
   let (client_read, client_write) = split(client_stream);   // stays tokio for RawClient
   let (server_read, server_write) = server_stream.compat().split();
   let handle = tokio::spawn(run_over_streams(server, server_read, server_write));
   ```
   Drop the `serve::{TokioReader, TokioWriter, …}` import (keep `run_over_streams`);
   fix the module doc comment, which credits the bridging to the adapters in
   `serve.rs`.
4. `src/error.rs` — `ServerError::Io`'s doc sentence ("stdio-wire I/O failures
   arrive as the `Lsp` variant") is updated: stdio lock failures now also arrive
   as `Io`.
5. Docs sync (English): `README.md:4` and `CLAUDE.md:7` ("tokio stdio
   transport" → pipe-based transport wording; README is the rendered crate doc),
   `.claude/rules/product.md:5` (same phrase), `.claude/rules/structure.md`
   (`serve()` paragraph: upstream `PipeStdin`/`PipeStdout`, unix-only,
   `run_over_streams` described as server constructor + wire-test seam),
   `.claude/rules/testing.md` (harness inventory: bridging via `tokio-util`
   compat, dev-dependency).

### Data flow

Production: stdin fd (`O_NONBLOCK`, `AsyncFd`) → `PipeStdin` → `run_buffered`
framing → `MainLoop` → middleware stack → `LanguageServerWithState`; responses
back through `PipeStdout`. Tests: duplex pipes instead of the process fd, the
rest identical. `examples/minimal.rs` and `examples/tree_sitter.rs` change
nothing.

### Error handling

`lock_tokio` returns `std::io::Error`; `ServerError::Io` converts it through
the existing `#[from]`, preserving the source chain. No new error variants.
The new failure mode (non-pipe-like fd) and the unix-only constraint are
documented in `serve`'s `# Errors` section.

## Rejected alternatives

- **Delete the wire tier** ("upstream already tests it") — upstream tests
  upstream's machinery; our 19 wire tests cover our stack composition,
  dispatch table, encoding conversion, staleness, and termination. Nothing
  upstream can cover those.
- **Keep the adapters as `cfg(test)` code** (option B) — 40 lines of test-only
  bridging to maintain when upstream uses a tested crate for exactly this job.
- **tokio-util compat fallback in production for non-unix** — re-adds a
  dependency and a code path for a platform nobody builds (decision 1).
- **An inspector-based harness** (decision 4) — raw-client isolation is the
  wire tier's design principle.
- **A public `serve_over(server, read, write)` entry** — downstream deployment
  is stdio-only (product.md); the generic entry already exists as the
  `pub(crate)` seam tests need.

## Testing

Zero new tests, zero modified tests. Rationale: the transport is process-global
(pipe locks on the real stdin/stdout) and cannot be driven from a unit test;
upstream's `tests/stdio.rs` covers it; our layer is covered by the existing 19
wire tests, which ride the same `run_over_streams` the shipped `serve()` uses.
Type-level acceptance of `PipeStd*` by `run_over_streams` is compile-checked by
`serve`'s own body.

Success criteria — the full battery in all three feature configurations:

```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo test --workspace --no-default-features
cargo test --workspace --all-features
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Plus: `grep -rn "TokioReader\|TokioWriter" src/ macros/ examples/ tests/`
returns nothing, and `git diff src/server/tests/` is empty.

## Out of scope

- Non-unix platform support (may be added later by owner decision).
- Any public API surface change.
- Anything beyond the stdio transport: the middleware stack order, concurrency
  bound, and `run_over_streams` internals are untouched.
