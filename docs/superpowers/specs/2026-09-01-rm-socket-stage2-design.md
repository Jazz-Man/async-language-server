# Socket removal — stage 2: the removal itself

Date: 2026-09-01. Branch: `feature/rm-socket`. Inputs: the stage-1 census
(`docs/superpowers/research/2026-09-01-socket-usage-census.md` — every
disposition below traces to a census row) and the owner's three decisions:
NO sockets anywhere (business logic, tests, examples, dependencies,
configs, prose); `serve()` is publicly stdio-only with no transport
parameter; `run_over_streams` STAYS as the `pub(crate)` seam
("видозмінити").

## Goal

Every socket touchpoint the census mapped is gone; `serve(server)` is the
crate's only entry point, speaking stdin/stdout; the W2 duplex tier
becomes the single wire-test tier, its monolith split per concern; docs
and dupes config match the new reality. The `-D warnings` battery turns
green again (the deprecation transient ends with the marks' removal).

## Production changes

1. **`src/transport.rs` deleted entirely** (census: 35 internal sites +
   doctest rows, all disposition delete-with-file). Nothing public
   survives it; the private tokio→futures adapters for stdin/stdout
   (~30 lines, the poll-bridging shape the file already has for Stdio)
   move into `src/server/serve.rs`, because async-lsp's `run_buffered`
   speaks futures traits.
2. **`serve(server)` — new signature**: `pub async fn serve<S>(server: S)
   -> ServerResult<()>` where `S: Server + Clone + Send + Sync +
   'static`. Body: adapt `tokio::io::stdin()/stdout()` to the futures
   traits, delegate to `run_over_streams`. The doctest updates (still
   `no_run`). Examples (`minimal.rs`, `tree_sitter.rs`) drop the
   `Transport::Stdio` argument.
3. **`run_over_streams` stays `pub(crate)`, unchanged in mechanics**;
   its doc comment is rewritten to state the honest role: the internal
   stdio seam over which `serve()` runs, and which the wire-tier tests
   drive over in-memory duplex pipes (no sockets).
4. **All five `#[deprecated]` marks removed** (transient scaffolding
   ends; the rustc/clippy `-D warnings` transient failure ends with
   them).
5. **`ServerError::TcpConnect` deleted**: the variant, its two unit
   tests (`tcp_connect_preserves_its_source`,
   `tcp_connect_maps_to_internal_error`), and the doctest that uses it
   as the `Display` example — the example is replaced with
   `InvalidFilePath` (the doc keeps a resident), and the
   `From<ServerError>` arm coverage remains complete over the surviving
   variants (Rpc/Lsp/Io/InvalidFilePath/Other).
6. **Manifest**: tokio main features lose `net` (transitive `socket2`
   leaves Cargo.lock with it). `io-util` placement is decided
   empirically: if `cargo build` (production targets only, no
   `--all-targets`) compiles green without it, move `io-util` to the
   tokio dev-dependency (the duplex harness is its only consumer —
   product rule: production does not pay for test infrastructure);
   otherwise it stays in main with a one-line reason comment. Dev
   features otherwise unchanged (`time`/`sync`/`fs`/`macros`/rt stay;
   census: nothing to remove there).

## Test changes

1. **`tests/lsp_wire.rs` deleted** (census rows: 8 sites + the field
   binding). `socket_connect_failure_maps_to_tcp_connect_error` is
   socket-natured and dies by the owner's rule.
   `serve_happy_path_over_tcp_resolves_ok`'s contract — full cycle and
   `serve()` resolving `Ok(())` — is already pinned by W2's
   `shutdown_exit_terminates_the_server_loop_cleanly` plus its 13
   siblings; no coverage is lost.
2. **`src/server/tests.rs` (872 lines) split into the single wire tier**:

```
src/server/
├── mod.rs            (`#[cfg(test)] mod tests;` stays)
├── testing.rs        harness: FuturesReadHalf/FuturesWriteHalf adapters,
│                     RawClient (+pending stash), spawn_wire_server,
│                     bounded, is_response_to, trim_crlf, echo_hover,
│                     did_open, hover_params, WIRE_TIMEOUT
└── tests/
    ├── mod.rs        mod declarations only
    ├── lifecycle.rs  #1 negotiation; #3 pre-init −32002; #4 double-init
    │                 and post-shutdown −32600
    ├── conversion.rs #2 UTF-16 round-trip; #6 incremental didChange
    ├── dispatch.rs   #5 unwired → −32601 (parametrized)
    ├── staleness.rs  #7 CONTENT_MODIFIED then retry (probe ordering
    │                 verbatim)
    ├── robustness.rs #8 panic→error; #9 concurrency bound + async-lsp#30
    │                 tripwire (verbatim, abort cleanup); #14 cancel;
    │                 #15 framing smoke
    └── termination.rs #10 shutdown/exit Ok(()) + EOF; #11
                      workspace/configuration mid-flight
