# CI/CD Pipeline Rework — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the serial `rust.yml` job with a cached, staged `ci.yml` — parallel checks + two-leg test matrix, then a main-only release line — plus the shared `rust-setup` composite action and the docs battery sync.

**Architecture:** One workflow, four jobs, two permission tiers: `checks` (fmt/clippy/doc) and `test` (matrix: `--all-features`, `--no-default-features`) run in parallel through `.github/actions/rust-setup` (checkout + `rust-cache@v2.9.2`, saves on `main` only); `release` (`needs: [checks, test]`, `if` main+push, `contents: write`) runs the tag/changelog/gh-release trio. `rust.yml` is deleted; CLAUDE.md/tech.md/testing.md battery text is rewritten in the same cycle.

**Tech Stack:** GitHub Actions (workflow + composite action), `actions/checkout@v4`, `Swatinem/rust-cache@v2.9.2`, `anothrNick/github-tag-action@v1`, `loopwerk/tag-changelog@v1`, `softprops/action-gh-release@v2`.

**Spec:** `docs/superpowers/specs/2026-09-07-cicd-pipeline-rework-design.md` · **Research (cache choice):** `docs/superpowers/research/2026-09-07-rust-cache-vs-actions-cache-research.md`

## Global Constraints

- **No git commands, no commits** — every task ends with a *checkpoint* (file group); the owner commits. Branch: `feature/ci_cd`.
- **No Rust code changes** in this cycle — workflow YAML and docs only. The cargo battery is not part of verification (nothing compiles differently); do not "fix" Rust files.
- **Exact action versions, copied verbatim:** `actions/checkout@v4`, `Swatinem/rust-cache@v2.9.2` (never the floating `v2`), `anothrNick/github-tag-action@v1`, `loopwerk/tag-changelog@v1`, `softprops/action-gh-release@v2`.
- Composite-action inputs are named exactly `fetch-depth` and `cache`; the workflow references the action as `./.github/actions/rust-setup`.
- YAML hazard: `#major`-style env values MUST stay single-quoted or YAML eats them as comments.
- English artifacts; workflow comments minimal and load-bearing.
- Deleting `rust.yml` is a working-tree deletion (`rm`); the owner's commit records it. Never run `git rm`.

## File Structure

| file | responsibility | task |
|---|---|---|
| `.github/actions/rust-setup/action.yml` | composite: checkout + Cargo cache (shared setup) | 1 |
| `.github/workflows/ci.yml` | triggers, concurrency, permissions, all four jobs | 2 |
| `.github/workflows/rust.yml` | deleted (superseded by `ci.yml`) | 2 |
| `CLAUDE.md` | Commands bullet: new CI description | 3 |
| `.claude/rules/tech.md` | Verification battery rewrite + `rust.yml`→`ci.yml` ref | 3 |
| `.claude/rules/testing.md` | "All three feature configurations" → current legs | 3 |

---

### Task 1: `rust-setup` composite action

**Files:**
- Create: `.github/actions/rust-setup/action.yml`

**Interfaces:**
- Produces (consumed by Task 2): composite action at `./.github/actions/rust-setup` with inputs `fetch-depth` (string, default `'1'`) and `cache` (string, default `'true'`); performs `actions/checkout@v4` with the given depth and, when `cache == 'true'`, `Swatinem/rust-cache@v2.9.2` with main-only saves.

- [ ] **Step 1: Write the action**

```yaml
name: Rust setup
description: Checkout plus Cargo build caching (Swatinem/rust-cache; cache saves on main only).

inputs:
  fetch-depth:
    description: Checkout fetch depth; pass 0 for full history (release line).
    default: '1'
  cache:
    description: Whether to set up the Cargo build cache; false for jobs that compile nothing.
    default: 'true'

runs:
  using: composite
  steps:
    - name: Checkout repository
      uses: actions/checkout@v4
      with:
        fetch-depth: ${{ inputs.fetch-depth }}

    - name: Cache cargo build
      if: ${{ inputs.cache == 'true' }}
      uses: Swatinem/rust-cache@v2.9.2
      with:
        save-if: ${{ github.ref == 'refs/heads/main' }}
```

