# Test-tooling integration — Makefile · nextest · cargo-mutants · Miri — design

**Date:** 2026-09-09
**Status:** approved design, pre-implementation
**Inputs:** owner directives 2026-09-09 (in session): Makefile as the single command
source, staged tool adoption with no CI changes this cycle, disposition of all mutant
survivors as the final stage, "Type first, test second" reaffirmed, rust-skills + LSP
consultation mandatory per finding; completed cargo-mutants run (`mutants.out/` on the
working tree, 2026-09-09); empirical Miri run (aarch64 macOS, 2026-09-09);
`.claude/rules/{testing,tech,structure}.md`
**Branch:** `develop` (owner decision 2026-09-09: all work lands on the current branch)

## Goal

One local entry point for every verification command (a hand-editable root `Makefile`),
cargo-nextest as the test runner inside it, cargo-mutants and Miri as committed on-demand
tooling, and a complete, reasoned disposition of all 149 surviving mutants — measured
against the crate's behavioral contracts, not against a kill-rate. The end state is
"fewer bugs while preserving business logic", not "the tool is satisfied". CI/CD wiring
of the new stack is explicitly a later cycle.

## Verified starting facts (2026-09-09, session-observed)

- **cargo-mutants 27.1.0 run completed** (config `.cargo/mutants.toml`: `test_tool =
  "nextest"`, `all_features = true`): 859 mutants — **459 caught, 149 missed, 12
  timeouts, 239 unviable** (~74% kill rate of viable mutants). Inventory:
  `mutants.out/missed.txt`; per-mutant patches: `mutants.out/diff/` (859 unified diffs);
  full outcomes: `mutants.out/outcomes.json`.
- **Survivor clusters** (149, by file): text_utils ~50 (encoding/position `From` impls,
  RangeExt arithmetic in all three flavors); tree-sitter navigation ~16
  (`find_child/ancestor/descendant/nearest*`, `Document::node_at_*`); Document accessors
  + `DocumentReader::read` offset math ~18; workspace/diagnostics gating state machine
  17; oneshot 11; server trait defaults/options/state ~10; state/documents didChange
  internals 10; lsp_requests conversions 10; matcher 3.
- **Doctest blind spot:** nextest does not run doctests, so doctest-pinned behavior
  surfaces as "missed". Doctests must stay in the battery via `cargo test --doc`.
- **Timeout caveat:** a `cargo +nightly miri nextest run` executed concurrently during
  part of the mutants window; the 12 timeouts are unverified until re-run in isolation.