```

   The four test servers live where used: `EchoServer` in `testing.rs`
   (shared), `GatedServer`/`PanickingServer` in `robustness.rs`
   (`GatedServer` shared with `staleness.rs` via `pub(crate)`),
   `ConfigurableServer` in `termination.rs`. This is a PURE MOTION
   split: bodies, assertions, probe orderings, the tripwire's two 250 ms
   absence-checks and `server.abort()` migrate verbatim; only module
   paths and imports change. The W2/W3 duplication question dissolves:
   W3 is gone, the harness types exist exactly once.
3. **Dupes config**: `cargo dupes cleanup` removes the dissolved entries
   — the deferred EchoServer pair (`646121fd`) and trim_crlf
   (`e4c0eb70`) lose their second member; `.dupes-ignore.toml:71`
   (LspTransportWrite forwarding group) is deleted as dead config.
   `cargo dupes check` must exit 0; any NEW group from the split (e.g.
   concern files normalizing together) gets a reasoned entry or stays
   visible — never a threshold loosening.

## Documentation changes

- `.claude/rules/testing.md`: tier table — one wire tier
  (`src/server/tests/`, duplex through the internal seam); the W3 row
  and the W2/W3 deferred-duplication note die; harness inventory maps
  `src/server/testing.rs`; tripwire/`-32602`/echo-fixpoint notes keep
  content with updated paths.
- `.claude/rules/structure.md` and `CLAUDE.md`: remove
  `Transport`/`Socket` mentions from the serve/transport descriptions
  (census docs-axis rows).
- `README.md`: checked via the census grep; any transport mentions
  updated to `serve(server)`.
- `error-handling.md`: verified to carry no TcpConnect mention; if the
  empirical check contradicts this, update per the census
  update-text disposition.

## Constraints

- Owner's rule, verbatim scope: nothing socket-related survives —
  business logic, tests, examples, dependencies, configs, prose.
- Public surface NET shrinks: `Transport`, `LspTransportRead`,
  `LspTransportWrite`, `ServerError::TcpConnect` out; nothing new
  public in.
- The split is behavior-preserving (pure motion); assertion values,
  test names, and probe orderings unchanged.
- Full battery in all three feature configurations green at every task
  boundary; `cargo dupes check` exit 0; the census's four axes re-swept
  clean at the end (grep-level proof of "no sockets").
- Git read-only for agents; the owner commits; English artifacts;
  LSP-first navigation; no lint suppression.

## Verification (cycle-end, beyond the battery)

Re-run the census sweeps — LSP findReferences on the removed names must
return zero live references; the docs-axis grep must return zero
socket/tcp rows outside history docs; `Cargo.lock` free of `socket2`
after `cargo update -p tokio` if needed (no full `cargo update`).

## Out of scope

The registered testing-cycle follow-ups (DocumentLinkResolve hole,
RangeExt split_at flavor semantics, diagnostic naming doc); any new
entry points or features.
