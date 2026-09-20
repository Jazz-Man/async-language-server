# Testing

This rule is normative for all test work in this crate. The suite runs on
rstest 0.27 (2026-09 migration; design
`docs/superpowers/specs/2026-09-19-rstest-migration-design.md`, spike findings
`docs/superpowers/research/2026-09-19-rstest-integration.md` §Q2, verdicts and
audits in `.superpowers/sdd/task-09b-report.md` and `task-09c-report.md`).
Test-catalog numbering per
`docs/superpowers/research/2026-08-31-testing-strategy.md` §4.3.

## Philosophy

Type first, test second. Before writing a test, ask whether a type can
remove the invalid state it would pin — `RangeError` made `RangeExt`
fallible and deleted its `# Panics` contracts; `QueryError` on
`Document::query` is the same criterion. A test exists only for behavior
no type can express; there are no tests for quantity or coverage
statistics. The typing criterion: a type must remove a representable
invalid state or separate a genuinely confusable pair — otherwise it is
ceremony.

## Survey before you write

The project-wide survey-first principle (`~/.claude/rules/principles.md`)
lands in test work as the cure for this suite's duplication debt:
task-scoped agents wrote tests for behavior a neighbor already pinned —
same unit, same inputs, same outcome under different names, another
file, or appended at the end of this one.

Before planning a test — and before planning the feature work that will
ask for one — survey what exists:

1. Find every test that already touches the production unit: LSP
   references, plus identifier greps (macro-emitted callers live in
   `macros/src/`, invisible to reference tools). Read them.
2. Name the neighbors in the plan or brief: which existing tests pin
   adjacent behavior, which fixture already builds the state you need.
   Controllers put this in every SDD test brief; implementers hold the
   same duty for any test they touch.
3. Extend before you add: a new `#[case]` row in the family table beats
   a new fn; an injected fixture beats re-derived setup; the lowest tier
   that can see the behavior beats a new file.
4. Same unit + same inputs + same asserted outcome = no new oracle, no
   new test — regardless of renamed variables, file, or placement.

The dupes gate is a floor, not permission. Reshaping a body so
`cargo dupes` stops matching it — renamed locals, reshuffled asserts,
spurious bindings — is a workaround in the `no-workarounds` sense: if
two tests are one oracle, delete one; if they pin different axes, name
the mutator each kills — that name is the difference that justifies
both.

## The two tiers

| tier | where | what it pins |
|---|---|---|
| W0 unit | inline `#[cfg(test)] mod tests` / sibling `tests.rs` | arithmetic, conversion math, state machines, `Request` conversion hooks |
| wire | `src/server/tests/` | framing + serde + the real middleware stack (`serve::run_over_streams`) over `tokio::io::duplex`, driven by a raw JSON-RPC client |

Choose the lowest tier that can express the assertion. The wire tier exists
only for what unit tests cannot see: lifecycle gating, staleness retry,
panic mapping, the concurrency bound, termination, wire encoding. The
concurrency test (`at_most_limit_requests_run_concurrently`,
`src/server/tests/robustness.rs`) pins bound and recovery: at most
`available_parallelism()` handlers run at once and the overflow handler
enters and completes — the dependency is git-pinned to async-lsp's PR #30
fix (`Cargo.toml`, matching `allow-git` in `deny.toml`). If the
overflow-blocked failure reappears, the pin was lost: a crates.io release
without the fix replaced the dependency.

## The rstest shape

Every handwritten test fn runs under `#[rstest]` — single-case tests
included: one uniform attribute, fixtures always injectable — with
`#[tokio::test]` below it where the test is async. No plain-`#[test]`
emission remains: the D5 exemption (the `conversion_tests!` tables) is
closed — the 2026-09-20 emission rework
(`docs/superpowers/specs/2026-09-20-conversion-tables-migration-design.md`)
stamps each table as one `#[rstest]` fn whose rows are data-only
`#[case::old_name]`s; the macro is the table harness, and its emitted fns
are not handwritten tests. Stamped tables are self-contained: the fixture
rides the emission (`#[from(crate::testing::utf16_state)]`), so an
invoking module needs no fixture import — the macro alone injects it
(row-expression builders still import normally).