- [ ] **Step 2: Verify parse + content**

Run: `ruby -ryaml -e 'YAML.load_file(".github/actions/rust-setup/action.yml") or raise; puts "parse ok"'`
Expected: `parse ok` (if ruby is unavailable, use any local YAML parser; syntax check is the floor).

Run: `grep -c 'rust-cache@v2.9.2\|inputs.cache\|inputs.fetch-depth' .github/actions/rust-setup/action.yml`
Expected: `3` (pinned version present, both inputs consumed).

- [ ] **Step 3: Checkpoint** — one file. **Pause for the owner's commit.**

---

### Task 2: `ci.yml` (checks, test matrix, release)

**Files:**
- Create: `.github/workflows/ci.yml`
- Delete: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: Task 1's `./.github/actions/rust-setup` (`fetch-depth`, `cache` inputs).
- Produces: jobs `checks`, `test`, `release` — names referenced by Task 3's docs text.

- [ ] **Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always

jobs:
  checks:
    runs-on: ubuntu-latest
    steps:
      - uses: ./.github/actions/rust-setup
      - name: Format
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Docs
        run: cargo doc --workspace --no-deps
        env:
          RUSTDOCFLAGS: "-D warnings"

  test:
    runs-on: ubuntu-latest
    name: test (${{ matrix.features }})
    strategy:
      matrix:
        features: ["--all-features", "--no-default-features"]
    steps:
      - uses: ./.github/actions/rust-setup
      - name: Test (${{ matrix.features }})
        run: cargo test --workspace ${{ matrix.features }} --verbose

  release:
    needs: [checks, test]
    if: github.ref == 'refs/heads/main' && github.event_name == 'push'
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: ./.github/actions/rust-setup
        with:
          fetch-depth: '0'
          cache: 'false'
      - name: Bump version and push tag
        uses: anothrNick/github-tag-action@v1
        id: bump
        env:
          GITHUB_TOKEN: ${{ github.token }}
          MAJOR_STRING_TOKEN: '#major'
          MINOR_STRING_TOKEN: '#minor'
          PATCH_STRING_TOKEN: '#patch'
          NONE_STRING_TOKEN: '#none'
          TAG_PREFIX: v
          VERBOSE: false
      - name: Build changelog
        id: changelog
        uses: loopwerk/tag-changelog@v1
        with:
          token: ${{ github.token }}
      - name: Release
        uses: softprops/action-gh-release@v2
        with:
          token: ${{ github.token }}
          generate_release_notes: true
          append_body: true
          make_latest: true
          tag_name: ${{ steps.bump.outputs.tag }}
          name: ${{ steps.bump.outputs.tag }}
          body: ${{ steps.changelog.outputs.changelog }}
```

- [ ] **Step 2: Delete the superseded workflow**

Run: `rm .github/workflows/rust.yml`
(`.github/workflows/` must contain exactly `ci.yml` afterwards.)

- [ ] **Step 3: Verify parse + content**

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml") or raise; puts "parse ok"'`
Expected: `parse ok`.

Run: `grep -c 'workflow_dispatch\|cancel-in-progress\|needs: \[checks, test\]\|contents: write\|--no-default-features\|github-tag-action@v1\|tag-changelog@v1\|action-gh-release@v2' .github/workflows/ci.yml`
Expected: `8`.

Run: `ls .github/workflows/`
Expected: `ci.yml` only.

- [ ] **Step 4: Checkpoint** — `ci.yml` added, `rust.yml` deleted. **Pause for the owner's commit.**

---

### Task 3: Docs battery sync

**Files:**
- Modify: `CLAUDE.md` (Commands, first bullet)
- Modify: `.claude/rules/tech.md` (Verification battery)
- Modify: `.claude/rules/testing.md` (feature-configurations sentence)

**Interfaces:**
- Consumes: Task 2's job names (`checks`, `test`) and file name `ci.yml`.
- Produces: none.

