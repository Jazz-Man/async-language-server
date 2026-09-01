# Wire-tests consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One black-box wire tier — all 16 wire tests drive the public `serve()` over loopback TCP from a single `tests/wire/` integration target; `src/server/tests.rs` and the duplex adapters disappear; the W2/W3 harness duplication dissolves.

**Architecture:** A directory test target (`tests/wire/main.rs` + per-concern submodule files) with one shared harness (`tests/wire/harness.rs`: fallible bounded RawClient over `BufStream<TcpStream>`, `spawn_serve` helper, the four test servers). Tests migrate verbatim in semantics from `src/server/tests.rs` (duplex) and `tests/lsp_wire.rs` (TCP), adapting only transport wiring and fallible call sites. Demolition and documentation land last.

**Tech Stack:** tokio (existing features), serde_json, async_language_server public API only.

**Spec:** `docs/superpowers/specs/2026-09-01-wire-tests-split-design.md`

## Global Constraints

- Public surface unchanged: NO new `pub`, features, or manifest edits.
- Tests assert through the PUBLIC `serve()`/`Transport` API only; nothing may import `async_language_server` internals.
- Fallible-helper style: `expect()`/`unwrap()` ONLY inside `#[tokio::test]` fns — `expect_used = deny` covers helper methods in integration crates (`allow-expect-in-tests` does not).
- Determinism: channel gates, never sleeps; every cross-task await bounded by `WIRE_TIMEOUT` (5 s); the async-lsp#30 tripwire's two 250 ms bounded absence-checks are the sanctioned exception and migrate verbatim with `server.abort()` cleanup.
- Assertion values, probe orderings, `processId: null`, and test names carry over UNCHANGED from the sources; only transport wiring and `.expect(...)` wrapping change.
- `tests/wire.rs` (sibling file) must never exist (`self_named_module_files`).
- Full battery in three configs gates every task: `cargo test`, `cargo test --no-default-features`, `cargo test --all-features`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- **Git is read-only for agents** (hook): tasks end by reporting a suggested commit message; the owner commits. Never run `git add`/`git commit`.
- Navigation is LSP-first for symbols; grep only for literal text. English artifacts.
- Source anchors below are verified at plan time; if drifted, re-anchor by test name.

## Migration rules (apply in Tasks 2–5; repeated here because every task needs them)

R1. `let (mut client, server) = spawn_wire_server(X);` → `let (mut client, server) = spawn_serve(X).await;` (spawn_serve awaits `accept()`; it is async).
R2. Every former infallible client call gains `.expect("<message>")` at the call site, inside the test fn: `client.request(..).await` → `client.request(..).await.expect("hover responds")`, `client.initialize_client(&[..]).await` → `.. .await.expect("initialize round-trips")`, `client.notify(..).await` → `.. .await.expect("notification sends")`, `client.send_request(..).await` / `client.await_response(id).await` likewise.
R3. `drop(client); let _ = bounded(server).await;` → `drop(client); let _ = bounded(server).await.expect("serve task joins");` (the inner `ServerResult` stays deliberately ignored via `let _`; the tripwire keeps `server.abort();` with its comment, unchanged).
R4. Raw garbage writes (`client.writer.write_all(...)` / `client.stream.write_all(...)`) → `client.write_raw(bytes).await.expect("garbage writes")`.
R5. EOF drain (`client.read_to_end(&mut raw)` with the surrounding `timeout(...)`) → `client.read_to_end(&mut raw).await.expect("server closes the wire")` (the helper is internally bounded).
R6. Imports per file: `use async_language_server::server::ServerError;` only where asserted (#12); harness items via `use crate::harness::*;`-style explicit lists — match each file's actual usage, no glob.
R7. Gated/Panicking/Configurable server definitions MOVE into the concern file that uses them (they are not shared across concerns), with their helper fns (`no_hover`, `hover_params` → `hover_params` stays in harness as shared; `no_hover` moves with `PanickingServer`).

---

### Task 1: tests/wire skeleton + harness + the two TCP tests

**Files:**
- Create: `tests/wire/main.rs`, `tests/wire/harness.rs`, `tests/wire/termination.rs`
- Delete: `tests/lsp_wire.rs`

**Interfaces:**
- Produces (used by Tasks 2–5): `crate::harness::{WIRE_TIMEOUT, bounded, spawn_serve, RawClient, EchoServer, echo_hover, did_open, hover_params}` with the exact signatures in the code below.

- [ ] **Step 1: Create `tests/wire/main.rs`**

```rust
//! The wire tier: black-box tests through the only general public entry
//! point — `serve()` over a loopback TCP socket. One integration target,
//! one shared harness, per-concern test files.

mod harness;
mod termination;
```

(Each later task adds its `mod` line here, keeping the list alphabetical.)

- [ ] **Step 2: Create `tests/wire/harness.rs`** — the merged harness (W3's fallible style + W2's stash/bounded semantics):

```rust
//! Shared harness for the wire tier: a raw JSON-RPC client over a real
//! TCP stream to a spawned `serve()`, plus the test servers. The client
//! uses no async-lsp code — it sees the exact wire bytes and stays
//! isolated from async-lsp client-path bugs.
//!
//! The helpers are fallible and `expect()` calls live inside the
//! `#[tokio::test]` functions: `expect_used` is deny crate-wide and only
//! relaxed for code clippy can see is a test (`clippy.toml`,
//! `allow-expect-in-tests`), which does not cover helpers in integration
//! test crates.

use std::time::Duration;

use async_language_server::server::{Server, ServerResult, Transport, serve};
use async_lsp::lsp_types::{Hover, HoverContents, HoverParams, MarkedString, Position, Range};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufStream};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub(crate) const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) type WireError = Box<dyn std::error::Error + Send + Sync>;