1. Import `use rstest::{fixture, rstest};` only — never `use rstest::*`
   (`wildcard_imports` is pedantic-deny). `#[case]`, `#[values]`,
   `#[future]`, `#[with]`, `#[default]` are attribute tokens the `#[rstest]`
   macro consumes; never import them.
2. Keep all rstest code in `src/` `#[cfg(test)]` modules; never create a
   `tests/` target (`tests/architecture.rs`, the arch-lint self-test, is
   the one standing exception). arch-lint scans `tests/` as production
   code (`std::fs` there trips `no-sync-io`), and clippy's test context
   is syntactic — a `#[fixture]` outside `#[cfg(test)]` gets no
   `allow-expect-in-tests`.
3. Name every case row descriptively (`#[case::end_relative_boundary]`);
   bare `case_N` is forbidden. Generated names — `case_N_name` for named
   rows, `{arg}_{i}_{value}` for `#[values]` — feed nextest selectors, the
   mutants disposition table, and dupes legibility. When a `#[values]`
   value does not slug readably (URLs, globs, whitespace), use named rows
   or 0.27's doc-comment name override.
4. Gate a single row with `#[cfg_attr(feature = "tree-sitter",
   case::gated(...))]`; a gated test keeps `#[cfg(feature)]` on the fn.
   Both battery legs must compile and pass; keep shared harness code free
   of tree-sitter API.
5. Reference only real paths in doc comments — dylint
   `nonexistent-path-in-comment` denies stale ones.
6. Select with `-E`: `cargo nextest run -E 'binary(async_language_server)'`
   scopes to the lib target, `-E 'test(wired_methods)'` matches a fn and
   all its case rows. A plain name substring still works within the lib
   binary; it never matches an integration target's name.

## Rows and bodies

- Make a family `#[case]` rows only when members differ in data, not
  orchestration — a different await/setup flow is a separate test, not a
  row.
- Unfold an internal fresh-state loop over data into named rows; a
  same-state sequential contrast (write, assert, then mutate) stays a body.
- Reserve `#[values]` for one-axis sweeps whose oracle is invariant across
  the values and whose slugs read — the two adopted sites are the
  `refresh_support` and `advertised` bools in `src/workspace/diagnostics.rs`.
  A matrix only when every combination is a distinct oracle; multi-column
  tables and per-value heterogeneous expectations keep named rows.
- Keep tests that mutate files mid-test (stamp/watch/watcher families)
  single tests — rows carry data, not sequences.
- Delete a migrating family member only when no mutator class
  distinguishes it (the mutation oracle); textual similarity is never the
  criterion, count deltas are renames only.

## The harness

Two homes by tier, both `#[cfg(test)] pub(crate)`. All temp-disk work goes
through the `TempWorkspace` Drop-guard in `src/testing.rs`: inject it as
`#[with("walker")] workspace: TempWorkspace` (the prefix names the owning
module, so a leaked directory is attributable); `write(rel, text) -> Url`
does write + canonicalize + URL in one call and creates parent dirs;
`Drop` removes the tree — also on unwind. Directory names are
pid+seq+millisecond-unique under `std::env::temp_dir()`; in the suite's
`#[cfg(test)]` code, manual `fs::write`/`canonicalize`/
`fs::remove_dir_all` tails exist nowhere else (doctests do their own
temp dirs — they cannot see the `pub(crate)` guard).

Crate-tier fixtures: `workspace` (the guard); `state()` — a matcher-less
`TestServer` state, compose `test_document_matchers`,
`extension_matchers`, or feature-gated `json_matchers` explicitly; and
`utf16_state()`, the canonical UTF-16 conversion fixture wrapping
`state_with_documents` (the `"🙂abc"` document is load-bearing: U+1F642 is
4 UTF-8 bytes but 2 UTF-16 units, so byte offset 4 == UTF-16 offset 2).
`seed_workspace(&TempWorkspace) -> SeededWorkspace` is a plain async
helper, not a fixture: the refresh must run after the test's writes, and
fixtures resolve before the body — the same sequencing rule that keeps
wire spawn explicit. Its matcher contract is `*.test`-only; seeding
another extension yields an empty `urls`. Position/token/URL builders
stay plain fns (`url` is called mid-test, not at injection time).

