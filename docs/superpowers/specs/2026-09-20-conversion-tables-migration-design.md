# conversion_tests! tables → rstest — migration design

*Spec, 2026-09-20. Branch `feature/rstest` (continues after the merged
rstest-migration cycle). Pre-agreed path: the 2026-09-19 research Q3 merit
analysis (owner-ratified exemption D5 with the revisit note), the first
cycle's spec §8 revisit intent, and testing.md's D5 note ("migrating those
27 tables is a deferred follow-up — re-adjudicate before extending the
exemption"). The re-adjudication happened at the owner's direction
2026-09-20: uniformity and naming are the gain; dedup is not (the shared
body already lives in the macro template) — accepted openly.*

## 1. Context

`conversion_tests!` (macros/src/conversion_tests.rs) stamps one plain
`#[test]` fn per row across 27 tables in the `src/lsp_requests/` modules —
the only plain-`#[test]` emission left in the workspace. The rstest cycle
migrated all 291 handwritten tests; the tables stayed by exemption D5
(compile-time-shaped rows, closure inference, rename churn). The blockers
now have a path: the macro can stamp typed closure wrappers itself, and the
D8 name-map discipline covers the churn.

## 2. Decisions

| # | decision |
|---|---|
| D1 | Scope: tables + consequences in one cycle — macro rework, 27 table conversions, disposition reconciliation, steering edit to testing.md (owner 2026-09-20). |
| D2 | Emission: one `#[rstest]` fn per table; each row becomes `#[case::name(...)]` carrying the row's current test name; the body runs once per table (the script today duplicated per row). |
| D3 | Fixture by injection: the emitted fn takes `utf16_state: (ServerState, Url, Url)` resolved by parameter name (each file imports `use crate::testing::utf16_state;`); fallback mechanism if name-resolution fails in generated code: `#[from(crate::testing::utf16_state)]` — decided by the Task-1 spike, recorded either way. |
| D4 | Inference dissolves by macro-stamped types: the row's `Request` type is known at stamp time, so closure values are emitted with explicit fn-pointer type ascriptions (`params: fn(&Url) -> P`; optional columns as `Option<fn(...)>`). |
| D5 | Risk-first order: Task 1 = macro rework + ONE table converted end-to-end (emission, nextest names, dupes behavior validated); then the remaining tables in file waves; macro's own tests reworked to assert the new emission. |
| D6 | `state_with_documents` stays (the `utf16_state` fixture wraps it; it remains load-bearing through the fixture). |

## 3. Emission shape (target)

Before (per row, stamped):

```rust
#[test]
fn hover_incoming_utf16_becomes_utf8() {
    let (state, _, emoji) = state_with_documents();
    // ... fixed script: build params, modify_params, assert, modify_response, assert
}
```

After (per table, stamped — sketch; the exact column typing is fixed by
the Task-1 spike, default = one typed column per grammar field):

```rust
#[rstest]
#[case::hover_incoming_utf16_becomes_utf8(/* params fn, optional hooks, expected */)]
#[case::references_round_trips_both_directions(/* ... */)]
fn conversion_round_trips(  // table fn name: per-file, meaningful
    utf16_state: (ServerState, Url, Url),
    #[case] params: fn(&Url) -> HoverParams,
    #[case] incoming: Option<fn(&Url, &mut HoverParams)>,
    // ... one typed column per grammar field: expects, response, outgoing, returns
) {
    // the fixed script, once per table — rows carry only data
}
```

(The row grammar is `params`, `incoming`, `expects`, `response`,
`outgoing`, `returns`; the spike may swap the column tuple for a stamped
row struct if arity or `clippy::too_many_arguments` demands it — the
choice and reason land verbatim in the plan.)

## 4. Consequences (in-cycle)

- **Disposition reconciliation**: every stamped name changes
  (`module::tests::<old>` → `module::tests::<table>::case_N_<old>`); each
  conversion task records its map; one final pass applies them to
  `docs/superpowers/plans/2026-09-09-mutants-disposition-table.md`.
- **testing.md steering edit** (small, via the steering skill): the D5
  exemption note is replaced by the new reality (macro-stamped rstest
  tables; no exemption); the "rstest surface, adjudicated" table's implicit
  exemption row flips; the fixture-injection idiom for stamped tables is
  noted. Conflict-check against tech/structure as usual.
- **Dupes watch**: body-twin groups of the T4 class may mint (content in
  excluded attributes); the ratified successor-entry mechanism applies —
  fingerprints collected per task, entries added at the pause with reasons.

## 5. Waves

1. Macro rework + one table end-to-end (validation gate).
2. Remaining tables in file waves (grouped by row-grammar shape: plain
   position rows first, then range/custom/response-rich rows).
3. Macro's own test module reworked to the new emission.
4. Final: disposition reconciliation + steering edit.

## 6. Gates

`make battery` both legs (zero warnings) per task; `cargo dupes` per task
(green or fingerprints); D5-boundary inverse — this cycle the macro files
ARE in scope, but the 291 already-migrated tests' oracles are untouched
(conversion tasks change emission, not row semantics; every row's asserts
survive 1:1 under the new shape).

## 7. Success criteria

1. `grep -rn '#\[test\]' src macros/src --include='*.rs'` → zero matches
   outside doc comments (the `quote!` template itself now stamps rstest
   forms; nothing plain remains).
2. All 27 tables run as rstest case-tables; nextest ids carry the old test
   names as `case_N_<name>` (spot-verifiable via `cargo nextest list`).
3. Row semantics preserved: each row's assertions identical in strength to
   its pre-migration stamped test (verified per task against the old
   emission via `git show`).
4. Disposition table reconciled from the collected maps.
5. testing.md updated via steering; no stale exemption text anywhere
   (CLAUDE.md/tech.md checked for mentions).
6. Battery both legs, dupes 0/0 (with any successor entries reasoned),
   zero warnings.
