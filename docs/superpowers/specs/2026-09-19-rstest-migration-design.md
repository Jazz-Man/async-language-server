# rstest migration — test-suite overhaul design

*Spec, 2026-09-19. Branch `feature/rstest`. Evidence base:
`docs/superpowers/research/2026-09-19-rstest-integration.md` (toolchain
spike, `rstest_reuse` verdict, `conversion_tests!` merit analysis, field
evidence, fixture sketches — all verdicts below cite it). The migration
decision itself is owner-ratified: rstest 0.27.0 is already a
dev-dependency.*

## 1. Context

The suite's three duplication axes (107 `fs::write` fixture sites across
6 test files; `canonicalize`+`Url::from_file_path` chains; 20
state-setup triplets in `src/server/state/tests.rs` alone, 1747 lines)
grow from SDD fresh-context implementers who cannot see the existing
suite. The owner mandated the remedy as its own cycle: migrate **every
handwritten test function** to rstest and rebuild the suite's
architecture around fixtures and cases — deduplication, simpler tests,
and cheaper tokens for future tests.

Baseline (grep, 2026-09-19): 207 `#[test]` + 62 `#[tokio::test]` = 269
handwritten test functions across 12 test files/harnesses; 29
`conversion_tests!` occurrences; 26 wire spawn sites.

## 2. Decisions

| # | decision |
|---|---|
| D1 | Two fixture homes by tier: `src/testing.rs` (crate/W0) and `src/server/testing.rs` (wire) — mirroring the existing harness split. |
| D2 | `TempWorkspace` Drop-guard fixture with methods (`.write(rel, text) -> Url`, `.url(rel)`, `.root()`, `Deref` to `PathBuf`); attribution prefix via `#[with("module")]`. |
| D3 | Wire tier: **plain fns stay** — `spawn_wire_server`/`RawClient`/`EchoServer` are explicit calls; repeated handshake tails dissolve into local plain async helpers in their own files. No wire fixture (survey: 17 of 26 spawn sites are bespoke servers; a default fixture would cover 9 and chase handshake variations). |
| D4 | `rstest_reuse`: **not adopted** (last release 2024-05; would mint a `rand`+`getrandom` group in the lock; no shared case-list need). Revisit only if the need appears. |
| D5 | `conversion_tests!` keeps plain `#[test]` emission — reasoned exemption (owner pre-authorized): rows are compile-time-shaped per-`Request` types with closures that lose inference in case expressions; migration buys zero and renames 29 tables at once. The macro's own 7 handwritten unit tests migrate like any other. |
| D6 | Doctests are out of scope (owner-ratified): they are doc examples, not test functions. |
| D7 | Migration order is risk-first: fixture foundation → `state/tests.rs` (hardest file, all three axes) → remaining W0 modules → wire tier → final sweeps. |
| D8 | Disposition-table reconciliation: each conversion task records an old→new test-name map in its report; one final reconciliation pass updates the mutants disposition table (rows renamed, oracle-covered, or dropped). |
| D9 | The `.claude/rules/testing.md` rework is the cycle's **final** task, written from what actually happened, executed through the **steering** skill (draft → conflict-check → approval), not a bare file edit. |

## 3. Fixture architecture

All fixtures live under `#[cfg(test)] pub(crate)` in the two harness
modules — nothing new enters the crate's public API. rstest findings
F2/F3 make this placement load-bearing: clippy's test context is
syntactic and arch-lint lints any `tests/` directory as production
code, so no integration-test target may be created.

### 3.1 `src/testing.rs` — crate tier

- **`TempWorkspace` guard** (replaces `temp_workspace(prefix, name)` +
  the 107 `fs::write` sites + `canonicalize` chains + manual
  `fs::remove_dir_all` tails): created by the `#[fixture] fn workspace`,
  unique-name root under `std::env::temp_dir()`; `.write(rel, text) ->
  Url` performs write + canonicalize + `Url::from_file_path` in one
  call; `.url(rel)` for existing files; `Drop` removes the tree — also
  on unwind, closing today's leak-on-failure window. The attribution
  prefix (which module leaked a directory) is kept via
  `#[with("walker")]`-style overrides.
