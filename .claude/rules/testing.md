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

## The two tiers

| tier | where | what it pins |
|---|---|---|
| W0 unit | inline `#[cfg(test)] mod tests` / sibling `tests.rs` | arithmetic, conversion math, state machines, `Request` conversion hooks |
| wire | `src/server/tests/` | framing + serde + the real middleware stack (`serve::run_over_streams`) over `tokio::io::duplex` through the internal seam, driven by a raw JSON-RPC client |

Choose the lowest tier that can express the assertion. The wire tier
exists only for what unit tests cannot see: lifecycle gating, staleness
retry, panic mapping, the concurrency bound, termination, wire encoding.

The concurrency test (`at_most_limit_requests_run_concurrently`, in
`src/server/tests/robustness.rs`) pins both the bound and the recovery:
at most `available_parallelism()` handlers run at once, and — since the
dependency pins async-lsp's PR #30 fix (drive in-flight tasks while
waiting for `poll_ready`, oxalica/async-lsp#30; git-pinned to the fix
branch in `Cargo.toml`, with the matching `allow-git` in `deny.toml`) —
the overflow handler enters and every response arrives once the gates
release. History: async-lsp 0.2.4 from crates.io deadlocked here (the
overflow never proceeded and the server task had to be aborted); the
absence-check failure after swapping the dependency was the flip signal,
executed 2026-09-18. If the overflow-blocked failure ever reappears, the
git pin was lost — a crates.io release without the fix replaced the
dependency.

## Harness inventory

- `crate::testing` (`src/testing.rs` — a `#[cfg(test)] pub(crate)` module
  declared in `src/lib.rs`, scopeless like `src/error.rs`) — the single
  shared home for fixtures: `line_position`, `line_range`, `same_line`
  (LSP positions and ranges), `token` (a `SemanticToken` from relative
  columns, type and modifiers zero), `url`, `TestServer`, `open_document`,
  `state_with_documents`, `temp_workspace(prefix, name)`,
  `workspace_folder`, `diagnostic`, and `json_matchers`
  (tree-sitter-gated). The `"🙂abc"` document and the UTF-16 encoding in
  `state_with_documents` are load-bearing: U+1F642 is 4 UTF-8 bytes but
  2 UTF-16 units, so byte offset 4 == UTF-16 offset 2. The byte and
  tree-sitter `r()` twins stay local in their own test files: each flavor
  names its local range builder `r`, with types specific to that flavor —
  they are not (and need not be) the shared LSP fixtures.
- `src/server/testing.rs` — the wire scaffolding: `spawn_wire_server`,
  `RawClient`, `EchoServer`, `bounded`; the server halves of the duplex
  cross to the futures traits through `tokio-util`'s `compat`
  (dev-dependency). `GatedServer` / `PanickingServer` stay local to
  `src/server/tests/robustness.rs`, their only consumers.

## Conventions

- Tests live inline per module, or in a sibling `tests.rs` for larger
  modules — never a stray file.
- Real temp workspaces on disk with millisecond-unique names under
  `std::env::temp_dir()`, created through `temp_workspace(prefix, name)`;
  the prefix names the calling test module so a leaked directory can be
  attributed to its file.