/// Bounds any future by `WIRE_TIMEOUT`; the error is a plain message
/// because the elapsed case is always a test failure at the call site.
pub(crate) async fn bounded<F: std::future::Future>(future: F) -> Result<F::Output, WireError> {
    timeout(WIRE_TIMEOUT, future)
        .await
        .map_err(|_| "exceeds the bounded wire timeout")
}

/// Binds a loopback listener, spawns `serve()` against its port, accepts
/// the server's connection, and hands back the raw client plus the serve
/// task handle. Bind and accept are both bounded.
pub(crate) async fn spawn_serve<S>(
    server: S,
) -> (
    RawClient,
    tokio::task::JoinHandle<ServerResult<()>>,
)
where
    S: Server + Clone + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener binds");
    let port = listener
        .local_addr()
        .expect("listener reports its address")
        .port();
    let handle = tokio::spawn(serve(Transport::Socket(port), server));
    let (stream, _) = timeout(WIRE_TIMEOUT, listener.accept())
        .await
        .expect("server connects within the bound")
        .expect("accept succeeds");
    (
        RawClient {
            stream: BufStream::new(stream),
            pending: Vec::new(),
        },
        handle,
    )
}

pub(crate) struct RawClient {
    stream: BufStream<TcpStream>,
    /// Server-initiated messages seen while waiting for a response.
    pending: Vec<Value>,
}

impl RawClient {
    pub(crate) async fn write_message(&mut self, message: &Value) -> Result<(), WireError> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        timeout(WIRE_TIMEOUT, async {
            self.stream.write_all(header.as_bytes()).await?;
            self.stream.write_all(body.as_bytes()).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_| "writes exceed the bounded wire timeout")??;
        Ok(())
    }

    /// Reads one framed message; `None` on EOF (server closed the wire).
    pub(crate) async fn read_message(&mut self) -> Result<Option<Value>, WireError> {
        let read = timeout(WIRE_TIMEOUT, async {
            let mut content_length = None;
            let mut line = Vec::new();
            loop {
                line.clear();
                if self.stream.read_until(b'\n', &mut line).await? == 0 {
                    return Ok(None); // EOF
                }
                let trimmed = trim_crlf(&line);
                if trimmed.is_empty() {
                    break;
                }
                if let Some(value) = std::str::from_utf8(trimmed)
                    .map_err(|_| "header line is not UTF-8")?
                    .strip_prefix("Content-Length: ")
                {
                    content_length = Some(value.trim().parse::<usize>()?);
                }
            }
            let length = content_length.ok_or("Content-Length header missing")?;
            let mut body = vec![0u8; length];
            self.stream.read_exact(&mut body).await?;
            Ok(Some(serde_json::from_slice(&body)?))
        })
        .await
        .map_err(|_| "reads exceed the bounded wire timeout")?;
        read
    }

    /// Writes raw bytes, bypassing framing — for malformed-input tests.
    pub(crate) async fn write_raw(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        timeout(WIRE_TIMEOUT, self.stream.write_all(bytes))
            .await
            .map_err(|_| "raw write exceeds the bounded wire timeout")??;
        Ok(())
    }

    /// Drains all remaining bytes to EOF, including any still buffered.
    pub(crate) async fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<usize, WireError> {
        Ok(timeout(WIRE_TIMEOUT, self.stream.read_to_end(out))
            .await
            .map_err(|_| "drain exceeds the bounded wire timeout")??)
    }

