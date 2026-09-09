# Test-Tooling Infrastructure — Plan A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the Makefile command contract, nextest as the test runner inside it, and the mutants/Miri on-demand tooling — stages 1–3 of `docs/superpowers/specs/2026-09-09-test-tooling-integration-design.md`.

**Architecture:** One flat root `Makefile` becomes the single source of verification commands; docs (`CLAUDE.md`, `.claude/rules/tech.md`, `.claude/rules/testing.md`) point at target names instead of inlining cargo commands. nextest replaces `cargo test` inside the two test targets (doctests stay a separate `cargo test --doc` step per leg). mutants becomes a committed on-demand target; Miri is decided by a one-time cross-interpretation experiment whose outcome adds targets or records a deferral. Plan B (survivor disposition) is written after this plan lands.

**Tech Stack:** GNU make (macOS system make 3.81+), cargo-nextest (installed), cargo-mutants 27.1.0 (installed), Miri (nightly rustup component).

## Global Constraints

- Spec `docs/superpowers/specs/2026-09-09-test-tooling-integration-design.md` is binding (D1–D8). Branch: `feature/nextest`. **No git commands — the owner commits after each task's clean review.**
- All written artifacts in English. Working language for reports: as dispatched.
- Both feature legs green at every task boundary; `make battery` green at plan end.
- rust-skills + LSP-first for any source/doc work: locate via LSP, targeted reads, no whole-file reads.
- No suppressions, no workarounds; a failing check is investigated, never silenced.
- After Task 1, raw verification commands live only in the Makefile; artifacts reference targets. Allowed exception: the single expansion table in `tech.md` and single-test prose (`cargo nextest run <filter>`).
- **Makefile recipes are TAB-indented** (make's hard requirement; spaces produce "missing separator").
- Nothing heavy may run concurrently with a mutants sweep (spec D4).

---

### Task 1: Makefile contract + nextest adoption + docs sync

**Files:**
- Create: `Makefile`
- Modify: `.claude/rules/tech.md` (battery block, dylint/dupes/bench paragraphs, single-test line), `CLAUDE.md` (Commands section), `.claude/rules/testing.md` (`cargo dupes check` mentions → `make dupes`)

**Interfaces:**
- Produces (consumed by Tasks 2–3 and all future artifacts): the target contract `help, fmt, fmt-fix, clippy, doc, test, test-no-default-features, dylint, battery, dupes, bench` — names are fixed; later tasks only add targets (`mutants`, `miri`, `miri-setup`), never rename.
- Consumes: `.config/nextest.toml` and `.cargo/config.toml` (`MIRIFLAGS`) exactly as they exist — no edits.

- [ ] **Step 1: Write the Makefile** (spec stages 1–2 merged: test bodies are final-form nextest; no `mutants`/`miri` targets yet — they land in Tasks 2–3)

```make
# Single source of verification commands — the contract referenced by CLAUDE.md
# and .claude/rules/tech.md. Edit freely; keep targets .PHONY and bodies plain.

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-fix clippy doc test test-no-default-features dylint battery \
        dupes bench

## help: list available targets
help:
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## //'

## fmt: check formatting (gate)
fmt:
	cargo fmt --check

## fmt-fix: apply formatting
fmt-fix:
	cargo fmt

## clippy: lint every target, warnings are errors (gate)
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

## doc: build docs, doc warnings are errors (gate)
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

## test: all-features leg — nextest binaries, then doctests (gate)
test:
	cargo nextest run --workspace --all-features
	cargo test --doc --workspace --all-features

## test-no-default-features: no-default-features leg — nextest, then doctests (gate)
test-no-default-features:
	cargo nextest run --workspace --no-default-features
	cargo test --doc --workspace --no-default-features

## dylint: external lint suites via their pinned nightlies (gate)
dylint:
	cargo dylint --all -- --all-targets

## battery: the full pre-done gate (CI parity)
battery: fmt clippy doc test test-no-default-features dylint

## dupes: duplication gate (on demand)
dupes:
	cargo dupes check

## bench: oneshot diagnostics benchmark (on demand)
bench:
	cargo bench --bench oneshot_diagnostics
```

- [ ] **Step 2: Verify the contract works**

Run: `make help`
Expected: every target listed with its `##` description.

Run: `make test` then `make test-no-default-features`
Expected: both legs green — nextest runs the test binaries, then `cargo test --doc` runs doctests. Record the exact pass counts of each leg in the report (they become the plan's reference numbers).

- [ ] **Step 3: Docs sync — targets replace inline commands**

- `tech.md`, "Verification battery": the fenced command block becomes `make battery`, followed by a one-time mapping list (which target runs which command — the only place raw commands remain). The `cargo test <test_name>` line becomes `cargo nextest run <filter>`. The dylint paragraph's command becomes `make dylint`. The dupes paragraph: `make dupes`. The bench paragraph: `make bench`. Add one sentence: nothing heavy runs concurrently with `make mutants` (lands in Task 2).
- `CLAUDE.md`, Commands section: add a first line — local verification is `make battery` (CI parity) and `make help` lists all targets; inline battery command listings are replaced by the target names. Keep the CI description prose as-is (CI itself is untouched).
- `testing.md`: every `cargo dupes check` mention becomes `make dupes` (the tool name in prose stays "the dupes gate").

- [ ] **Step 4: No-drift check**

Run: `rg -n "cargo (fmt|clippy|doc|test|dylint|dupes|bench|nextest|mutants)" CLAUDE.md .claude/rules/`
Expected: hits only in (a) the `tech.md` mapping list, (b) single-test/`nextest run <filter>` prose, (c) CI-description prose in `CLAUDE.md` (describes what CI runs — not a local instruction). No local-command instructions remain.

- [ ] **Step 5: Full gate**

Run: `make battery`
Expected: all six targets green in order.

- [ ] **Step 6: Self-review + report + owner commit**

Self-review the diff (tabs in recipes, `.PHONY` complete, docs consistent), write the report, pause for the owner's commit.

---

### Task 2: mutants on-demand target + baseline snapshot

**Files:**
- Modify: `Makefile` (add `mutants`), `.claude/rules/tech.md` (on-demand paragraph: `make mutants`, exit-code semantics, concurrency rule)
- Commit (owner): `.cargo/mutants.toml` — content unchanged (`test_tool = "nextest"`, `all_features = true`)

**Interfaces:**
- Consumes: Task 1's Makefile and the `tech.md` battery section (mutation of one paragraph, no restructuring).
- Produces: `make mutants` and `make mutants FILE=<path>` for Task 3's and Plan B's scoped verification; the frozen baseline at `.superpowers/mutants-baseline-2026-09-09/` (missed/caught/timeout/unviable lists, `outcomes.json`, `diff/`) that Plan B's disposition table is built from.

- [ ] **Step 1: Freeze the baseline BEFORE any mutants re-run** (every run overwrites `mutants.out/`)

```bash
mkdir -p .superpowers/mutants-baseline-2026-09-09
cp -R mutants.out/caught.txt mutants.out/missed.txt mutants.out/timeout.txt \
      mutants.out/unviable.txt mutants.out/outcomes.json mutants.out/diff \
      .superpowers/mutants-baseline-2026-09-09/
```

- [ ] **Step 2: Add the target** (append to Makefile; add `mutants` to `.PHONY`)

```make
## mutants: mutation-testing sweep (on demand, heavy; run it alone — a concurrent
## build poisons its auto-derived per-scenario timeout). Optional FILE=src/foo.rs
## scopes the sweep to one file. Exit code 1 means survivors were found: this is a
## diagnostic sweep, not a gate — the battery never runs it.
mutants:
	cargo mutants $(if $(FILE),-f $(FILE))
```

- [ ] **Step 3: Scoped verification**

Run: `make mutants FILE=src/documents/matcher.rs`
Expected: only `src/documents/matcher.rs` scenarios run (roughly a dozen; single-file sweep well under five minutes). Known survivors in that file (from the full run): the three `lang_strings` mutants — so **exit code 1 is the expected outcome**. Verify scoping worked: `mutants.out/log/` contains only matcher logs and the summary names no other file.

- [ ] **Step 4: tech.md sync**

On-demand paragraph: `make mutants` joins `make dupes`/`make bench`; state the exit-code semantics (nonzero = survivors reported, it is a diagnostic not a gate) and the concurrency rule from the target comment.

- [ ] **Step 5: Gate + report + owner commit**

Run: `make battery` (Makefile changed). Expected: green. Self-review, report (include the scoped run's observed scenario count), pause for the owner's commit — this commit includes `.cargo/mutants.toml`.

---

### Task 3: Miri cross-interpretation experiment (one-time, decides the branch)

**Files:**
- Modify (branch A only): `Makefile` (add `MIRI_TOOLCHAIN`, `miri`, `miri-setup`), `.claude/rules/tech.md` (miri paragraph)
- Modify (branch B only): `.claude/rules/tech.md` (deferral note), `docs/superpowers/specs/2026-09-09-test-tooling-integration-design.md` (addendum recording the outcome)
- No source files change in either branch.

**Interfaces:**
- Consumes: Task 1's Makefile; nextest `default-miri` profile (observed working); `MIRIFLAGS` from `.cargo/config.toml`.
- Produces: either the `make miri` / `make miri-setup` targets (branch A) or the recorded deferral with the exact blocking operation (branch B) — both feed Plan B's scope note and the later CI cycle.

**Decision procedure (spec D5):** the experiment's raw commands are run once, by hand; targets are added only on branch A.

- [ ] **Step 1: Install the interpreter and record its date**

```bash
rustup toolchain install nightly
rustc +nightly --version        # record the full version line incl. date
rustup component add miri --toolchain nightly
```

The recorded date (e.g. `nightly-2026-09-03`) is the pin value if branch A is taken.

- [ ] **Step 2: Host smoke — toolchain functional on the known-good subset**

Run: `cargo +nightly miri nextest run --no-default-features error`
Expected: the `error::tests` (non-Rope) tests pass under interpretation. Rope-dependent tests are NOT run here (known NEON blocker). If this smoke fails for a non-blocker reason, investigate before proceeding — do not work around.

- [ ] **Step 3: Cross-interpretation setup + the experiment**

```bash
rustup target add x86_64-apple-darwin --toolchain nightly
cargo +nightly miri setup --target x86_64-apple-darwin
cargo +nightly miri nextest run --target x86_64-apple-darwin --no-default-features
```

Read the outcome by failure type, not by pass/fail alone:
- **Branch A — interprets:** rope-dependent tests execute (fast or slow is irrelevant). Remaining failures split into (i) genuine Miri UB findings — record each as a Plan B candidate (D6: real defects, root-caused), (ii) flaky/environmental. At least one Rope-building test passing under interpretation is the branch-A criterion.
- **Branch B — still unsupported:** failures are again `unsupported operation` (an x86 SIMD shim or other foreign/intrinsic call that Miri lacks). Record the exact operation name from the error.

- [ ] **Step 4a (branch A): Add the targets** (append to Makefile; add `miri miri-setup` to `.PHONY`)

```make
# Toolchains (dated pins; upgrades are deliberate, never casual).
MIRI_TOOLCHAIN ?= nightly-<the date recorded in Step 1>

## miri: UB interpreter over the no-default-features leg, cross-interpreted for
## x86_64 (the ropey/str_indices NEON path is not interpretable on an aarch64 host);
## on demand, slow — never in the battery
miri:
	cargo +$(MIRI_TOOLCHAIN) miri nextest run --target x86_64-apple-darwin --no-default-features

## miri-setup: one-time toolchain preparation for make miri
miri-setup:
	rustup component add miri --toolchain $(MIRI_TOOLCHAIN)
	rustup target add x86_64-apple-darwin --toolchain $(MIRI_TOOLCHAIN)
	cargo +$(MIRI_TOOLCHAIN) miri setup --target x86_64-apple-darwin
```

Then `make miri-setup` (idempotent re-verification) and `make miri` must both run. `tech.md` gains the miri paragraph (what it covers: no-default leg only, why cross-target, on-demand).

- [ ] **Step 4b (branch B): Record the deferral**

No Makefile targets. Add a short addendum to the spec under D5: the experiment's date, the exact unsupported operation observed, and the upstream blockers (Miri NEON/x86-shim coverage, str_indices no-SIMD option). `tech.md`'s lint-stack area gains one sentence: Miri evaluated 2026-09-09, deferred with the blocker recorded in the spec.

- [ ] **Step 5: Gate + report + owner commit**

Run: `make battery` (Makefile changed only on branch A; run it either way as the plan's final gate). Report must state: the branch taken, the evidence (passing Rope test name / unsupported-op string), and — on branch A — any UB findings queued for Plan B. Pause for the owner's commit.

---

## Self-Review (performed at plan time)

1. **Spec coverage:** stages 1–3 → Tasks 1–3 (stage 1+2 merged into Task 1 — one file, one coherent contract; the split would be same-file churn). Stage 4 (disposition) is deliberately absent: Plan B per spec D7. D5's two-branch outcome is written as explicit divergent steps. ✓
2. **Placeholder scan:** the only execution-time value is `MIRI_TOOLCHAIN`'s date, produced by Task 3 Step 1's recorded command output — not a TBD. All file contents and commands are complete. ✓
3. **Consistency:** target names identical across Tasks 1–3 and the docs-sync instructions; Task 2's expected survivors (`lang_strings` ×3) match the 2026-09-09 run's `missed.txt`; baseline snapshot precedes any overwrite. ✓

## Execution Handoff

Subagent-driven (recommended — fresh implementer per task on sonnet[1m], two-verdict review per task, owner commits at each pause) or inline execution. Plan B is written after this plan's tasks land and the owner commits them.
