# CI/CD pipeline rework — Design

**Cycle opened:** 2026-09-07 · **Design finalized:** 2026-09-07 · **Branch:**
`feature/ci_cd` (the owner branches per feature)
**Inputs:** research
`docs/superpowers/research/2026-09-07-rust-cache-vs-actions-cache-research.md`
(sonnet[1m], 18 cited sources); the owner's reference workflows
(`email-listener/.github/workflows/npm-publish-github-packages.yml` for the
release trio, `shared-workflows/.github` for the composite-action pattern);
the current `.github/workflows/rust.yml`; `Cargo.toml [features]`.
**Status:** sections presented and approved in brainstorm (owner, 2026-09-07);
all five design choices resolved through AskUserQuestion rounds.

## Goal

Replace the single serial CI job with a staged, cached pipeline: parallel
`checks` (fmt, clippy, doc) and a two-leg test matrix, then — only on pushes
to `main` — a release line that bumps a tag, builds a changelog, and
publishes a GitHub Release. Compilation is cached with `rust-cache@v2.9.2`;
the two redundant runs disappear (the `cargo build --all-targets` step and
the `default` test configuration, which is identical to `--all-features`
while the crate has exactly one feature). Nothing is published to crates.io;
tags and Releases exist to mark revisions for git-dependency pinning.

## Owner decisions log

1. **CodeQL is out of scope** (owner, after reading the docs): it stays on
   GitHub's autonomous default setup — no workflow file in this repo, no
   sequencing, no dependency from our pipeline on it.
2. **`cargo build --workspace --all-targets` is dropped.** Nothing is
   published; `cargo clippy --all-targets -D warnings` compiles every target
   (tests, examples, benches) and fails on any compile error, and `cargo
   test` compiles and runs the test targets. Zero verification loss.
3. **Two test legs, not three:** `--all-features` and
   `--no-default-features`. Today `default` ≡ `--all-features` (single
   feature, on by default), so the third serial run is a literal duplicate.
   Migration rule: the `default` leg returns the day a second, non-default
   feature exists.
4. **One workflow file** (`ci.yml`), release as a job with `needs` + `if` —
   not two workflows chained by `workflow_run` (default-branch-only,
   conclusion checks, `head_sha` checkout, rerun fragility).
5. **Composite action `.github/actions/rust-setup`** owns checkout + cache
   setup, shared by all compiling jobs — the owner's `php-init` pattern,
   in-repo (no cross-repo shared-workflow repository).
6. **`Swatinem/rust-cache@v2.9.2`, pinned** (never the floating `v2` tag),
   with `save-if` limited to `main` — research-backed: automatic key
   derivation over toolchain + lock + member manifests, post-save `target/`
   cleanup, and quota hygiene; the pin plus main-only saving buys down the
   known risks (copy-forward growth, floating-tag lag).
7. **No PR/Issue templates** (collaborators-only repo). Dependabot flows
   through the normal PR trigger.

## Verified findings (this cycle's research + repo facts)

- `rust-cache` derives keys from job identity, OS/arch, the rustc
  release/host/commit-hash of every toolchain, `CARGO`/`CC`/`RUST*` envs,
  member `Cargo.toml`s, registry-only `Cargo.lock` rows,
  `rust-toolchain.toml`, and `.cargo/config.toml` — a hand-rolled
  `actions/cache` key that omits the toolchain (the official example does)
  rebuilds cold on every stable bump (research, src/config.ts).
- `rust-cache` strips workspace-crate artifacts (including the `macros`
  proc-macro `.so`) and prunes the registry/git caches to the used set
  before saving; a manual `target/` save grows unboundedly and
  `restore-keys` copy-forward never prunes (research).
- Both mechanisms share the 10 GB per-repo quota with LRU + 7-day eviction;
  PR-branch saves land in `refs/pull/N/merge` and are reusable only by that
  PR's reruns — saving only on `main` avoids the waste (research).
