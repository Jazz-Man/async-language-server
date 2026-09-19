# rstest integration — toolchain verification, `conversion_tests!` merit, and fixture-design input

*Research doc, 2026-09-19. Feeds the rstest-migration spec; the migration decision itself is
owner-ratified (rstest 0.27.0 already a dev-dependency). Baseline facts from the brief are
restated, not re-derived: 207 `#[test]` + 62 `#[tokio::test]` across 12 test files/harnesses,
29 `conversion_tests!` occurrences, 107 `fs::write` sites; rstest 0.27.0 MSRV 1.85 / edition
2021 / MIT OR Apache-2.0; `Cargo.lock` gains no duplicate-version groups from rstest itself.*

Method: one temporary integration-test spike (`tests/rstest_spike.rs`, deleted after
verification — `git status` clean at the end, confirmed), run through the exact CI ladder;
crates.io registry API + GitHub for ecosystem evidence; direct reads of `macros/src/conversion_tests.rs`,
`src/testing.rs`, `src/server/testing.rs`, and sample request test modules. Research only —
no commits, no branch changes.

---

## Q1 — `rstest_reuse`: verdict **unnecessary for a suite our size; avoid adopting now**

Registry facts (crates.io API, `api/v1/crates/rstest_reuse`, retrieved 2026-09-19):

| fact | value |
|---|---|
| latest release | **0.7.0, published 2024-05-30** (10 versions total; nothing since) |
| declared MSRV of 0.7.0 | `rust_version = "1.60.0"`, edition 2021 |
| license | MIT OR Apache-2.0 |
| downloads | 13.6M total, ~2.97M recent |
| normal dependencies | `quote ^1.0.9`, `syn ^2.0.2` (features `full`, `extra-traits`), **`rand ^0.8.5`** |

Maintenance is alive in-tree but unreleased: commits touching `rstest_reuse/` include
"Bump msrv to 1.85 also for rstest_reuse (#342)" (2026-03-26) and the `#[hidden]`
docs feature touching `rstest_reuse/src/lib.rs` (2026-01-09) — so the repo master supports
rstest 0.27-era code, but **no release carrying it has shipped**. rstest 0.27.0's own
changelog (release v0.27.0, 2026-09-06) says "Bump msrv to 1.85.0 both for `rstest` and
`rstest_reuse`" — again describing master, not the published crate.

Does the project still recommend it? Yes — the rstest README (master, retrieved 2026-09-19)
still documents `rstest_reuse` as *the* way to share a case list across tests
("[`#] Use Parametrize definition in more tests") and notes its "Feature flagged cases"
pattern "also works with `rstest_reuse`". There is no native replacement: `#[rstest]`
has no built-in template mechanism.

Cost of adopting, concretely for this lock: `rand ^0.8.5` (which pulls `getrandom`) is a
**normal** dependency of rstest_reuse 0.7.0, and our `Cargo.lock` contains neither `rand`
nor `getrandom` today (verified by grep) — adoption would mint a brand-new duplicate-version
group for cargo-deny to adjudicate, for a crate last published 2.5 years ago.

Verdict rationale: sharing a parametrized table across *different test functions* is a
real need only for suites with repeated case lists. Ours has none today — the 29
`conversion_tests!` tables are per-request-type (each table tests one `Request`), and
rstest fixtures already give cross-test setup reuse. If the need appears later, adopt
then, watching the release situation.

## Q2 — empirical toolchain spike: **all five legs green; six findings, all recorded**

Spike: `tests/rstest_spike.rs`, an integration-test target using only `rstest`, `tokio`,
and `std` (final form after three honest iterations — see findings F2, F3, F6). Shapes:
`#[fixture]` returning a Drop-typed value, `#[case::named]` rows, `#[values]`, an async
`#[fixture]` consumed via `#[future]` under `#[rstest]`+`#[tokio::test]`, and a
`#[cfg_attr(feature = "tree-sitter", case::gated(20))]` row.

### Verification ladder (commands + results)

| leg | command | result |
|---|---|---|
| run + names | `cargo nextest run -E 'binary(rstest_spike)'` | **9/9 PASS** |
| names, default features | `cargo nextest list` | 9 tests (table below) |
| names, no default features | `cargo nextest list --no-default-features` | 8 tests — `case_2_gated` **absent**, others unchanged |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **clean** (pedantic deny held over all generated fns) |
| fmt | `cargo fmt --check` | **clean** |
| no-default leg | `cargo nextest run --workspace --no-default-features` | **292/292 PASS** (incl. arch-lint test) |
| dylint | `make dylint` (pinned `nightly-2026-05-28`, drivers cached) | **clean** — the perfectionist-on-case-rows question is **verified green**, not assumed |

