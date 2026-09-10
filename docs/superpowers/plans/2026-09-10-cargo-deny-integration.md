# cargo-deny Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land `deny.toml` with the owner-ratified policies, the `make deny` on-demand target, and a first-run triage that closes every deny-level finding — fix or reasoned exception. Spec: `docs/superpowers/specs/2026-09-10-cargo-deny-integration-design.md`.

**Architecture:** One config file (policies per spec D1), one Makefile target (D4), one triage pass whose evidence lives in the report and whose exceptions live in `deny.toml` with reasons (D2), one docs-sync step. On-demand placement; battery untouched (D3).

**Tech Stack:** cargo-deny 0.20.2 (installed), GNU make, existing Makefile conventions (`$(CARGO_BIN)` routing, `##` one-line help, `.PHONY`).

## Global Constraints

- Spec binding (D1–D4). Branch `feature/nextest`. **No git commands — the owner commits.** English artifacts. Makefile recipes tab-indented.
- Exception discipline (D2): every exemption carries a reason in `.dupes-ignore.toml` style; levels never lowered to silence a finding; judgment-call findings escalate to the owner with a worked proposal (what fired / options / trade-offs) — the mutants-cycle protocol.
- `make battery` green at every task boundary (the cycle adds no battery members).
- Advisory DB fetch needs network; a fetch failure is reported as an environment error, never worked around in config.

---

### Task 1: deny.toml + make deny target + first-run triage + docs sync

**Files:**
- Create: `deny.toml` (policies per spec D1, byte-exact below)
- Modify: `Makefile` (add `deny` target + `.PHONY`), `.claude/rules/tech.md` (on-demand paragraph), `CLAUDE.md` (Commands), `docs/superpowers/specs/2026-09-07-dylint-integration-design.md` (out-of-scope note → landed)
- Modify only as triage output: `deny.toml` (reasoned exceptions)

**Interfaces:**
- Produces: `make deny` (exit 0 = no deny-level findings) consumed by the future CI cycle; the exception inventory in `deny.toml`.

- [ ] **Step 1: Write `deny.toml`** (exact initial content)

```toml
# Dependency-hygiene policies (spec: docs/superpowers/specs/2026-09-10-cargo-deny-integration-design.md).
# Run: make deny. Deny-level findings gate; warnings advise. Every exception carries
# a reason: what fires, why it is accepted, what would remove it.

[advisories]
version = 2
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
severity-threshold = "low"

[bans]
multiple-versions = "deny"
wildcards = "deny"
skip = []

[licenses]
version = 2
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
]
unused-allowed-license = "warn"

[sources]
unknown-registry = "deny"
unknown-source = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 2: First run** — `cargo deny check` (raw, to see the full report before any exemptions exist). Record every finding by section and level in the report file. Expect: `multiple-versions` violations (the inherited `multiple_crate_versions` debt), possibly license gaps, possibly unmaintained warns.

- [ ] **Step 3: Add the target** (read the Makefile first; match current formatting; extend the `.PHONY` continuation line)

```make
## deny: dependency policies — advisories, duplicate bans, licenses, sources (on demand; deny-level findings gate)
deny:
	@$(CARGO_BIN) deny check
```

- [ ] **Step 4: Triage every deny-level finding** — fix (dependency upgrade via `cargo update -p <crate>` where compatible with `Cargo.lock`-committed policy and both test legs) or reasoned exception per D2. For duplicates: prefer upgrading the shared dependent; when a duplicate is structurally unavoidable (two direct deps each pin different majors), one `[[bans.skip]]` entry naming both versions with the reason and the removal condition. Escalate judgment calls to the owner with worked proposals — do not self-decide.
- [ ] **Step 5: Verify** — `make deny` exits 0; `make battery` green (upgrades can change behavior); `make dupes` unaffected but run it if any source file changed (should not).
- [ ] **Step 6: Docs sync** — `tech.md` on-demand paragraph gains `make deny` (deny-level gates, warns advise); `CLAUDE.md` Commands gains the target; the DyLint spec's out-of-scope cargo-deny bullet gains "(landed 2026-09-10 — see docs/superpowers/specs/2026-09-10-cargo-deny-integration-design.md)". Self-review, report, owner commit.

---

## Self-Review (plan time)

1. **Spec coverage:** D1 → Step 1 (byte-exact policies); D2 → Step 4 + Global Constraints; D3 → target not in battery; D4 → Step 3 shape; triage core → Steps 2–4; docs sync → Step 6; acceptance → Step 5. ✓
2. **Placeholder scan:** none — the only execution-time content (finding list, exception set) is defined by the triage procedure itself, which is stepwise and complete. ✓
3. **Consistency:** target name `deny` matches across Steps 3/5/6 and the spec's D4; exit semantics consistent (deny gates, warns advise). ✓

## Execution Handoff

Subagent-driven recommended (one implementer task on the standing protocol: LSP-first, rust-skills, no git mutations, owner commits at the pause; one reviewer with replay/verification mandate). Small enough for a single task.