- [ ] **Step 1: CLAUDE.md Commands bullet** — replace the first bullet of the Commands section (the one starting `CI runs the full battery`) with:

```markdown
- CI (`.github/workflows/ci.yml`) runs on push/PR to `main` and via `workflow_dispatch` on any branch: a `checks` job (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`) plus a two-leg test matrix (`cargo test --workspace --all-features` and `--no-default-features`) in parallel, with Cargo caching (`rust-cache`, saves on `main` only); on pushes to `main`, a release job tags (`v` prefix; `#major`/`#minor`/`#patch`/`#none` commit-message tokens) and publishes a GitHub Release with changelog. The `default` test leg returns when a second, non-default feature exists (today `default` ≡ `--all-features`)
```

- [ ] **Step 2: tech.md Verification battery** — replace the intro sentence and the fenced block with:

````markdown
Before considering work done, run the same battery CI runs
(`.github/workflows/ci.yml`, on push/PR to `main` and via `workflow_dispatch`):

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The standalone `cargo build --workspace --all-targets` step is gone:
nothing is published, and `clippy --all-targets` compiles every target.
The `default` configuration is currently identical to `--all-features`
(the crate has one feature, on by default) and returns as a third leg the
day a non-default feature lands.
````

- [ ] **Step 3: tech.md gated-path sentence** — in the Feature gates section, replace `verify at least \`cargo test --no-default-features\` in addition to the default configuration` with `verify at least \`cargo test --workspace --no-default-features\` in addition to \`cargo test --workspace --all-features\``.

- [ ] **Step 4: testing.md sentence** — in Conventions, replace `All three feature configurations must compile and pass.` with `Every feature configuration CI runs must compile and pass — \`--all-features\` and \`--no-default-features\` (plus \`default\` again once a non-default feature exists).`

- [ ] **Step 5: Stale-text sweep** — run, expect 0 hits each:

```bash
grep -rn "three feature configurations\|three feature configs" CLAUDE.md .claude/rules/
grep -rn "cargo build --workspace --all-targets" CLAUDE.md .claude/rules/
grep -rn "rust\.yml" CLAUDE.md .claude/rules/
```

(`--all-targets` alone remains legitimate inside the clippy lines — do not touch those.)

- [ ] **Step 6: Checkpoint** — three docs files. **Pause for the owner's commit.**

---

### Task 4: Final verification + rehearsal handoff

- [ ] **Step 1:** Re-run both YAML parses (Task 1/2 commands) and the three Task 3 greps — all green.
- [ ] **Step 2:** Report the rehearsal procedure for the owner (the pipeline is its own test; the agent cannot push):
  1. Push `feature/ci_cd`; in the Actions tab run **CI → Run workflow** on the branch — expect `checks` + both `test` legs green, no `release` job, cache restore only.
  2. After merge to `main`: first push runs the full line; expect a `v`-prefixed tag, a GitHub Release with generated notes and the tag-changelog body. The first bump starts from the tag action's default initial version when no previous tag exists — verify the actual tag value on that first run.
- [ ] **Step 3: Checkpoint** — nothing new to commit (verification only); hand the report over. Final whole-branch review follows per SDD.

---

## Self-review notes

- Spec coverage: components 1 (ci.yml) → T2; 2 (checks) → T2; 3 (test matrix) → T2; 4 (release) → T2; 5 (rust-setup) → T1; trigger matrix → T2 YAML (`workflow_dispatch`, `if` gate, `save-if` in T1); docs sync → T3 (spec named CLAUDE.md + tech.md; testing.md's "all three configurations" sentence is a stale-text consequence — included in T3 step 4 and covered by the spec's "docs stay in sync" constraint); testing/rehearsal → T4. Rejected alternatives need no tasks.
- Placeholder scan: every step carries full content; no TBD/TODO.
- Interface consistency: input names `fetch-depth`/`cache` identical in T1 definition and T2 usage; job names `checks`/`test`/`release` identical in T2 and T3; `rust-cache@v2.9.2` pinned in T1 only (single cache step).