### Exact generated test names (feed for the mutants disposition table and dupes analysis)

Default features (`tree-sitter` on):

```
async_fixture_via_future
feature_gated_case::case_1_always
feature_gated_case::case_2_gated
fixture_injection
named_cases::case_1_zero
named_cases::case_2_one
values_rows::n_1_1
values_rows::n_2_2
values_rows::n_3_10
```

Rules observed: a named `#[case::foo]` becomes **`case_N_foo`** (index kept, name appended);
unnamed cases are `case_N`; `#[values]` rows are **`n_M_<slug-of-value>`** (`n_3_10` for
value `10`); plain (non-parametrized) tests keep their exact name. Under
`--no-default-features` the `cfg_attr` row vanishes cleanly and nothing else moves.

### Findings

- **F1 — imports.** rstest 0.27 exports exactly `rstest`, `fixture`, `Context` (plus
  `#[doc(hidden)]` modules; verified in the registry source). `#[case]`, `#[values]`,
  `#[future]`, `#[with]`, `#[default]` etc. are **never imported** — the enclosing
  `#[rstest]` proc macro consumes them as inert attribute tokens (the README's own example
  uses bare `use rstest::rstest;` with `#[case]`). The README's `use rstest::*;` style
  must NOT be copied here: `wildcard_imports` is pedantic-deny in this workspace.
  Migration idiom: `use rstest::{fixture, rstest};`.
- **F2 — clippy's `allow-expect-in-tests` is syntactic.** First spike iteration used
  `.expect(...)` inside the `#[fixture]` body of a `tests/` target → `expect_used`
  denied, because clippy recognizes test context via `#[test]` fns / `#[cfg(test)]`
  scope, and a `#[fixture]` fn is neither. Inside `src/` `#[cfg(test)]` modules (where the
  migration lives) fixtures are covered by the cfg and the config holds. Setup code that
  must fail loudly outside that context panics explicitly.
- **F3 — arch-lint scans `tests/` as production code.** First iteration's `std::fs`
  calls in the `tests/` target tripped `no-sync-io [AL002]` in
  `architecture_rules_hold` (2 violations; the scan covers `tests/`, 108 files). Any
  future integration-test target is production-linted; **the migration must stay in
  `src/` `#[cfg(test)]` modules** — which is also the testing rule's layout mandate.
- **F4 — nextest filter semantics.** Integration-binary test ids do not carry the target
  name: `cargo nextest run rstest_spike` (plain filter) matched **0 tests**; the correct
  selectors are `-E 'binary(rstest_spike)'` (target) or the fn/case name
  (`-E 'test(named_cases)'`, which matches all nested cases). Suite-wide search-and-run
  workflows and any scripts must switch to `-E` expressions when case rows appear.
- **F5 — attribute stacking works as documented.** `#[rstest]` on top, `#[tokio::test]`
  below (the runtime attribute lands on each generated case fn); async `#[fixture]` +
  `#[future]` parameter; `#[cfg_attr(feature = …, case::named(...))]` all compile and
  run on stable 1.98 (pinned) and under the dylint nightly.
- **F6 — lint suites watch comments too.** `nonexistent-path-in-comment` (Trail of Bits
  supplementary, dylint) denied a doc comment referencing this research doc before the
  doc existed. Cosmetic, but migration PRs referencing files in doc comments must point
  at real paths.

## Q3 — `conversion_tests!` merit analysis: **keep plain `#[test]` emission (option b)**

Mechanics today (`macros/src/conversion_tests.rs`): the proc macro parses rows
`name : RequestType { params: expr, incoming/expects?, response/outgoing/returns? }` and
stamps **one `#[test] fn` per row** whose body is a fixed script — `state_with_documents()`,
`document(&emoji)` expect, `(#params)(emoji.clone())`, `modify_params`, optional
`assert_converted_position` halves. Emitted code uses call-site `crate::` paths; the
closures in rows rely on contextual type inference (`|uri| HoverParams { … }`).

(a) Feasibility of emitting `#[rstest]` + `#[case]`: mechanically possible — the macro
could emit a single generic `#[rstest]` fn (`R: Request`) with one `#[case::name]` per
row carrying (params-builder, extractor, expected, optional response triple). But:

1. **What rstest buys here is zero.** `#[case]` parameterizes *one body over runtime
   values*; these rows are *compile-time-shaped*: each row carries a different `Request`
   type and different closure sets, and the full/incoming-only halves differ structurally.
   The shared body already lives in the macro template — rstest would re-earn exactly
   that sharing, one layer down, with more machinery.