- **Miri is non-viable on this machine for both feature legs as-is (aarch64 macOS):**
  1. default leg: `can't call foreign function tree_sitter_json` (extern "C" is not
     interpreted);
  2. no-default leg: `can't call LLVM intrinsic llvm.aarch64.neon.uaddlv` in
     ropey → str_indices on every `Rope` construction — only non-Rope tests (e.g.
     `error::tests`) survive.
  Unverified escape hatch: cross-interpretation with `--target x86_64-apple-darwin`
  (SSE2 path + Miri's partial x86 SIMD shims). Upstream blockers: NEON support in
  Miri, or a no-SIMD option in str_indices/ropey.
- **Mutation testing measures test sensitivity, not correctness.** A survivor is a
  question — "does any behavioral contract care about this?" — never a defect report.
  (cargo-mutants is dynamic, not static: it mutates code and runs the real suite; what
  it reports is which behavior changes no test observes.)

## Decisions

- **D1 — Makefile first, single source of commands.** A flat, hand-editable root
  `Makefile`; every check is a `.PHONY` target; toolchain pins live in variables at the
  top; `make help` lists targets. After it lands, no project artifact (CLAUDE.md,
  `.claude/rules/tech.md`, subagent briefs) inlines raw cargo commands — only target
  names. Questions like "with or without `+nightly`" become edits to one variable.
- **D2 — `make battery` = full CI parity.** `fmt clippy doc test
  test-no-default-features dylint`. `mutants`, `dupes`, `miri`, `bench` are on-demand
  targets, never in the battery. (Owner decision 2026-09-09.)
- **D3 — nextest is adopted inside the targets, CI untouched.** `test` /
  `test-no-default-features` bodies move to `cargo nextest run` + a separate
  `cargo test --doc` step per leg. The already-present `.config/nextest.toml`
  (`[profile.ci] fail-fast = false`) and `MIRIFLAGS` in `.cargo/config.toml` are part of
  the contract as-is. CI keeps its raw commands this cycle; the drift is accepted and
  closed by the later CI cycle, which can invoke the same targets.
- **D4 — mutants is committed, on-demand, with an operational rule.** `.cargo/mutants.toml`
  is committed unchanged (default knobs: jobs auto, timeout auto-derived from the
  baseline run). `make mutants` accepts an optional `FILE=` passthrough
  (`cargo mutants -f <file>`) for per-cluster verification. Rule from live experience:
  nothing heavy runs concurrently with a mutants sweep — the auto-timeout (observed
  23 s) is poisoned by competing builds. `mutants.out/` is gitignored scratch,
  overwritten by every run; the durable artifact is the disposition table embedded in
  plan B.
- **D5 — Miri: dated toolchain pin + in-cycle experiment, two-branch outcome.**
  `MIRI_TOOLCHAIN ?= <dated nightly>` (chosen at implementation as the newest nightly
  whose miri component runs the non-Rope subset green; upgrades deliberate — same
  discipline as dylint's pinned nightly). The experiment:
  `rustup target add x86_64-apple-darwin` on that toolchain, then
  `cargo +$(MIRI_TOOLCHAIN) miri nextest run --target x86_64-apple-darwin
  --no-default-features`. Branch (a): green → `make miri` pins that configuration
  (on-demand; interpretation is slow, never in the battery). Branch (b): x86 SIMD
  shims insufficient → Miri is deferred: no `miri`/`miri-setup` targets in the final
  Makefile, the experiment result and upstream blockers are recorded here and in
  `tech.md`. Note: cross-interpretation only addresses the NEON blocker; the
  tree-sitter FFI blocker keeps the default leg out of scope for Miri either way.
- **D6 — Disposition principle: contract-first, never tool-first.** Every survivor is
  dispositioned against the component's behavioral contract (module docs, the
  three-place pattern contracts in `structure.md`, callers found via LSP — not grep),
  with rust-skills consulted for the change class (`test-*` for pin tests, `api-*`/
  `type-*` for type-first changes, `err-*` where errors are touched). Exactly one
  disposition per survivor: **(a)** type-first change (removes a representable invalid
  state or confusable pair), **(b)** pin test at the lowest tier that can see it (W0 or
  wire), **(c)** doctest-covered (the doctest is named), **(d)** accepted with a reason
  that cites the contract (e.g. "accessor of a construction-immutable handle; no
  behavioral contract exists") — never a reason of tool convenience. (d) is an expected
  mass outcome, not an exception. Miri findings (UB reports) are a different
  epistemic class — actual defects, root-caused per `no-workarounds`, with
  business-logic context determining *where* the fix lands, not *whether*.
- **D7 — One spec, two plans.** Plan A: infrastructure (Makefile → nextest →
  mutants/Miri targets + experiment). Plan B: disposition, written after A lands, its
  tasks shaped by the final target inventory. The pause between them is deliberate.
- **D8 — No numeric kill-rate target, ever.** The done-bar is the complete table plus
  a verified re-run, per D6 and the testing philosophy (`no tests for coverage
  statistics`).

## Design

### 1. Makefile contract (stage 1)

| target | body | kind |
|---|---|---|
| `help` | list targets from `## ` comments | convenience |
| `fmt` / `fmt-fix` | `cargo fmt --check` / `cargo fmt` | gate / apply |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | gate |
| `doc` | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | gate |
| `test` | stage 1: `cargo test --workspace --all-features`; stage 2: `cargo nextest run --workspace --all-features` + `cargo test --doc --workspace --all-features` | gate |
| `test-no-default-features` | same for `--no-default-features` | gate |
| `dylint` | `cargo dylint --all -- --all-targets` (from repo root) | gate |
| `battery` | `fmt clippy doc test test-no-default-features dylint` | aggregate |
| `dupes` | `cargo dupes check` | on-demand |
| `mutants` | `cargo mutants $(FILE-passthrough)` | on-demand |
| `miri` / `miri-setup` | per D5 branch | on-demand / helper |
| `bench` | `cargo bench --bench oneshot_diagnostics` | on-demand |

Conventions: all `.PHONY`; no file targets; no include machinery; variables only for
toolchains (`MIRI_TOOLCHAIN`); plain recipes. Docs sync lands in the same stage:
`CLAUDE.md` Commands and `tech.md` battery point at targets (single-test invocations
stay prose: `cargo nextest run <filter>`); `testing.md`'s dupes command becomes
`make dupes`.

### 2. nextest adoption (stage 2)

Rewrite the two test targets' bodies; doctests become an explicit step inside each leg
(nextest does not run them). No new mechanisms; profiles already exist in
`.config/nextest.toml`. Both feature legs green under nextest + doctests is the stage
exit.

### 3. mutants + Miri (stage 3)

Commit `.cargo/mutants.toml`; add `mutants` target with `FILE=` passthrough; add
`miri`/`miri-setup` per the D5 experiment outcome. The experiment runs once, its result
is recorded in this spec (addendum) and `tech.md`.

### 4. Disposition (stage 4, plan B)

Procedure per cluster, in order: LSP verification of the component (references,
callers, targeted reads of the contract) → rust-skills consultation for the change
class → disposition decision with a contract-citing reason → kill-verification for
(a)/(b): apply the specific `mutants.out/diff/<mutant>.diff`, run the one test via
nextest, observe the failure, revert. Before the table: re-run the 12 timeouts in
isolation (`make mutants FILE=...` or `-F` filters) and drop any that were artifacts of
the concurrent Miri run. The table is keyed by cluster + function (line numbers drift
with every code change). Final full `make mutants` is the acceptance gate: zero live
mutants from categories (a)/(b); (c)/(d) may survive legitimately.

## Error handling

- Miri experiment failure is an expected branch (D5-b), not a blocker; it produces a
  documented deferral, not a workaround.
- Timeout artifacts are handled by verification, not by assumption (12 re-runs first).
- A survivor that resists clean disposition (genuinely ambiguous contract) escalates to
  the owner as a design question, not a default (d).

## Out of scope

- CI/CD wiring of Makefile/nextest/mutants/Miri — later cycle (CI keeps raw commands).
- `cargo-deny` — registered follow-up, unchanged.
- Any kill-rate or coverage-statistics targets (D8).
- Upstream work: Miri NEON shims, str_indices no-SIMD option — watched, not done here.

## Acceptance

- `make help` works; `make battery` runs the full CI-parity set green on the current
  tree.
- Both feature legs green under nextest with doctests still running.
- `.cargo/mutants.toml` committed; `make mutants` (and `FILE=` passthrough) works.
- Miri: either a pinned working `make miri` configuration or a recorded deferral with
  upstream blockers in this spec and `tech.md`.
- Disposition table complete: 149/149 (+ verified timeout outcomes), each row carrying
  its disposition and a contract-citing reason; zero live (a)/(b) mutants in the final
  `make mutants` re-run.
- `CLAUDE.md` / `tech.md` / `testing.md` point at targets; no raw battery commands
  inlined in artifacts.
- Full `make battery` green at cycle close.

## Provenance

Owner decisions 2026-09-09 (in session): Makefile as the first step and single command
source (agent runs legitimate targets instead of memorizing commands and flags);
battery = CI parity including dylint; Miri x86_64 experiment in-cycle; done-bar =
disposition table + verified re-run, no numeric kill target; one spec with two plans;
mutants findings verified against business logic per component with rust-skills and LSP
consultation ("my goal is fewer bugs while preserving business logic, not satisfying
the linter"); cargo-mutants mischaracterized as static in discussion — corrected here:
it is dynamic mutation testing whose findings are questions about test sensitivity.