- **State fixture family**: `state()` (default `TestServer`, closed
  `ClientSocket`); the canonical UTF-16 fixture
  (`state_with_documents` shape — the `"🙂abc"` document stays
  load-bearing) as a fixture; capability-seeded variants
  (`gated_state`) via composition; and a plain async
  `seed_workspace(&TempWorkspace) -> SeededWorkspace` helper absorbing
  the state-setup triplet (folders + advertise + refresh) that repeats
  20 times in `state/tests.rs` — a helper, not a fixture: the refresh
  must run after the test's own writes, and fixtures resolve before the
  test body (the same sequencing rule that keeps wire spawn explicit).
- Existing position/token/url helpers pass the §6 audit unchanged as
  plain fns where fixtures cannot express them (`url` is called
  mid-test on arbitrary names, not at injection time).

### 3.2 `src/server/testing.rs` — wire tier

`spawn_wire_server`, `RawClient`, `EchoServer`, `bounded` stay plain
(D3). Wire tests still run under `#[rstest]` + `#[tokio::test]` and may
inject crate-tier fixtures (e.g. `TempWorkspace` in
`workspace_diagnostics.rs`). Where a file's tests share a handshake
tail (e.g. the Watcher/Refresh servers in `workspace_diagnostics.rs`,
7 sites), a local plain async helper in that file wraps spawn +
initialize for its own servers. `GatedServer`/`PanickingServer` stay
local to `robustness.rs` as today.

## 4. Hard rules (from the spike, spec-mandated)

1. **Imports**: `use rstest::{fixture, rstest};` only. Never
   `use rstest::*` (`wildcard_imports` is pedantic-deny). `#[case]`,
   `#[values]`, `#[future]`, `#[with]`, `#[default]` are never
   imported — the `#[rstest]` macro consumes them as inert tokens.
2. **Location**: all rstest code in `src/` `#[cfg(test)]` modules. No
   `tests/` directory targets (arch-lint scans them as production;
   clippy's `allow-expect-in-tests` is syntactic and would deny
   fixtures there).
3. **Async shape**: `#[rstest]` above, `#[tokio::test]` below; async
   fixtures consumed via `#[future]`. `#[once]` is not used (every
   test needs isolated state).
4. **Naming**: descriptive `#[case::name]` on every case row — bare
   `case_N` is forbidden (generated names `case_N_name` feed nextest,
   the mutants disposition table, and dupes legibility). For
   `#[values]` rows whose values do not slug readably (URLs, globs,
   whitespace strings), prefer named case rows or 0.27's doc-comment
   name override.
5. **Feature gates**: a gated row uses
   `#[cfg_attr(feature = "tree-sitter", case(...))]`; a gated test
   keeps `#[cfg(feature)]` on the fn. Both legs of the battery must
   stay green.
6. **Doc comments**: paths referenced in comments must exist (dylint
   `nonexistent-path-in-comment`).
7. **nextest selectors**: target-level filters use
   `-E 'binary(...)'`; fn/case names match with `-E 'test(...)'` or a
   plain test-name substring. `make mutants FILE=...` scoping and the
   single-test recipe in the rules move to `-E` expressions.

## 5. Conversion rules (core of the future testing.md)

- Every test fn becomes `#[rstest]`; single-case tests included — one
  uniform attribute, fixtures always injectable.
- A family becomes `#[case]` rows **only when members differ in data,
  not orchestration** — a different await/setup flow is a separate
  test, not a row.
- `#[values]` for one-axis variation; a matrix only when every
  combination is a distinct oracle; convention caps matrix width.
- Tests that mutate files mid-test (stamp/watch/watcher families) stay
  single tests — rows carry data, not sequences.
- Deletion is by mutation oracle (R6): a migrating family loses a
  member only when no mutator class distinguishes it; textual
  similarity is never the criterion. Twins pinning different axes
  survive as separate rows/tests.

## 6. Audits (final sweeps)

- **R4 utility audit**: every helper in both harnesses and every local
  test helper in the 12 files ends as exactly one of: becomes a
  `#[fixture]`, stays a plain fn (with a reason fixtures cannot express
  it), or is deleted. The harness is expected to shrink.
- **R5 visibility audit**: enumerate items whose visibility was widened
  solely for test access; sibling `tests.rs` modules already see their
  parent's private items, so tighten exactly those whose only
  cross-module consumers were helpers this cycle deletes or localizes.
- **dupes-ignore dissolution**: entries whose groups dissolve under the
  migration are removed (`cargo dupes cleanup` dry-run first); no
  threshold is touched.
