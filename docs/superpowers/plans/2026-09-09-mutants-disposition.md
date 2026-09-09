# Mutant Survivor Disposition — Plan B Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Disposition every one of the 149 surviving mutants from the 2026-09-09 cargo-mutants baseline — each into exactly one of type-fix / pin-test / doctest-covered / accepted-with-contract-reason — implement the (a)/(b) dispositions, and verify with a final full `make mutants` re-run in which no (a)/(b) mutant survives. Spec: `docs/superpowers/specs/2026-09-09-test-tooling-integration-design.md` D6–D8.

**Architecture:** Two phases. Phase 1 produces the disposition table (analysis artifact, owner-reviewed — the business-logic gate) and verifies the 12 timeouts in isolation. Phase 2 implements the table in four module-scoped batches, each batch kill-verified against its own baseline diffs; one final task runs the full sweep and reconciles. A survivor is a question, never a defect report; (d) accepted is an expected mass outcome.

**Tech Stack:** cargo-mutants 27.1.0 (exit 2 = survivors found), nextest, rust-skills + LSP-first workflow, `crate::testing` fixtures.

## Global Constraints

- **Type first, test second** (normative, `.claude/rules/testing.md`): before any pin test, the disposition must record why no type can remove the questioned behavior. No tests for coverage statistics; no numeric kill-rate target (D8).
- **The table governs**: no implementation outside a table row's disposition. When implementation proves a row wrong, the row is **owner-decided, not implementer-revised**: the implementer stops at that row, leaves it unimplemented, and reports a worked proposal — what the table assumed, the observed reality, concrete behavior examples (sample inputs/outputs that distinguish the candidate readings), and each candidate disposition with its trade-off. The controller surfaces the proposal to the owner; only after the owner's choice is the `revised:` note appended to the table row, quoting the decision.
- **Baseline is frozen**: `.superpowers/mutants-baseline-2026-09-09/` (missed.txt = 149 rows; diff/ = one patch per mutant; outcomes.json). `mutants.out/` is scratch — never treat it as the inventory.
- **Disposition taxonomy, exactly one per survivor**: (a) type-first change; (b) pin test at the lowest tier that sees it (W0 inline/sibling per module layout, wire only for what W0 cannot see); (c) doctest-covered — the pinning doctest is named; (d) accepted — reason cites the component's behavioral contract (module docs, `structure.md` three-place contracts, trait-mandated defaults), never tool convenience.
- **Mandatory procedure per cluster** (D6): LSP first (references/callers of the mutated function — not grep), targeted contract reads, rust-skills consultation matched to the change class (`test-*`, `api-*`/`type-*`, `err-*`).
- **Kill-verification per implemented (a)/(b) row**: apply the row's `.diff` from the baseline (`git apply` the patch file), run the covering test via `cargo nextest run <filter>`, observe failure, revert (`git apply -R` / checkout the file). Do not run full sweeps mid-plan — scoped evidence only; the single full sweep is Task 6.
- **Dupes gate duties**: every new test batch runs `make dupes`; new duplicate groups get reasoned `.dupes-ignore.toml` entries (rust-skills-grounded avoidance analysis per entry) or the tests are reshaped — thresholds are never loosened.
- No git mutations (owner commits); English artifacts; both feature legs green after every task; battery = `make battery`; no `#[allow]`/suppressions anywhere; tree-sitter-gated tests gate themselves with `#[cfg(feature = "tree-sitter")]` and shared-harness code stays feature-free.

---

### Task 1: Timeout verification + the disposition table

**Files:**
- Create: `docs/superpowers/plans/2026-09-09-mutants-disposition-table.md` (the durable artifact D4 promised)
- Test (re-run only): the 12 baseline timeout scenarios via `make mutants FILE=<file>` (or `-F` regex); a timeout that passes on re-run was an artifact of the concurrent Miri session and leaves the table (recorded as such); one that times out again joins the missed inventory.

**Interfaces:**
- Consumes: `.superpowers/mutants-baseline-2026-09-09/{timeout.txt,missed.txt,diff/,outcomes.json}`.
- Produces: the table every later task implements — one row per survivor: `| cluster | function@file | mutation summary | disposition (a/b/c/d) | reason (contract-cited) | covering test/doctest name |`. Cluster inventory (from missed.txt): text_utils conversions ~20 + range_ext ~37; tree-sitter navigation ~16; Document accessors/reader ~19; workspace/diagnostics state machine 17; oneshot 11; server defaults/options ~7; state ~3; lsp_requests conversions 10; matcher 3.