- Determinism: channel gates, never sleeps. Every cross-task await is
  bounded by `tokio::time::timeout` (`WIRE_TIMEOUT`, five seconds in
  the wire harness) — futures-rs has no timer, so the bound rides the
  `time` feature of the tokio dev-dependency. `processId: null` in test
  `initialize` keeps `ClientProcessMonitorLayer` inert; shutdown asserts
  the expected EOF instead of hanging. Parallel-gate tests use the same
  grammar — fill a bounded pool through channels or semaphores, bound
  every wait, assert on what entered — never on elapsed time
  (`for_each_bounded`'s width test, the wire concurrency tripwire).
- Every feature configuration CI runs must compile and pass — `--all-features` and `--no-default-features` (plus `default` again once a non-default feature exists). Keep shared
  harness code free of tree-sitter API; a test that needs the feature
  gates itself with `#[cfg(feature = "tree-sitter")]`.
- `expect`/`unwrap` are allowed in tests (`allow-unwrap-in-tests` and
  `allow-expect-in-tests` in `clippy.toml`). Production `src/` is
  `unwrap`/`expect`-clean outside the one blessed invariant — the
  has-tree `expect` in `src/server/state/documents.rs` — and its
  remaining panicking paths are documented invariants under
  `error-handling.md`.

## The duplication gate

`make dupes` is a gate, not a report: `dupes.toml` pins
`max_exact_duplicates = 0` and `max_near_duplicates = 0`, and tests sit
inside the analysis (owner call: tests are code). There is no
`exclude_tests` knob and none is to be added. Deliberate parallelism —
spec-matrix rows, mirror pairs — carries one
reasoned entry per group in `.dupes-ignore.toml`; a NEW unignored group
must fail the check, and thresholds are never loosened to hide one. List
maintenance has exactly one automatic step: at cycle ends, run
`cargo dupes cleanup`, which removes entries whose groups no longer
exist — dry-run first (`--dry-run` lists the stale entries; the plain
invocation drops them). Dissolving a group that still exists is
refactoring; keeping every reason truthful is human/agent review. The
tool does neither. The
command runs on demand or periodically, outside the per-task battery
(see `tech.md`). The criterion bench (`make bench`) also runs on demand, outside the battery.

## Mutation-driven test design

cargo-mutants (`make mutants`, on demand — never in the battery; `tech.md`
owns the invocation constraints) is a teacher, not a gate to satisfy: a
survivor means the suite has no oracle the mutants runner can see for a
behavior difference (doctest kills are invisible to the runner), never
that the code is wrong. Read each survivor through its mutator class —
the class names the test-design dimension the missing test forgot:

| mutator class | design dimension it demands |
|---|---|
| range boundary shift | boundary values one step past each edge, not just interior samples |
| comparison flip | one sample beyond the equivalence class, on both sides |
| removed call | every side effect of the call carries at least one observable assertion |
| literal replacement | degenerate and neutral-element cases (`0`, `1`, `""`) |
| removed `?` / replaced return | assert the error surfaces — the failing case itself, never `is_ok()` on the happy path |

Every survivor lands in one of four baskets — Philosophy's type-first
question applies before all four — and only the first basket writes a
test:

| basket | action |
|---|---|
| business gap | write the test, shaped by the mutator-class row above |
| equivalent mutant | ratify a disposition-table row (owner approval, never a silent skip) |
| dead logic | propose deleting the code instead of testing it |
| test-pleasing | reject — a test whose only statement is "this mutant dies" is not a test |

The survivor database is the disposition table
(`docs/superpowers/plans/2026-09-09-mutants-disposition-table.md`),
maintained by reconciliation: a survivor absent from the table is new
work; a row whose code no longer exists is dropped. Never write a test
whose only purpose is the kill — if the only behavior it can state is
the mutant's death, the mutant was equivalent or the logic dead, and
the basket above says so.

Scoped runs beat full ones: `make mutants FILE=src/foo.rs` over the file
just touched, a re-run over the diff's files after a refactor, the full
sweep only at cycle acceptance points.

## Adding a test for a new `Server` method

The method already follows the three-place pattern (`structure.md`):
the `#[lsp_request]` struct in its own file under `src/lsp_requests/`, the
`lsp_method!`/`lsp_resolve_method!` block for the trait method, one
`lsp_dispatch!` row. The `#[lsp_request]` attribute fields cover the
common shapes (`document(...)`, `incoming_position(...)`); hand-write
hooks only for response-shaped or multi-position methods, as free
`convert_*` fns wired through `incoming_custom`/`outgoing`.

Testing adds one piece: a W0 conversion test — `conversion_tests!` rows
in the `#[cfg(test)] mod tests` block next to the marker struct —
importing fixtures from `crate::testing` (`state_with_documents` is the
standard UTF-16 fixture).
Dispatch needs nothing new: the wire unknown-method test
(`unknown_methods_answer_method_not_found`) pins the router default —
`-32601` under a method name no handler is registered for. Every
client-to-server request `lsp_types` defines IS registered (the 48 dispatch rows,
`workspace/diagnostic` and `initialize` by the wrapper, `shutdown` by
the trait default), so only a synthetic name outside `lsp_types`
reaches that reply; surface growth adds no wire tests, and a dispatch
row lost while the fixture still lists its method fails
`wired_methods_dispatch` loudly.

Wire-note: params validation lives inside each registered handler, so
an unknown name answers `-32601` before any deserialization — even
garbage params cannot turn it into `-32602`. A registered method with
garbage params, by contrast, fails with `-32602` inside its handler and
never reaches the engine; the dispatch-row test sends minimally valid
params for exactly that reason.

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