    pub(crate) async fn send_request(
        &mut self,
        id: i64,
        method: &str,
        params: Value,
    ) -> Result<(), WireError> {
        self.write_message(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }))
        .await
    }

    pub(crate) async fn await_response(&mut self, id: i64) -> Result<Value, WireError> {
        // A response to this id may already have been stashed in `pending`
        // while reading past it for an earlier message.
        if let Some(position) = self
            .pending
            .iter()
            .position(|message| is_response_to(message, id))
        {
            return Ok(self.pending.remove(position));
        }
        loop {
            let message = self
                .read_message()
                .await?
                .ok_or("connection closed by the server")?;
            if is_response_to(&message, id) {
                return Ok(message);
            }
            self.pending.push(message);
        }
    }

    pub(crate) async fn request(
        &mut self,
        id: i64,
        method: &str,
        params: Value,
    ) -> Result<Value, WireError> {
        self.send_request(id, method, params).await?;
        self.await_response(id).await
    }

    pub(crate) async fn notify(&mut self, method: &str, params: Value) -> Result<(), WireError> {
        self.write_message(&json!({
            "jsonrpc": "2.0", "method": method, "params": params
        }))
        .await
    }

    /// Full initialize handshake; returns the `InitializeResult`.
    pub(crate) async fn initialize_client(&mut self, encodings: &[&str]) -> Result<Value, WireError> {
        let response = self
            .request(
                1,
                "initialize",
                json!({
                    "processId": null,
                    "capabilities": {
                        "general": { "positionEncodings": encodings }
                    }
                }),
            )
            .await?;
        self.notify("initialized", json!({})).await?;
        Ok(response["result"].clone())
    }
}

fn is_response_to(message: &Value, id: i64) -> bool {
    message.get("id").and_then(Value::as_i64) == Some(id)
        && (message.get("result").is_some() || message.get("error").is_some())
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

// --- shared test server and request-shape helpers ---

#[derive(Clone)]
pub(crate) struct EchoServer;

pub(crate) fn echo_hover(position: Position) -> Option<Hover> {
    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String("echo".into())),
        range: Some(Range::new(position, position)),
    })
}

impl Server for EchoServer {
    fn hover(
        &self,
        _state: async_language_server::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
        let position = params.text_document_position_params.position;
        async move { Ok(echo_hover(position)) }
    }
}

pub(crate) fn did_open(uri: &str, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": "test",
            "version": 1,
            "text": text
        }
    })
}