- `rust-cache` risks: copy-forward growth on prefix restores (#381),
  floating `v2` tag lagging releases (#330), glibc/runner-image outside the
  key (#214), rate-limit errors (#188), cleanup regressions fixed in v2.9.2.
  Bought down by pinning the release and saving on `main` only.
- The crate has exactly one feature (`tree-sitter`, in `default`), so
  `default` and `--all-features` select the identical feature set — the
  current third test run executes the identical suite (Cargo.toml).
- Wall-clock savings bound: dependency compilation (the tree-sitter C build
  is the heavy dependency) plus the removed duplicate run; the actual job-time
  delta for this crate is unmeasured (research).

## Design

### Architecture

One workflow, four jobs, two permission tiers:

```
push/PR/dispatch ──► checks (fmt, clippy, doc)     permissions: contents: read
                 └─► test[all-features]           permissions: contents: read
                 └─► test[no-default-features]    permissions: contents: read
                          │ (needs: checks + all test legs)
push to main ────► release (tag, changelog, gh-release)  contents: write
```

`checks` and both `test` legs run in parallel; wall-clock is the slowest
leg, not the sum. `release` waits on all of them via `needs` — it cannot
detach from green tests.

### Components

1. **`.github/workflows/ci.yml`** (replaces `rust.yml`, deleted in the same
   change):
   - Triggers: `push` to `main`, `pull_request` to `main` (covers
     Dependabot), `workflow_dispatch` (any branch — the temporary manual-run
     lever; no config edit needed).
   - `concurrency: ci-${{ github.ref }}` with `cancel-in-progress: true` —
     a fresh push to the same ref cancels the superseded run.
   - Top-level `permissions: contents: read`; the release job overrides.
   - `env: CARGO_TERM_COLOR: always` carried over.

2. **`checks` job** — `rust-setup`, then `cargo fmt --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`, and
   `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS: "-D warnings"`.

3. **`test` job** — matrix over `features: ["--all-features",
   "--no-default-features"]`, each leg: `rust-setup`, then
   `cargo test --workspace ${{ matrix.features }} --verbose`. Default
   per-job cache keys isolate the legs' different feature sets.

4. **`release` job** — `needs: [checks, test]`,
   `if: github.ref == 'refs/heads/main' && github.event_name == 'push'`,
   `permissions: contents: write`, `rust-setup` with `fetch-depth: 0` and
   `cache: false` (it compiles nothing). Then the owner's email-listener
   trio, minus the npm tail:
   - `anothrNick/github-tag-action@v1` — patch bump by default,
     `#major`/`#minor`/`#patch`/`#none` tokens from commit messages,
     `TAG_PREFIX: v`, output `tag`;
   - `loopwerk/tag-changelog@v1` — changelog between tags;
   - `softprops/action-gh-release@v2` — `generate_release_notes: true`,
     `append_body: true`, `make_latest: true`, tag/name from the bump
     output, body from the changelog output.
   Every green push to `main` produces a tag and a Release (patch by
   default); `#none` in the merge commit skips a bump.

5. **`.github/actions/rust-setup/action.yml`** (composite):
   - Inputs: `fetch-depth` (default `"1"`; the release job passes `"0"` —
     the `shared-workflows` precedent for the tag/changelog pair) and
     `cache` (default `"true"`; release passes `"false"`).
   - Steps: `actions/checkout@v4` with the input's `fetch-depth`; then,
     gated on `cache`, `Swatinem/rust-cache@v2.9.2` with
     `save-if: ${{ github.ref == 'refs/heads/main' }}`.
   - The toolchain comes from `rust-toolchain.toml` via rustup — no
     toolchain action.

### Trigger × behavior matrix

| event | checks | test×2 | release | cache save |
|---|---|---|---|---|
| push to `main` | ✓ | ✓ | ✓ | ✓ (main) |
| PR to `main` (incl. Dependabot) | ✓ | ✓ | — | restore only |
| `workflow_dispatch`, any branch | ✓ | ✓ | — | restore only¹ |

¹ save-if is `ref == main`; a dispatch on `main` restores and saves, on any
other branch restores only. The `event_name == 'push'` gate keeps dispatch
from ever releasing.

### Error handling

- A failing leg fails the run (default fail-fast is acceptable at two legs);
  `needs` keeps release from running.
- `cancel-in-progress` supersedes outdated runs on the same ref.
- Release-job failures surface as a red run on `main` with no partial state
  beyond what the failing action already did (tag bump before a release
  failure leaves a tag but no Release; the next push re-bumps past it).

## Testing

No in-repo test code — the verification is the pipeline observing itself:

1. YAML/schema sanity: parse the workflow and composite action (actionlint
   if locally available; otherwise review against the actions' docs).
2. Branch rehearsal: `workflow_dispatch` the new `ci.yml` from a feature
   branch — exercises `rust-setup`, cache restore, checks, and both test
   legs end-to-end without any release side effects.
3. First `main` run: checks + tests green, then the release job produces
   tag + Release + changelog; verify the Release body and tag prefix (`v`).
4. Docs greps: no stale battery text ("three feature configurations",
   standalone `cargo build --workspace --all-targets` as a CI step) in
   `CLAUDE.md` / `.claude/rules/tech.md`.

## Documentation sync (same change)

- `CLAUDE.md` (Commands): the CI description becomes — checks (fmt, clippy
  `-D warnings`, doc `-D warnings`) plus a two-leg test matrix
  (`--all-features`, `--no-default-features`) in parallel, cached; a release
  job tags and publishes a GitHub Release with changelog on `main` pushes;
  workflow_dispatch runs CI on any branch. The `cargo build --all-targets`
  sentence goes.
- `.claude/rules/tech.md` (Verification battery): the battery loses the
  standalone build step and the `default` test configuration — **this
  defines the local battery too**, since tech.md pins local verification to
  "the same battery CI runs". Add the migration rule: the `default` leg
  returns when a second, non-default feature exists. `cargo dupes check`
  and criterion benches stay out of CI (on demand).
- README.md: not affected (no CI claims there).

## Rejected alternatives

- **Two workflows chained by `workflow_run`** — fires only from the default
  branch, needs explicit conclusion checks and `head_sha` checkouts, reruns
  are fragile; `needs` inside one file does the same job natively (decision
  4).
- **Three-leg matrix always** — future-proof, but keeps the
  `default`/`--all-features` duplicate alive until a second feature lands;
  the migration rule is one documented line instead (decision 3).
- **Manual `actions/cache@v4`** — defensible only under a hard
  no-third-party-action constraint; loses key derivation over the toolchain,
  `target/` cleanup, and quota hygiene, and degrades as cargo changes
  `target/` internals (research verdict; decision 6).
- **Keeping `cargo build --all-targets`** — compiles nothing the remaining
  steps don't already compile (decision 2).
- **CodeQL sequencing/ownership** — owner decision 1: autonomous default
  setup stays untouched.
- **Cross-repo shared-workflow repository** — owner: the pattern is wanted
  in-repo, not as a separate repo.
