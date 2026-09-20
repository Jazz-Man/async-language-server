# conversion_tests! Tables Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate all 27 `conversion_tests!` tables (31 rows) to rstest case-tables — macro-stamped typed columns, injected fixture — per spec `docs/superpowers/specs/2026-09-20-conversion-tables-migration-design.md`.

**Architecture:** `conversion_tests!`'s `expand()` emits ONE `#[rstest]` fn per table (the script once) with one `#[case::name]` per row (data only: typed closure/value columns); the fixture is injected by parameter name. Inference dissolves via macro-stamped type ascriptions using the row's known `Request` type.

**Tech Stack:** rstest 0.27 (dev-dep of both workspace members), `lsp_macros` proc-macro crate, existing `crate::testing` fixtures.

## Global Constraints

- **Row semantics are the contract**: every row's assertions survive 1:1 under the new shape — verify per file against the pre-task emission (`git show`). The script is PORTED from the current per-row emission, not reinvented.
- **Naming**: each row keeps its current test name as `#[case::name]` → new id `module::tests::<table_fn>::case_N_<old_name>`. Bare `case_N` forbidden. Every task's report carries its old→new name map.
- **Imports**: `use rstest::rstest;`-style named imports only (emitted code + each file's `use crate::testing::utf16_state;`); never `use rstest::*`; `#[case]`/`#[with]` never imported; attribute tokens only.
- **Fixture injection**: emitted table fn takes `utf16_state: (ServerState, Url, Url)` resolved by parameter name; if the Task-1 spike finds generated-code name resolution needs it, the `#[from(crate::testing::utf16_state)]` fallback applies (recorded either way).
- **Typed columns**: closure/value columns carry macro-stamped type ascriptions derived from the row's `Request` type via its associated types (`<R as crate::lsp_requests::Request>::Params`, `::Response`) or the concrete types at stamp time — whichever the spike proves clean; the choice and reason go in the Task-1 report and are then binding for Waves A–C.
- **D5-inverse boundary**: the 291 already-migrated handwritten tests are untouched (no edits outside the 27 table files + `macros/src/` + the consequences docs). `state_with_documents` stays (the `utf16_state` fixture wraps it).
- **Dupes protocol**: body-twin groups of the T4 class may mint; collect fingerprints + one-line reasons per task in the report; never self-add entries, never contort bodies; the controller adds reasoned successor entries at the pause.
- `expect`/`unwrap` allowed in tests; no `#[allow]`; no suppression; no git writes (owner commits per task); `make battery` both legs zero warnings is each task's done-bar, plus `cargo dupes` (green or fingerprints).
- Controller duty: each brief names the wave's files, the Task-1 emission shape (binding), and the fixture-import line.

**Task-1 emission target (binding sketch, from the real hover.rs table):**

```rust
// macro-emitted for the hover.rs table (sketch — exact column types fixed
// by the spike; names/anchors real):
#[rstest]
#[case::hover_incoming_utf16_becomes_utf8(
    /* params */ |uri| async_lsp::lsp_types::HoverParams { /* ... */ },
    /* incoming */ |p| p.text_document_position_params.position,
    /* expects */ line_position(0, 4),
    /* response */ None,
    /* outgoing */ None,
    /* returns */ None,
)]
#[case::hover_outgoing_utf8_becomes_utf16(/* ... */)]
fn hover_conversion_round_trips(
    utf16_state: (crate::server::ServerState, Url, Url),
    #[case] params: fn(Url) -> <HoverRequest as crate::lsp_requests::Request>::Params,
    #[case] incoming: fn(&<_>::Params) -> Position,          // spike fixes the exact form
    #[case] expects: Position,
    #[case] response: Option<fn(&Url, &Url) -> Option<Hover>>,
    #[case] outgoing: Option<fn(&Option<Hover>) -> Position>,
    #[case] returns: Option<Position>,
) {
    // the script, stamped ONCE — ported verbatim from today's per-row
    // emission, with state_with_documents() replaced by the injected
    // fixture destructure
}
```

(The table fn name is stamped by the macro from the file's request name —
`<request>_conversion_round_trips` — uniform across all 27 files; the
spike may adjust, the choice is then binding.)

---

### Task 1: Macro rework + hover.rs end-to-end + macro's own tests (validation gate)

**Files:** `macros/src/conversion_tests.rs` (emission + its test module), `src/lsp_requests/hover.rs`.

- [ ] Rework `expand()` to emit the table shape (sketch above): parse rows as today, stamp one `#[rstest]` fn with typed columns + `#[case::name]` rows; port the per-row script into the single body (fixture destructure instead of `state_with_documents()`); stamp the fixture param; `#[case]` on every column param.
- [ ] Spike decisions recorded in the report (binding for Waves A–C): fixture resolution form (param-name vs `#[from]`); column typing form (associated types vs concrete); optional-column encoding (`Option<fn>`/`Option<Value>` vs row struct if `too_many_arguments` fires); table fn name scheme.
- [ ] Rework the macro's own test module to the new emission (the 4 fns assert stamped output — update expectations; the emission-template test now asserts `#[rstest]`/`#[case]` forms).
- [ ] Convert `hover.rs`: table now relies on the new emission; add `use crate::testing::utf16_state;` if the param-name form needs it in scope.
- [ ] Verify: `cargo nextest run -p lsp_macros` green; `cargo nextest list` shows `hover_conversion_round_trips::case_1_hover_incoming_utf16_becomes_utf8` / `::case_2_hover_outgoing_utf8_becomes_utf16` (exact ids recorded); `git show HEAD:src/lsp_requests/hover.rs` row asserts vs the new rows — 1:1; `cargo fmt`; clippy `-D warnings`; `make battery` both legs; `cargo dupes`.
- [ ] Report (`.superpowers/sdd/ct-task-01-report.md`): spike decisions, the binding emission shape verbatim, hover name map (2 rows), verification tails, concerns.

### Task 2: Wave A — 9 files, simple single rows

`moniker.rs`, `signature_help.rs` (no response column), `declaration.rs`, `implementation.rs`, `type_definition.rs`, `document_highlight.rs`, `linked_editing_range.rs`, `rename_prepare.rs`, `references.rs` (2 rows).

- [ ] Convert each file's table to the new emission (no macro edits — Task 1's shape is binding; if a row shape genuinely doesn't fit, STOP and report NEEDS_CONTEXT rather than bending the shape).
- [ ] Per file: name map; row asserts vs `git show` 1:1; fixture import where needed.
- [ ] Verify scoped (`-E 'test(lsp_requests::)'`), then `make battery` both legs, `cargo dupes`.
- [ ] Report with all wave name maps + dupes result.

### Task 3: Wave B — 9 files, standard response/outgoing rows

`definition.rs`, `document_format.rs`, `document_range_format.rs`, `document_link.rs`, `on_type_formatting.rs`, `will_create_files.rs`, `will_delete_files.rs`, `will_rename_files.rs`, `will_save_wait_until.rs`.

Same steps as Task 2.

### Task 4: Wave C — 8 files, shape-heavy tail

`code_lens.rs`, `completion.rs`, `document_symbol.rs`, `inlay_hint.rs`, `prepare_call_hierarchy.rs`, `prepare_type_hierarchy.rs`, `subtypes.rs` (2 rows, 2 response cols), `supertypes.rs` (2 rows, 2 response cols).

Same steps as Task 2; subtypes/supertypes carry the two-response shapes — flag any column-arity surprise for the controller rather than improvising.

### Task 5: Consequences — disposition + rules + final checks

- [ ] Disposition reconciliation: apply every task's name map (31 rows) to `docs/superpowers/plans/2026-09-09-mutants-disposition-table.md`; add the dated note.
- [ ] Steering edit to `.claude/rules/testing.md` (through the steering skill, small pass): D5 exemption note → new reality; the adjudicated-table's exemption row flips; stamped-table fixture idiom noted; conflict-check as usual.
- [ ] Stale-mention sweep: `grep -rn 'conversion_tests' CLAUDE.md .claude/rules/ docs/superpowers/specs/2026-09-19-rstest-migration-design.md` — historical specs stay as written (dated records); CLAUDE.md/tech.md/structure.md must state the present.
- [ ] Full gates: `make battery` both legs, `cargo dupes` (successor entries resolved), and the spec §7 success criteria probes (grep `#[test]` → zero outside doc comments).
- [ ] Report: reconciliation summary, steering diff summary, probe results.

---

## Self-review notes (plan-author, 2026-09-20)

- Spec coverage: D1–D6 map to Tasks 1–5 (D5 risk-first = Task 1's gate; consequences D1 = Task 5); §7 criteria verified at Task 5.
- Binding-shape strategy: Task 1 fixes the emission verbatim (spike decisions recorded), Waves A–C are mechanical applications with a STOP-and-report escape hatch — no per-wave reinterpretation.
- Counts: 27 files / 31 rows (23×1 + hover/references/subtypes/supertypes ×2); hover consumed by Task 1, 26 files split 9/9/8.