2. **Closure inference dies in case expressions.** Case values are free-standing
   expressions; the rows' `|uri| …` closures lose their contextual signatures and need
   full annotations on all 29 tables.
3. **Name stability breaks everywhere at once.** Every stamped test name
   (`hover_incoming_utf16_becomes_utf8`, `references_round_trips_both_directions`, …)
   would become nested (`<fn>::case_N_name`), churning the mutants disposition table,
   `nextest` filters, and IDE/CI discovery for 29 tables in one commit — with the
   migration mandate elsewhere requiring 100% of *handwritten* tests anyway.

(b) Keep plain `#[test]` emission with a reasoned exemption: the macro is not
test-quantity duplication — it is the type-safe table harness the testing rule describes;
its rows share the body by construction, and migrating its *emission* buys no fixture
reuse, no matrix, and no naming benefit while paying (2) and (3). The exemption text for
the rules rework: *"rows stamped by `conversion_tests!` keep plain `#[test]` emission —
the macro is the table harness; its per-row fns are not handwritten tests."* Note the
macro's own seven handwritten unit tests (in `macros/src/conversion_tests.rs` `mod tests`)
are ordinary tests and migrate like any others.

## Q4 — field evidence: adoption is massive and unremarkable; pitfalls are known and shallow

Scale (crates.io reverse-dependencies API, 2026-09-19): **3281 reverse dependencies**
total; the top-dependent crates list includes `ciborium` (185M downloads), `time`
(26.6M), `base64` 0.23 (17.2M), `comfy-table` (494K), `convert_case` (507K), `redis`
(329K), `smawk` — and the dominant version requirement is `^0.26.1`, i.e. dependents
track current majors quickly (dev-dependency kind throughout).

GitHub code search (index known-incomplete; several queries returned
`incomplete_results: true`, so absence of hits is not evidence of absence):

- `time-rs/time`: **39 files** using rstest, including the fixture-driven integration
  harness (`tests/integration/ext.rs`, `tests/integration/time.rs`) — a
  fast-moving, MSRV-strict crate betting its test suite on rstest.
- `Nukesor/comfy-table`: rstest in `Cargo.toml` and `tests/all/constraints_test.rs`.
- `redis-rs` depends on rstest per crates.io (code-search index returned nothing usable).

Endorsed patterns (rstest README, master): fixtures for dependency injection; named
`#[case]` rows; `#[values]` lists; `#[with]`/`#[default]` fixture overrides; `#[once]`
shared fixtures; `#[timeout]` with the default-on `async-timeout` feature and
`RSTEST_TIMEOUT` env; `#[cfg_attr(feature = …, case(…))]` for feature-flagged cases
(documented, spike-verified).

Reported pitfalls, with our exposure:

1. **Case blowup** — `#[case]`×`#[values]` generate the cartesian product (README's own
   example: 2 cases × 3 values = 6 tests). Exposure: low; our matrices are small, but the
   spec should cap matrix width by convention.
2. **Value-name slugification** — names "try to convert the input expression in a Rust
   valid identifier": `"     "` becomes `query_1_____` (README example). rstest 0.27
   added doc-comment overrides for generated matrix names (release notes, PR #321 by
   orhun) — the sanctioned escape hatch. Exposure: medium for `#[values]` over strings
   (URLs, globs); the spec should prefer named `#[case]` rows where the value doesn't
   slug readably.
3. **Discovery/CI** — nextest lists rstest nests exactly as `cargo test` does
   (spike-verified); rust-analyzer runs rstest tests via macro expansion. [Inference,
   based on observed patterns: generated-code expansion is standard RA behavior; no
   rstest-specific RA breakage observed in this spike.]
4. **Fixture anti-patterns** — `#[once]` shared mutable state across tests breaks
   isolation (README: `#[once]` returns a shared reference); fixtures returning guards
   must not be `Copy`-able or `Clone`-shared, or cleanup fires early. Exposure: our
   fixtures are per-test by design.

## Q5 — fixture-design input (survey + concrete sketches)

Setup shapes across the 12 async/wire harnesses and the W0 modules:

| shape | where today | count/form |
|---|---|---|
| `temp_workspace(prefix, name)` + N × `fs::write` | state, with_state, walker, diagnostics, oneshot | 107 `fs::write` sites; **no cleanup — directories leak by design** (prefix = attributing module) |
| `state_with_documents()` → `(ServerState, source, target)` | W0 conversion tables, state tests | one canonical UTF-16 fixture ("🙂abc" document) |
| `advertise_workspace_diagnostics(&state)` | capability-gated tests | seed advertisement without `initialize` |
| `allow_all_methods(&mut state)` | dispatch tests | open the capability gate |
| `spawn_wire_server(server)` → `(RawClient, JoinHandle)`; `RawClient` methods; `bounded()` | `src/server/tests/*` | wire tier; `EchoServer`/`GatedServer`/`PanickingServer` per-test |

