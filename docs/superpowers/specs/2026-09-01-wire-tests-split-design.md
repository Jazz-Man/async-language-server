# Wire-tests consolidation design

Date: 2026-09-01. Branch: `feature/wire-tests`. Inputs: the testing cycle's
final review (ready-to-merge verdict; the W2/W3 duplication deferred by
owner decision 2026-08-31), the rust-skills corpus consultation (2026-09-01,
sonnet research, citations below), and two owner decisions: scope = split +
dedup together; approach = full migration to `tests/` (B).

## Goal

One black-box wire-test tier: every wire test drives the PUBLIC `serve()`
entry point over loopback TCP from a single integration target, the
`src/server/tests.rs` monolith disappears, and the W2/W3 duplication
(RawClient, EchoServer, trim_crlf) dissolves into one harness.

## Why this shape (grounding)

- **rust-skills places tests by observability, not size** — "If a behavior
  can be tested entirely through the public API, prefer an integration test
  under `tests/`" (`test-integration-dir`); `#[cfg(test)]` in `src/` is for
  "small private helpers or invariants that cannot be observed through the
  public surface". All 13 W2 tests assert client-observable behavior; their
  only non-public aspect was the transport, not the assertions.
- **Directory test targets must use `main.rs`** — "a `mod.rs` here compiles
  ZERO test targets: cargo discovers only `tests/*.rs` and
  `tests/<dir>/main.rs`" (`test-integration-dir`, "Organizing Many Tests").
- **TCP cost is measured, not assumed**: 2 existing W3 tests run in
  0.00–0.01 s; loopback bind+connect+serve+full-cycle ≈ 2–5 ms per test.
  Migrating 13 tests adds ~30–40 ms total. The suite's deliberate cost
  (the async-lsp#30 tripwire's 2 × 250 ms bounded absence-checks) is
  identical in every approach.
- **Self-import probe**: `use async_language_server::…` inside the lib
  itself does NOT compile (E0433, empirically verified 2026-09-01) — which
  killed the shared-file option for server types while two compilation
  contexts existed. Migration to one `tests/` context removes the two-context
  problem entirely, so that constraint becomes moot.

## Layout

```
tests/
├── architecture.rs          (unchanged)
└── wire/
    ├── main.rs              aggregator: mod harness; mod lifecycle; …
    ├── harness.rs           the single shared harness (one target ⇒ no
                             tests/common/ needed; the corpus reserves that
                             pattern for sharing ACROSS targets)
    ├── lifecycle.rs         #1 negotiation end-to-end; #3 pre-initialize
    │                        −32002; #4 double-initialize / post-shutdown
    │                        −32600
    ├── conversion.rs        #2 UTF-16 ↔ UTF-8 through real serialization;
    │                        #6 incremental didChange over the wire
    ├── dispatch.rs          #5 unwired methods → −32601 (parametrized,
    │                        minimally-valid params; −32602 fires first on
    │                        garbage)
    ├── staleness.rs         #7 CONTENT_MODIFIED then retry succeeds (the
    │                        probe-request ordering is preserved verbatim)
    ├── robustness.rs        #8 handler panic → structured error; #9
    │                        concurrency bound (async-lsp#30 tripwire);
    │                        #14 $/cancelRequest −32800; #15 framing smoke
    └── termination.rs       #10 shutdown/exit → serve() resolves Ok(());
                             #11 workspace/configuration mid-request;
                             #12 TcpConnect failure mapping; #13 happy path
```

No sibling `tests/wire.rs` may exist (self_named_module_files); inside
`wire/`, plain submodule files — no nesting beyond one level.

## Harness (`tests/wire/harness.rs`)

- `spawn_serve::<S>(server) -> (RawClient, JoinHandle<ServerResult<()>>)` —
  binds `127.0.0.1:0`, spawns `serve(Transport::Socket(port), server)`,
  `accept()`s, returns the client; bind, accept, AND the join are bounded
  (this closes the final review's Minor: W3's unbounded `accept()` and
  `serve_handle.await`).
- `RawClient` over `BufStream<TcpStream>`: fallible helpers
  (`write_message`/`read_message`/`request`/`notify`/`initialize_client`
  including the `initialized` notification), I/O bounded by construction
  (the W2 fix carried over), pending-stash semantics (needed by #11),
  `is_response_to`, `trim_crlf`, `WIRE_TIMEOUT`.
- Test servers in the same module: `EchoServer` (+`echo_hover`),
  `GatedServer` (entered-channel `()` + watch release), `PanickingServer`
  (+`no_hover`), `ConfigurableServer` (capabilities override advertising
  the diagnostic provider). Helpers: `did_open`, `hover_params`.
- Style: fallible helpers with `?`/`.expect(...)` INSIDE `#[tokio::test]`
  fns only — the integration crate denies `expect_used` outside them
  (the proven `tests/lsp_wire.rs` pattern; `allow-expect-in-tests` does
  not cover helper methods).

## Migration

16 tests move verbatim in semantics: 14 from `src/server/tests.rs` (the 13
catalog items — #4 spans two tests: double-initialize and post-shutdown) +
2 from `tests/lsp_wire.rs` (which is deleted). Assertion values, the
staleness probe ordering, the tripwire's two 250 ms bounded absence-checks
and `server.abort()` cleanup, `processId: null`, and the `-32602`-before-
`-32601` router note all carry over unchanged. What changes per test:
transport wiring (duplex → loopback TCP through `spawn_serve`) and
fallible-call adaptation (~40 call sites gain `?`/`.expect(...)`).

## Deletions and consequences

- `src/server/tests.rs` deleted; `#[cfg(test)] mod tests;` removed from
  `src/server/mod.rs` — `src/server/` holds no test files afterwards.
- The duplex adapters `FuturesReadHalf`/`FuturesWriteHalf` deleted.
  `run_over_streams` stays `pub(crate)` — the production seam `serve()`
  delegates to; its coverage becomes transitive (every wire test reaches
  it through the public entry point).
- `tests/lsp_wire.rs` deleted (tests migrated).
- Dupes ignore list: the deferred entries `646121fd` (EchoServer pair) and
  `e4c0eb70` (trim_crlf) dissolve — one definition each remains; stale
  entries removed via `cargo dupes cleanup`. Any NEW group surfacing after
  migration (e.g. did_open/hover_params normalizing together) gets a
  reasoned entry or is left unhidden — never a threshold loosening.
- `.claude/rules/testing.md` updated: tier table loses W2 (one wire tier:
  `tests/wire/` through public `serve()` on loopback TCP); harness
  inventory maps `tests/wire/harness.rs`; tripwire/`-32602`/fixpoint notes
  keep their content with updated paths.

## Constraints

- Public surface unchanged: no new `pub`, no features, no test-util gate
  (the product rule's small-surface stance holds; `test-util-feature`
  from rust-skills was considered and rejected as unnecessary).
- Determinism: channel gates never sleeps (the tripwire's bounded
  absence-checks are the sanctioned exception, preserved); every cross-task
  await bounded; expected-EOF asserted at shutdown.
- The full battery in all three feature configurations gates the work;
  expected count shift: lib 143 → 129 (14 tests leave), the wire target
  carries 16 (14 migrated + 2 from the former `lsp_wire.rs`); totals
  preserved.
- No lint suppression anywhere; all artifacts English.

## Out of scope

The registered testing-cycle follow-ups (DocumentLinkResolve conversion
hole, RangeExt split_at flavor semantics, diagnostic naming doc); any
production-code change beyond deletions listed above.
