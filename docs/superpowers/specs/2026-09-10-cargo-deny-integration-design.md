# cargo-deny integration — design

**Date:** 2026-09-10
**Status:** approved design, pre-implementation
**Inputs:** owner decisions 2026-09-10 (in session): bans duplicated-versions deny with
full triage; advisories vulns-deny / informational-warn; standard license allow-list +
crates.io-only sources; `make deny` on-demand, battery untouched until the CI cycle.
Closes the registered follow-up from the DyLint cycle
(`docs/superpowers/specs/2026-09-07-dylint-integration-design.md`, Out of scope).
**Branch:** `feature/nextest` (owner: all work on the current branch)
**Environment fact:** cargo-deny 0.20.2 installed; dependency tree 194 packages,
2 workspace members.

## Goal

One committed `deny.toml` expressing the dependency-hygiene policies, a `make deny`
on-demand target in the Makefile contract, and a first-run triage in which every
deny-level finding is either fixed (dependency upgrade) or carries a reasoned exception
— fix or accept-with-reason, never threshold-loosening or unexplained suppression.

## Decisions

- **D1 — Policies (owner, 2026-09-10).** `[advisories]`: vulnerability-class = deny;
  unmaintained/yanked/informational = warn. `[bans]`: `multiple-versions = "deny"`,
  `wildcards = "deny"`. `[licenses]`: allow MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
  ISC, Unicode-3.0, Zlib; `unused-allowed-license = "warn"` keeps the list honest.
  `[sources]`: crates.io only; unknown registry/source = deny.
  *Amended 2026-09-10, post-review:* the owner trimmed the license list to the three
  actually-encountered (MIT, Apache-2.0, Unicode-3.0) — the four unused allowances
  warned; a minimal list deny-gates any future non-listed license into a conscious
  list-addition instead. Schema drift vs the original D1 wording (`unmaintained` scope
  semantics, dropped `vulnerability`/`severity-threshold` keys, `unknown-git` spelling)
  is recorded in `deny.toml` comments and the task report.
- **D2 — Exception discipline.** Every exemption lands in `deny.toml` via the tool's
  native mechanism (`[advisories] ignore`, `[[bans.skip]]`, `[[licenses.exceptions]]`)
  with a reason comment in the `.dupes-ignore.toml` style: what fires, why it is
  accepted, what would remove it. Levels are never lowered to silence a finding; an
  unresolvable or judgment-call finding escalates to the owner with a worked proposal
  (what fired / the options / trade-offs), same protocol as the mutants cycle.
- **D3 — Placement: on-demand, not battery.** `make deny` joins `dupes`/`mutants`/
  `bench`/`miri` as an on-demand target. The battery stays CI-parity (D2 of the
  test-tooling spec); when the later CI cycle lands, `deny` joins battery and CI
  together in one step.
- **D4 — Makefile contract.** Body: `$(CARGO_BIN) deny check` (rtk-routed locally,
  plain under future CI); `.PHONY` entry; one-line `##` help comment per the file's
  convention.

## Design

### 1. First-run triage (the core of the cycle)

Run `cargo deny check`, collect the full report, disposition every deny-level finding:
fix (upgrade the responsible dependency — Dependabot-compatible) or reasoned exception
per D2. Expected hot zone: duplicated crate versions (the inherited
`multiple_crate_versions` clippy allowance documents that duplicates exist). Warn-level
findings are surfaced and either resolved or recorded with reasons — visible, not
gating.

### 2. Docs sync

`tech.md` on-demand paragraph gains `make deny` (target, exit semantics: deny-level
findings gate, warnings advise); `CLAUDE.md` Commands gains the target; the DyLint
spec's out-of-scope record gets a "landed 2026-09-10" note; the SDD ledger closes the
registered follow-up.

## Error handling

- cargo-deny's advisory DB fetch requires network; a failed fetch is an environment
  error, reported as such (not silently retried or worked around in config).
- A finding that is neither cleanly fixable nor cleanly acceptable escalates to the
  owner (D2) — never a default decision by the implementer.

## Out of scope

- CI wiring and battery membership (the later CI cycle, per D3).
- `cargo modules`/`cargo machete` — the rest of the unlanded 2026-08-30 step 5.

## Acceptance

- `make deny` exits 0: zero deny-level findings; every warn-level finding resolved or
  reason-recorded.
- Every exception in `deny.toml` carries a reason; no level lowered to silence a
  finding.
- Makefile contract extended per D4; `tech.md`/`CLAUDE.md` synced; battery green.
- The DyLint spec's registered follow-up closed in the ledger.