- [ ] **Step 1: Re-verify the 12 timeouts in isolation** — one scoped run per affected file, nothing heavy running concurrently; record each outcome (pass → artifact, drop; timeout again → keep as survivor).
- [ ] **Step 2: Build the table** — for every survivor (149 ± timeout adjustments): LPS references/callers of the function, read its contract (module docs, `structure.md`), consult rust-skills for the change class, then choose (a)/(b)/(c)/(d) with a contract-citing reason. Type-first candidates are stated as such with the proposed type; (c) rows name the exact doctest (verify it exists — `cargo test --doc --workspace --all-features` output); (d) rows quote the contract sentence that makes the behavior non-load-bearing.
- [ ] **Step 3: Self-review the table** — 149 rows + verified-timeout rows present; every row has exactly one disposition and a reason; no "(d) because mutants is annoying" style reasons; cluster subtotals sum to the total.
- [ ] **Step 4: OWNER REVIEW GATE (hard)** — present the table to the owner; the owner validates dispositions against business logic. Only owner-approved rows proceed to implementation. Pause; no implementation in this task.

### Task 2: text_utils — conversions + RangeExt (the ~57-row batch)

**Files:** per table rows — `src/text_utils/encoding.rs`, `position.rs`, `range_ext/{bytes,lsp,tree_sitter}.rs` and their sibling test files; `.dupes-ignore.toml` if new groups appear.
**Interfaces:** Consumes Task 1's table rows for the text_utils clusters. Produces pin rows in the established spec-matrix style (fixtures from `crate::testing`; local `r()` builders stay local; asymmetric inputs chosen so `-`≠`+`, `>`≠`>=`, `==`≠`!=` are distinguishable — the surviving arithmetic mutations prove the existing rows' inputs are symmetric; that asymmetry is the content of each new row).

- [ ] Implement every (a)/(b) row in the cluster; (c)/(d) rows get no code — the table is their record.
- [ ] Kill-verify each implemented row against its baseline `.diff` (apply → nextest filter → fail → revert).
- [ ] `make test && make test-no-default-features` (tree_sitter flavor is gated) + `make dupes` + reasoned ignore entries where the gate fires.
- [ ] Report + owner commit pause.

### Task 3: tree-sitter navigation + Document accessors (~35 rows)

Same shape as Task 2 for: `src/tree_sitter_utils.rs` (`find_*`, `ts_*` converters, contains), `src/documents/document.rs` (`node_at_*`, `node_text`, `text_bytes`, accessors, `DocumentReader::read` arithmetic), `src/documents/matcher.rs` (`lang_strings`). Tree-sitter rows gate with `#[cfg(feature = "tree-sitter")]`; fixtures reuse the per-matcher compiled-query cache; grammar fixtures via the established `json_matchers` helper. Accessor rows expected to land mostly (c)/(d) — implement only what the approved table says.

### Task 4: workspace/diagnostics state machine + server/state (~20 rows)

Same shape for: `src/workspace/diagnostics.rs` (`can_*`, `*_generation`, `apply_enabled`, `request_configuration`, `register_configuration`, `refresh_diagnostics`, `push_related_reports`), `src/server/state/mod.rs` (encoding set/get, diagnostics-enable flag). Wire-tier behavior is out of scope unless a row's contract demands it — W0 state-machine pins preferred (atomics are directly constructible).

### Task 5: lsp_requests conversions + oneshot + server defaults (~28 rows)

Same shape for: `src/lsp_requests/{code_action,code_action_resolve,conversion}.rs`, `src/oneshot/{server,workspace_diagnostics}.rs`, `src/server/{options,server_trait}.rs`. Conversion rows follow the `conversion_tests!` / W0 conventions of their request files; server-trait default rows expected (d) trait-mandated unless the table says otherwise.

### Task 6: Final full sweep + reconciliation (the acceptance gate)

- [ ] `make battery` green.
- [ ] `make mutants` (full, alone, nothing concurrent). Reconcile its missed list against the table: every (a)/(b) row must be killed; remaining survivors must all be table rows with disposition (c)/(d). Any unexplained survivor → back to the owning task's fix loop.
- [ ] Update the table file with the final outcome column (killed / survived-as-dispositioned).
- [ ] Ledger + report; owner commit; cycle close (baseline snapshot dir may be deleted by the owner after this task).

---

## Self-Review (plan time)

1. **Spec coverage:** D6 procedure → Task 1 steps + Global Constraints; D8 → no rate targets, acceptance = table + sweep; spec acceptance bullets (table 149/149, zero live (a)/(b), docs already target-based from Plan A) → Tasks 1/6. Timeout caveat → Task 1 Step 1. ✓
2. **Placeholder scan:** no code contents are embedded because Phase 1's approved table is the single source for them — each implementation task's contract is "the table rows for cluster X", with conventions, fixtures, and kill-verification fully specified. The cluster inventories carry exact counts and function names from the frozen baseline. ✓
3. **Consistency:** taxonomy letters identical across spec/plan; kill-verification loop identical in every task; the hard owner gate sits exactly where D6 puts the business-logic judgment. ✓

## Execution Handoff

Subagent-driven (recommended; fresh implementer per task, two-verdict review, owner commits at pauses — with the Task 1 gate being an owner review, not just a commit pause). Task 1 first; Tasks 2–5 order can flex after the table exists (they are module-disjoint), Task 6 always last.
