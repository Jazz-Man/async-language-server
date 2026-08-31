# Testing

This rule is normative for all test work in this crate. It documents the
pipeline built in the 2026-08 testing cycle (spec:
`docs/superpowers/specs/2026-08-31-testing-implementation-design.md`;
test-catalog numbering per the research,
`docs/superpowers/research/2026-08-31-testing-strategy.md` §4.3).

## Philosophy

Type first, test second. Before writing a test, ask whether a type can
remove the invalid state the test would pin: `RangeError` made `RangeExt`
fallible and deleted its `# Panics` contracts, and the cycle's signature
audit applied the same criterion across the public surface, approving one
further type — `QueryError` on `Document::query`. A test exists only for
behavior no type can express; there are no tests for quantity or coverage
statistics. The typing criterion: a type must remove a representable
invalid state or separate a genuinely confusable pair — otherwise the
type is ceremony.

## The three tiers

| tier | where | what it pins |
|---|---|---|
| W0 unit | inline `#[cfg(test)] mod tests` / sibling `tests.rs` | arithmetic, conversion math, state machines, `Request` conversion hooks |
| W2 wire white-box | `src/server/tests.rs` | framing + serde + the real middleware stack (`serve::run_over_streams`) over `tokio::io::duplex`, driven by a raw JSON-RPC client |
| W3 wire black-box | `tests/lsp_wire.rs` | `serve()` + `Transport::Socket` over real TCP |

Choose the lowest tier that can express the assertion. W2/W3 exist only
for what unit tests cannot see: lifecycle gating, staleness retry, panic
mapping, the concurrency bound, termination, wire encoding.

The concurrency test (`at_most_eight_requests_run_concurrently`) doubles
as a tripwire for an upstream deadlock: with `ConcurrencyLayer` at
capacity, async-lsp 0.2.4's `MainLoop` stops polling in-flight tasks
while waiting for `poll_ready` (oxalica/async-lsp#30), so the ninth
request never proceeds even after the gates release — the test asserts
that absence, then aborts the server task because the join handle can
never complete. When that absence-check fails after an async-lsp upgrade,
the upstream fix has landed: flip it to asserting recovery, following the
instructions in the test's own comment.

## Harness inventory

- `crate::testing` (`src/testing.rs` — a `#[cfg(test)] pub(crate)` module
  declared in `src/lib.rs`, scopeless like `src/error.rs`) — the single
  shared home for fixtures: `line_position`, `line_range`, `same_line`
  (LSP positions and ranges), `url`, `TestServer`, `open_document`,
  `state_with_documents`, `temp_workspace(prefix, name)`,
  `workspace_folder`, `diagnostic`, and `json_matchers`
  (tree-sitter-gated). The `"🙂abc"` document and the UTF-16 encoding in
  `state_with_documents` are load-bearing: U+1F642 is 4 UTF-8 bytes but
  2 UTF-16 units, so byte offset 4 == UTF-16 offset 2. The byte and
  tree-sitter `r()` twins stay local in their own test files: each flavor
  names its local range builder `r`, with types specific to that flavor —
  they are not (and need not be) the shared LSP fixtures.
- `src/server/tests.rs` — the W2 scaffolding: `spawn_wire_server`,
  `RawClient`, `EchoServer` / `GatedServer` / `PanickingServer`,
  `bounded`.
- `tests/lsp_wire.rs` — its own minimal framing client: integration
  tests cannot reach `#[cfg(test)]` modules inside the library, so the
  duplication is accepted for now, not endorsed — splitting the
  wire-test files is the scheduled post-cycle brainstorm (see the
  matching deferred entries in `.dupes-ignore.toml`).

## Conventions

- Tests live inline per module, or in a sibling `tests.rs` for larger
  modules — never a stray file.
- Real temp workspaces on disk with millisecond-unique names under
  `std::env::temp_dir()`, created through `temp_workspace(prefix, name)`;
  the prefix names the calling test module so a leaked directory can be
  attributed to its file.
- Determinism: channel gates, never sleeps. Every cross-task await is
  bounded by `tokio::time::timeout` (`WIRE_TIMEOUT`, five seconds in
  both wire files) — futures-rs has no timer, so the bound rides the
  `time` feature of the tokio dev-dependency. `processId: null` in test
  `initialize` keeps `ClientProcessMonitorLayer` inert; shutdown asserts
  the expected EOF instead of hanging.
- All three feature configurations must compile and pass. Keep shared
  harness code free of tree-sitter API; a test that needs the feature
  gates itself with `#[cfg(feature = "tree-sitter")]`.
- `expect`/`unwrap` are allowed in tests (`allow-unwrap-in-tests` and
  `allow-expect-in-tests` in `clippy.toml`). Production `src/` is
  `unwrap`/`expect`-clean outside the one blessed invariant — the
  has-tree `expect` in `src/server/state/documents.rs` — and its
  remaining panicking paths are documented invariants under
  `error-handling.md`.

## The duplication gate

`cargo dupes check` is a gate, not a report: `dupes.toml` pins
`max_exact_duplicates = 0` and `max_near_duplicates = 0`, and tests sit
inside the analysis (owner call: tests are code). There is no
`exclude_tests` knob and none is to be added. Deliberate parallelism —
spec-matrix rows, mirror pairs, the two wire-test clients — carries one
reasoned entry per group in `.dupes-ignore.toml`; a NEW unignored group
must fail the check, and thresholds are never loosened to hide one. The
command runs on demand or periodically, outside the per-task battery
(see `tech.md`).

## Adding a test for a new `Server` method

The method already follows the three-place pattern (`structure.md`):
trait method, `Request` impl in its own file under `src/requests/`, one
`implement_methods!` line. The `Request` impl uses the shared macros for
the common shapes — `request_extract_url!` (document URL at a field
path) and `request_modify_params_position!` (one incoming position at a
field path) — hand-writing hooks only for response-shaped or
multi-position methods.

Testing adds one piece: a W0 conversion test in the `#[cfg(test)] mod
tests` block next to the `Request` impl, importing fixtures from
`crate::testing` (`state_with_documents` is the standard UTF-16 fixture).
Dispatch needs nothing new: the parametrized W2 unknown-method test
(`unwired_methods_return_method_not_found`) pins `-32601` for every
method the crate does not wire, so surface growth adds no wire tests.

Wire-note: an unwired method answers `-32601` only when its params
deserialize — the router validates params before dispatch, so garbage
params fail earlier with `-32602`. The parametrized test sends minimally
valid params per method for exactly that reason.

Known ceiling of the echo round-trip tests (#2 and #6 in the catalog:
`utf16_positions_round_trip_through_real_serialization` and
`incremental_did_change_applies_over_the_wire`): an echo server that
returns the position it received cannot distinguish "conversion works"
from "conversion was deleted" — both are fixpoints on the sent column.
They do fail under either single-direction regression; if a stronger pin
is ever needed, an asserting server that fails unless the handler sees
the UTF-8 byte column breaks the symmetry.

---
_Tests pin only what types cannot express, at the lowest tier that can
see it, on the shared harness — and the duplication gate keeps the
harness itself honest._