pub(crate) fn hover_params(uri: &str, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": character }
    })
}
```

- [ ] **Step 3: Create `tests/wire/termination.rs`** — migrate `tests/lsp_wire.rs`'s two tests (`socket_connect_failure_maps_to_tcp_connect_error`, `serve_happy_path_over_tcp_resolves_ok`). Read the current file first; the tests migrate with these changes only: the local `RawClient`/`trim_crlf`/`EchoServer`/`WIRE_TIMEOUT` definitions are deleted (harness provides them via `use crate::harness::{EchoServer, RawClient, WIRE_TIMEOUT, spawn_serve};` plus `tokio::time::timeout` where still used directly); the happy-path test's manual listener/accept block becomes `let (mut client, serve_handle) = spawn_serve(EchoServer).await;`; every client call gains `.expect(...)` per rule R2 where the harness is now fallible (the current file already has them). The connect-failure test keeps its own listener-bind/drop preamble verbatim.

- [ ] **Step 4: Delete `tests/lsp_wire.rs`.**

- [ ] **Step 5: Verify**

Run: `cargo test --test wire`
Expected: 2 passed. Then the full battery (lib untouched: 143+1+13 in default config).

- [ ] **Step 6: Report for commit**

Suggested: `test: tests/wire target with shared harness; migrate TCP tests`

---

### Task 2: lifecycle.rs (catalog #1, #3, #4×2)

**Files:**
- Create: `tests/wire/lifecycle.rs`
- Modify: `tests/wire/main.rs` (add `mod lifecycle;`)

**Interfaces:** Consumes the Task-1 harness.

- [ ] **Step 1: Migrate four tests** from `src/server/tests.rs` (read the current bodies at these anchors; re-anchor by name if drifted):
  - `initialize_negotiates_position_encoding_end_to_end` (~line 259)
  - `requests_before_initialize_are_rejected` (~319)
  - `double_initialize_is_rejected` (~340)
  - `requests_after_shutdown_are_rejected` (~359)

  Apply rules R1–R3 (`spawn_serve(...).await`, `.expect(...)` on client calls, bounded tail). Each test's assertions (utf-8 negotiation, −32002, −32600 ×2) stay verbatim. Tail idiom: `drop(client); let _ = bounded(server).await.expect("serve task joins");` — EXCEPT `initialize_negotiates...` which currently ends `let _ = bounded(server).await;` after `drop(client)` → same replacement.

- [ ] **Step 2: Verify**

Run: `cargo test --test wire`
Expected: 6 passed.

- [ ] **Step 3: Report for commit**

Suggested: `test: migrate lifecycle wire tests (negotiation, pre-init, double-init, post-shutdown)`

---

### Task 3: conversion.rs + dispatch.rs (catalog #2, #6, #5)

**Files:**
- Create: `tests/wire/conversion.rs`, `tests/wire/dispatch.rs`
- Modify: `tests/wire/main.rs` (add both mods)

**Interfaces:** Consumes the Task-1 harness (`did_open` used by both files).

- [ ] **Step 1: Migrate into `conversion.rs`** from `src/server/tests.rs`:
  - `utf16_positions_round_trip_through_real_serialization` (~286; uses the local `did_open` helper — now `crate::harness::did_open`)
  - `incremental_did_change_applies_over_the_wire` (~421)

  Rules R1–R3; assertions verbatim (character 3 / character 6 round-trips).

- [ ] **Step 2: Migrate into `dispatch.rs`**:
  - `unwired_methods_return_method_not_found` (~383; the `-32602`-before-`-32601` comment block migrates with it verbatim)

- [ ] **Step 3: Verify**

Run: `cargo test --test wire`
Expected: 9 passed.

- [ ] **Step 4: Report for commit**

Suggested: `test: migrate conversion and dispatch wire tests`

---

### Task 4: staleness.rs + robustness.rs (catalog #7, #8, #9, #14, #15)

**Files:**
- Create: `tests/wire/staleness.rs`, `tests/wire/robustness.rs`
- Modify: `tests/wire/main.rs` (add both mods)

**Interfaces:** Consumes the harness (`hover_params`, `EchoServer`); defines `GatedServer` + `PanickingServer` locally in `robustness.rs` (they are single-concern).

- [ ] **Step 1: Migrate into `staleness.rs`**:
  - `stale_document_answers_content_modified_then_succeeds_on_retry` (~526), together with `GatedServer`'s definition (~466–491) and its `tokio::sync::{mpsc, watch}` imports. The probe-request sequencing (`workspace/symbol` probe between didChange and release) migrates VERBATIM — it encodes async-lsp polling-order facts. Rules R1–R3.
  - NOTE: `GatedServer` is used by BOTH the staleness test (this file) and the concurrency/cancel tests (robustness.rs, Step 2). To avoid cross-file coupling, define `GatedServer` ONCE in `robustness.rs` as `pub(crate)` and import it in `staleness.rs` (`use crate::robustness::GatedServer;`) — it is gated-server machinery, robustness is its home.

- [ ] **Step 2: Migrate into `robustness.rs`** (with `GatedServer` + `PanickingServer` + `no_hover` living here):
  - `panicking_handler_returns_structured_error` (~598, with `PanickingServer` ~493 and `no_hover` ~498)
  - `at_most_eight_requests_run_concurrently` (~624) — the async-lsp#30 TRIPWIRE: the two 250 ms bounded absence-checks, the PR-link comment block, the restoration instructions, and the final `server.abort();` migrate VERBATIM; only the spawn and `.expect` wrappings change.
  - `cancel_request_answers_request_cancelled` (post-803) — cancel→await(−32800)→release ordering preserved.
  - `malformed_header_closes_the_connection` (post-803) — rule R4 for the garbage write (`client.write_raw(b"Content-Length: abc\r\n\r\n")`), the EOF assertion via `read_message` returning `None`, and the loop-outcome assertion via `bounded(server)` with the two-layer expect (task joins + inner `Err`).

- [ ] **Step 3: Verify**

Run: `cargo test --test wire`
Expected: 14 passed; the tripwire stable (~0.5 s).

- [ ] **Step 4: Report for commit**

Suggested: `test: migrate staleness and robustness wire tests (incl. async-lsp#30 tripwire)`

---

### Task 5: termination.rs completion (catalog #10, #11)

**Files:**
- Modify: `tests/wire/termination.rs`

**Interfaces:** Consumes the harness; `ConfigurableServer` defined locally.

- [ ] **Step 1: Migrate into `termination.rs`** (joining #12/#13 from Task 1):
  - `shutdown_exit_terminates_the_server_loop_cleanly` (~704): serve-handle join asserted via `bounded(server).await.expect("serve task joins").expect("serve loop resolves Ok(())")`; the EOF drain via rule R5 (`client.read_to_end(&mut raw).await.expect("server closes the wire")`), asserting no trailing bytes.
  - `workspace_configuration_request_is_served_mid_request` (~727, with `ConfigurableServer` ~678–702): the capability override (`DiagnosticServerCapabilities::Options`) migrates verbatim with its imports; `tokio::fs` fixture calls stay; the mid-flight read loop uses `client.read_message().await.expect(..).expect("wire stays open")` — check the current body for the exact `.ok_or`-style EOF handling and adapt with `.expect` per R2, preserving the pending-stash interplay (`await_response` checking `pending` first).

- [ ] **Step 2: Verify**

Run: `cargo test --test wire`
Expected: 16 passed.

- [ ] **Step 3: Report for commit**

Suggested: `test: migrate termination and server-to-client wire tests`

---

### Task 6: Demolition, dupes, steering doc, full battery

**Files:**
- Delete: `src/server/tests.rs`
- Modify: `src/server/mod.rs` (remove `#[cfg(test)] mod tests;`)
- Modify: `.claude/rules/testing.md`
- Modify: `.dupes-ignore.toml` (via `cargo dupes cleanup` + any reasoned additions)

**Interfaces:** none.

- [ ] **Step 1: Demolish** — delete `src/server/tests.rs`; remove the `#[cfg(test)]\nmod tests;` declaration from `src/server/mod.rs`. The duplex adapters die with the file (they were private to it); `run_over_streams` in `serve.rs` STAYS (production seam; coverage is now transitive through every wire test).

- [ ] **Step 2: dupes** — run `cargo dupes cleanup` (removes stale fingerprints: the EchoServer pair `646121fd`, trim_crlf `e4c0eb70`, and any wrapper-residue groups that dissolved). Then `cargo dupes check; echo exit=$?` → 0. If any NEW group surfaced (e.g. did_open/hover_params normalizing together), add a reasoned entry or leave it visible — never loosen thresholds. Report the final stats.

- [ ] **Step 3: Steering doc** — in `.claude/rules/testing.md`:
  - Tier table: replace the W2 and W3 rows with one row: `| wire black-box | tests/wire/ (single target) | serve() + Transport::Socket over loopback TCP: lifecycle, staleness retry, panic mapping, concurrency bound, termination, wire encoding |`
  - Harness inventory: replace the `src/server/tests.rs` bullet with: `tests/wire/harness.rs — spawn_serve, fallible bounded RawClient, EchoServer/GatedServer/PanickingServer/ConfigurableServer (per concern file), did_open/hover_params.`
  - Update the tripwire paragraph's path to `tests/wire/robustness.rs`; keep its content.
  - The W2/W3 "deferred duplication" note and the echo-fixpoint ceiling note keep their content with updated paths.

- [ ] **Step 4: Full battery + counts**

```bash
cargo test                          # lib 129 + arch 1 + wire 16 + doctests 13
cargo test --no-default-features    # 112-ish lib + 1 + 16 + 13 (recount; wire is ungated)
cargo test --all-features           # same as default shape
cargo fmt --check && cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test --test architecture && cargo dupes check
```
Expected: lib drops 143→129 (14 tests left), the wire target carries 16, zero failures anywhere, dupes check exit 0.

- [ ] **Step 5: Report for commit**

Suggested: `refactor: delete src/server/tests.rs; wire tier fully black-box through serve()`

---

## Self-Review (done at plan time)

- **Spec coverage**: layout (Tasks 1–5 create every file from the spec's tree); harness § (Task 1, full code); migration 16 tests (2+4+3+5+2 = 16 ✓); deletions § (Task 6: tests.rs, adapters via file deletion, lsp_wire.rs in Task 1); dupes § (Task 6 Step 2); steering § (Task 6 Step 3); constraints § (Global Constraints verbatim); out-of-scope respected.
- **Placeholders**: none — motion tasks carry anchors + rules (the proven T1/T4 pattern); the harness is complete code; Task 6's doc edit specifies exact replacement rows.
- **Type consistency**: `spawn_serve` async returning `(RawClient, JoinHandle<ServerResult<()>>)`; `bounded` returns `Result<F::Output, WireError>` (fallible — helper-expect ban); `RawClient::{write_message, read_message, write_raw, read_to_end, send_request, await_response, request, notify, initialize_client}` signatures used consistently across Tasks 2–5 rules; `GatedServer` defined once (robustness.rs, `pub(crate)`), imported by staleness.rs.
