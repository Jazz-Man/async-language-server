# Socket removal — stage 1: deprecation and usage census

Date: 2026-09-01. Branch: `feature/rm-socket`. Supersedes (for
execution; both stay in history): `2026-09-01-wire-tests-split-design.md`
and its plan — the owner's no-sockets decision changed the foundation.

## Owner decision (binding)

Sockets go completely: no fallbacks, no socket mentions in tests, no
dependencies kept for socket support — nothing socket-related remains.
Downstream servers are Stdio-only. The owner's workflow (familiar from
PHP/Node): mark the functionality deprecated first, collect the full
usage picture via references, THEN decide the architecture.

## Stage 1 scope — deprecation marks + census, nothing else

### Deprecation marks (temporary scaffolding of this cycle)

Each item gets `#[deprecated(note = "sockets are being removed; see
docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md")]`
(or the variant-position form where applicable):

1. `Transport` (the whole type — `src/transport.rs`)
2. `LspTransportRead`, `LspTransportWrite` (`src/transport.rs`)
3. `ServerError::TcpConnect` (`src/error.rs`)
4. `serve()` in its current `(transport, server)` signature — every
   caller is a change point (examples, tests, doctest)

The marks exist to make the compiler flag every internal use; they are
removed together with the socket machinery in stage 2.

**Mechanical note:** the battery runs `-D warnings`, so after marking,
`cargo build --all-targets` FAILS by design at every use site. The census
therefore runs WITHOUT `-D`:

```bash
cargo build --all-targets 2>&1 | grep -A2 "deprecated"
```

and the per-site fix during stage 2 turns each error green by removal or
rewrite — the deprecation errors double as a completion checklist.

### Census (two independent finders, cross-checked)

1. **LSP `findReferences`** on each deprecated symbol — the structured map
   (the owner's habitual tool).
2. **Compiler deprecation warnings** from the command above — the proof of
   completeness (every reference the compiler sees is flagged).

Plus two axes LSP/compiler do not cover:
3. **Dependency axis**: every use of `tokio::net` (the `net` tokio feature
   leaves the manifest with the sockets; `io-std` stays for stdio) and any
   other socket-only dependency surface (`TcpStream`, `TcpListener`,
   `SocketAddr`).
4. **Docs/text axis**: `grep -ri "socket\|tcp" CLAUDE.md .claude/rules/
   README.md examples/` — prose and examples must end up with zero socket
   mentions.

### Deliverable

A census report (committed as
`docs/superpowers/research/2026-09-01-socket-usage-census.md`) with one
table per axis: symbol → file:line → classification (production / test /
doc / manifest) → stage-2 disposition suggestion (delete / rewrite-via /
update-text). The report ends with the counts the stage-2 architecture
decision needs: how many `serve()` call sites, whether anything outside
`transport.rs` touches TCP types directly, and the full tokio-feature
delta.

## Out of scope (stage 2, decided on the census)

Architecture of the post-socket entry point (`serve_over`-style generic
vs internal seam vs other); wire-test migration; `transport.rs`'s final
fate (owner's hypothesis: the module dies entirely — stdio wrapping moves
into `serve.rs`); steering-doc and dupes updates.

## Constraints

- The deprecation marks and census CHANGE NO behavior; the battery is
  expected to fail `-D warnings` while marks are in place (recorded here
  as the intended transient state; the census runs without `-D`).
- No new dependencies; git read-only for agents; English artifacts;
  LSP-first navigation.