Sketches for the spec (all in `src/testing.rs` / `src/server/testing.rs`, under
`#[cfg(test)]` — F2/F3 constrain them there):

```rust
// 1. Temp-workspace Drop guard — cleanup on pass AND fail (unwind), prefix kept.
pub(crate) struct TempWorkspace { root: PathBuf }        // Drop: fs::remove_dir_all(root)
impl Deref for TempWorkspace { Target = PathBuf }        // let file = ws.join("a.test");

#[fixture]
fn workspace() -> TempWorkspace { TempWorkspace::new("ws") }             // unique name inside

#[rstest]
fn walker_is_deterministic(#[with("walker")] workspace: TempWorkspace) { … }
// `#[with(prefix)]` overrides the attribution prefix per module; name stays
// millisecond-unique inside. `#[default]` fixture args can take the prefix too.

// 2. Seeded-state family — the canonical fixture plus with-overrides.
#[fixture]
fn utf16_state() -> (ServerState, Url, Url) { state_with_documents() }   // existing fn, unchanged

#[fixture]
fn gated_state() -> ServerState { … advertise_workspace_diagnostics … }

// 3. Wire tier: keep spawn_wire_server/RawClient/EchoServer as plain fns.
//    Reasons: the server impl is a per-test generic (S: Server) a fixture cannot carry;
//    RawClient is driven as &mut across ordered awaits with no reuse across tests;
//    rstest would add attribute ceremony to zero setup duplication.
//    Only `bounded()` stays a plain async helper — it wraps an expression, not setup.
```

Feature notes: `#[with]` (fixture override args) is the one rstest feature this design
needs beyond basics; `#[default]` works for fixture-argument defaults if the prefix is
lifted to a parameter; async `#[future]` fixtures buy nothing here (the wire harness
spawns synchronously) — avoid; `#[once]` buys nothing (every test needs isolated state;
no expensive read-only shared data exists) — avoid. Timeout (`#[timeout]`) is available
if the spec wants per-test bounds, but the suite's determinism rule (channel gates, never
sleeps) stays the primary mechanism; a global `RSTEST_TIMEOUT` in CI is the cheap safety
net if wanted.

## Risks, ranked

1. **Name churn vs. the disposition table** (highest operational risk): migrating the
   269 handwritten test functions (207 `#[test]` + 62 `#[tokio::test]`) — the ones that
   become case rows get rstest names (`case_N_name`, `n_M_slug`); the
   mutants disposition table, any `nextest` filter strings, and dupes groups must move in
   lockstep. Mitigations, both spike-verified: named `#[case]` rows keep the human name
   (`case_1_zero`), and 0.27's doc-comment overrides can pin matrix names.
2. **Filter-semantics drift**: plain `cargo nextest run <substr>` no longer matches by
   target/binary the way muscle memory expects (F4); `-E` expressions are the durable
   selector.
3. **Lint-boundary surprises** (F2, F3, F6): fixtures only in `src/` `#[cfg(test)]`;
   no new `tests/` targets; no `use rstest::*` (F1); doc-comment paths must exist.
4. **`conversion_tests!` regression risk if reworked** — moot under the Q3 (b)
   recommendation; if the owner overrides toward (a), the cost is the F3-grade lockstep
   rename of 29 tables.
5. **`rstest_reuse` staleness** — avoided now (Q1); revisit only if shared case lists
   appear, re-checking for a post-2024 release.
6. **Compile-time impact of macro expansion across 269 tests** — [Inference, based on
   observed patterns: rstest_macros is syn-based like our own lsp_macros; spike compile
   was unremarkable, but the suite-wide effect is unmeasured] — watch the first full
   battery run.

## Spike cleanup

`tests/rstest_spike.rs` deleted after the ladder; `git status` clean (no changes to
`src/`, `Cargo.toml`, or `Cargo.lock` at any point; the three spike iterations' failures
— `expect_used`, `no-sync-io`, `nonexistent-path-in-comment` — were each investigated as
findings F2/F3/F6, not suppressed).

---

*Verdicts: `rstest_reuse` — unnecessary, avoid. Spike — all toolchain legs green on
pinned stable + dylint nightly. `conversion_tests!` — keep plain emission, reasoned
exemption. Ecosystem — 3281 reverse deps, mainstream. Fixtures — Drop-guard temp
workspace + seeded-state family in `crate::testing`; wire tier stays plain fns.*