- **Disposition reconciliation** (D8): the final pass applies every
  task's name map to the mutants disposition table.

## 7. Migration waves (order fixed; task boundaries belong to the plan)

1. **Foundation** — fixture sets in both harnesses; no test converted;
   battery green both legs.
2. **`src/server/state/tests.rs`** — the hardest file first: validates
   the fixture API against all three duplication axes before the rest
   of the suite commits to it.
3. **Remaining W0 modules** — `documents/`, `workspace/` (walker,
   diagnostics, parallel), `text_utils`, `server/with_state/tests.rs`,
   `oneshot`, handwritten tests in `lsp_requests/` modules (macro-stamped
   rows excluded per D5).
4. **Wire tier** — `src/server/tests/*` (7 files).
5. **Final sweeps** — §6 audits, name-map reconciliation.
6. **testing.md via steering** (D9) — content: the §4 hard rules and
   §5 conversion rules as normative text, the fixture inventory, the
   exemption text for `conversion_tests!` ("rows stamped by
   `conversion_tests!` keep plain `#[test]` emission — the macro is the
   table harness; its per-row fns are not handwritten tests"), the
   `-E` selector recipe, the Drop-guard convention, and a pointer for
   SDD briefs to name neighboring fixtures.

Each wave's tasks carry the full battery as the done-bar, and each
conversion task's report includes its old→new test-name map (D8).

## 8. Scope exclusions

- Doctests (D6).
- `conversion_tests!` emission (D5) — exemption recorded in testing.md
  at cycle end. **Revisit intent (owner, 2026-09-19):** after this
  cycle, migrating the 29 tables becomes a candidate follow-up — the
  macro can stamp typed closure wrappers itself (dissolving the
  inference blocker), and the D8 name-map discipline covers the rename
  churn. Decision deferred to post-cycle results; the exemption stands
  for this cycle.
- Wire fixture (D3) — wire setup stays explicit calls + local helpers.

## 9. Verification

- `make battery` both feature legs after every task.
- `make dupes` 0/0 maintained; expected to shed dissolved groups.
- `make deny` — baseline verified: rstest adds no duplicate-version
  groups (`futures-util` unified 0.3.34, `futures-timer` 3.0.4 single,
  `syn` 2/3 split pre-existing).
- `make dylint` — perfectionist on case rows verified green in the
  spike, not assumed.
- Compile-time watch: first full battery after wave 2 gauges the
  suite-wide expansion cost (research risk 6, [Inference]): if it
  regresses materially, the plan revisits before waves 3-4.

## 10. Success criteria

Verifiable at cycle end:

1. No handwritten plain `#[test]` attribute remains on an actual test fn
   in either workspace member: `grep -rn '#\[test\]' src macros/src --include='*.rs'` matches only the token-emission templates inside the `lsp_macros` bodies (`quote!` string literals, kept per D5) and the doc comments describing them — never a test fn. `#[tokio::test]` remains on async fns by design, always beneath `#[rstest]` (§4.3).

   *Erratum, 2026-09-20: the original probe `grep -rn '#\[test\]\|#\[tokio::test\]' ...` was self-contradictory — §4.3's mandated `#[rstest]`-above/`#[tokio::test]`-below stacking keeps `#[tokio::test]` on every async test fn, so its half of the probe could never match "only the emission templates". Corrected to the plain-`#[test]` probe above.*
2. `fs::write` appears only inside fixture/guard internals — the 107
   baseline collapses to the single `TempWorkspace::write`.
3. Manual `fs::remove_dir_all` tails exist only in the guard's `Drop`.
4. Both harness modules are smaller than their pre-cycle selves, with
   the §6 audit explaining every surviving item.

   *Deviation, 2026-09-20: `src/testing.rs` grew 362→524 lines — growth this spec's own §3.1 architecture mandates (the `TempWorkspace` guard, the state-fixture family, `SeededWorkspace`), net of the T9c deletions; `src/server/testing.rs` stayed byte-identical to `main` (D3). The criterion's intent — no unjustified growth, every surviving item explained — is met through the T9c reference-count audit.*
5. Battery green (both legs), dupes 0/0, deny green.
6. Mutants disposition table reconciled against the collected name
   maps.
7. testing.md reworked through the steering skill, reflecting executed
   reality.