The wire tier stays plain fns (D3: 17 of 26 spawn sites were bespoke
servers — a default fixture would chase handshake variations):
`spawn_wire_server`, `RawClient`, `EchoServer`, `bounded`, `WIRE_TIMEOUT`
in `src/server/testing.rs`; `GatedServer` (shared robustness→staleness)
and `PanickingServer` (local to `robustness.rs`) stay in
`src/server/tests/`; a file whose tests share a handshake tail wraps it in a
local plain async helper (`spawn_initialized` in
`src/server/tests/workspace_diagnostics.rs`). Wire tests run under
`#[rstest]` + `#[tokio::test]` and inject crate-tier fixtures freely.

Injection mechanics: `#[with(...)]` resolves by parameter name, so an
injected fixture parameter must be named `workspace`; one fixture cannot
be injected twice by name — for a second root call the fixture as a
function: `workspace("requests")`.

`#[fixture]` emits `#[allow(dead_code)]`, so liveness never surfaces in
the lint — sweep by reference count at cycle ends. Beware the
macro-emitted-caller trap: `prime_conversion_fallback`,
`warn_once_default`, `document_version`, `sole_document`,
`dispatch_allowed`, `utf16_state`, and
`assert_converted_position` have keeping callers in macro-emitted code
(`macros/src/` — the stamped tables' `#[from(crate::testing::utf16_state)]`
is `utf16_state`'s only consumer outside src/); src/-only sweeps misjudge
them. `expect`/`unwrap` are
allowed in tests (`clippy.toml`); production `src/` stays clean outside
the one blessed `expect` in `src/server/state/documents.rs`.

## rstest surface, adjudicated

| capability | verdict |
|---|---|
| `#[case]` named rows | adopted suite-wide |
| macro-stamped tables (`conversion_tests!`) | adopted — one `#[rstest]` case-table per request; data-only `#[case]` rows, fixture via `#[from]` (2026-09-20) |
| `#[values]` | single-axis invariant-oracle sweeps only (2 sites) |
| `#[timeout(...)]` | the three gate-driven wire tests — whole-test failsafe, never an oracle |
| `#[once]` | rejected — every test needs isolated state |
| `#[files]` family | rejected — files resolve at compile time against the checkout; this suite builds runtime temp trees |
| `#[context]` | rejected — no name-dependent logic; elapsed-time measurement is the anti-pattern below |
| `#[by_ref]` | rejected — no cross-argument lifetimes to tune |
| `#[from(...)]` | rejected for handwritten tests — the fixture graph is flat; the macro-stamped tables' emission is the one user (see the macro-stamped tables row) |
| `#[ignore]` | rejected — `#[tokio::test]` injects no arguments |
| `#[trace]`/`#[notrace]` | rejected — named rows and assert output carry the semantics; `Debug` bounds on fixtures buy nothing |
| `#[test_attr(...)]` | rejected — one runtime, nothing to deconflict |
| `#[awt]` / async fixtures | rejected — the sequencing rule keeps `seed_workspace` a helper |
| `rstest_reuse` | rejected — stale crate, `rand`/`getrandom` lock cost, no shared case lists |

Re-adjudicate against the evidence in `task-09b-report.md`, not from
memory.

## Determinism

Channel gates, never sleeps. Every cross-task await is bounded by
`tokio::time::timeout` (`WIRE_TIMEOUT`, five seconds, in the wire
harness). `processId: null` in test `initialize` keeps
`ClientProcessMonitorLayer` inert; shutdown asserts the expected EOF
instead of hanging. Parallel-gate tests fill a bounded pool through
channels or semaphores, bound every wait, and assert on what entered.
`#[timeout(Duration::from_secs(60))]` on the three gate-driven wire tests
(12× `WIRE_TIMEOUT`) caps a wedged gate and names the failure — a
failsafe, not an oracle; no test asserts elapsed time because of it. CI
should set `RSTEST_TIMEOUT=120` on the test steps (recorded
recommendation): the env var is read at compile time, its value in
seconds.

## The duplication gate

`make dupes` is a gate, not a report: `dupes.toml` pins 0 exact / 0 near,
tests included. Deliberate parallelism — spec-matrix rows, mirror pairs —
carries one reasoned entry per group in `.dupes-ignore.toml`; a new
unignored group must fail the check, and thresholds are never loosened to
hide one. At cycle ends run `cargo dupes cleanup` (dry-run first) — it
drops only entries whose GROUP vanished — then audit every surviving
entry by hand: does the group still exist, is the reason still truthful,
is the parallelism still deliberate? Plain `cargo dupes` output APPLIES
the ignore list, so it reports 0 groups while ignored ones exist —
enumerate live groups with the list temporarily set aside and
set-difference fingerprints (2026-09-19 audit: 36 entries ↔ 36 live groups).

## Mutation-driven test design

cargo-mutants (`make mutants`, on demand, never in the battery; scope with
`FILE=src/foo.rs`) is a teacher, not a gate: a survivor means the suite
has no oracle the runner can see for a behavior difference (doctest kills
are invisible), never that the code is wrong. Read each survivor through
its mutator class — the class names the dimension the missing test forgot:

| mutator class | design dimension it demands |
|---|---|
| range boundary shift | boundary values one step past each edge, not just interior samples |
| comparison flip | one sample beyond the equivalence class, on both sides |
| removed call | every side effect carries at least one observable assertion |
| literal replacement | degenerate and neutral-element cases (`0`, `1`, `""`) |
| removed `?` / replaced return | assert the error surfaces — the failing case itself, never `is_ok()` on the happy path |

Every survivor lands in one of four baskets — Philosophy's type-first
question applies before all four — and only the first writes a test:

| basket | action |
|---|---|
| business gap | write the test, shaped by the mutator-class row above |
| equivalent mutant | ratify a disposition-table row (owner approval, never a silent skip) |
| dead logic | propose deleting the code instead of testing it |
| test-pleasing | reject — a test whose only statement is "this mutant dies" is not a test |

The disposition table is
`docs/superpowers/plans/2026-09-09-mutants-disposition-table.md`,
maintained by reconciliation: a survivor absent from the table is new
work; a row whose code no longer exists is dropped. Migrations that
rename tests record old→new name maps and a final pass applies them —
the rstest migration re-pointed 11 rows, dropped none.

## Adding a test for a new `Server` method

The method already follows the three-place pattern (`structure.md`);
testing adds one piece: a W0 conversion test — a `conversion_tests!` table
in the `#[cfg(test)] mod tests` block next to the marker struct in
`src/lsp_requests/`, one row per round-trip (`utf16_state` is injected by
the emission; the module needs no fixture import). Dispatch needs nothing
new: the wire unknown-method test
pins the router default, and `wired_methods_dispatch` fails loudly if a
dispatch row is lost while its fixture still lists the method. Wire-note:
params validation lives inside each registered handler, so an unknown name
answers `-32601` before any deserialization — even garbage params cannot
turn it into `-32602`; the dispatch-row test sends minimally valid params
for exactly that reason.

Known ceiling of the echo round-trip tests (#2 and #6 in the catalog): an
echo server that returns the position it received cannot distinguish
"conversion works" from "conversion was deleted" — both are fixpoints on
the sent column. They fail under either single-direction regression; if a
stronger pin is ever needed, an asserting server that fails unless the
handler sees the UTF-8 byte column breaks the symmetry.

---
_Tests pin only what types cannot express, at the lowest tier that can see
it — under `#[rstest]`, on the shared harness, with the gates honest._
