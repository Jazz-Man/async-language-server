# Research: Swatinem/rust-cache@v2 vs manual actions/cache@v4 for Cargo CI caching

Date: 2026-09-07
Question: for the planned single `ci.yml` (jobs: `checks` = fmt + clippy + doc; two test legs
`--all-features` / `--no-default-features`; `release` = no compilation), triggered on push to
`main`, PRs to `main` (incl. Dependabot), and `workflow_dispatch`, on `ubuntu-latest` only —
which caching approach, and what are the verifiable trade-offs? The choice will be centralized
in a `.github/actions/rust-setup` composite action.

Repo facts that drive the analysis (verified locally): root library crate (`publish = false`) +
`macros` proc-macro member; `Cargo.lock` committed; `rust-toolchain.toml` = `stable` + rustfmt/
clippy components (MSRV 1.88, edition 2024); one default feature (`tree-sitter`, pulls a
C-compiling dependency); one criterion bench; two examples; doctests run in all feature
configurations. Current `.github/workflows/rust.yml` has no caching at all.

---

## Sources

Rust-cache (pinned where noted; master = `f0d9c388`):

- S1. https://github.com/Swatinem/rust-cache (README at master `f0d9c388`) — key inputs, cached
  paths, cleanup contract, quota notes, known issues.
- S2. https://github.com/Swatinem/rust-cache/blob/master/action.yml — inputs/outputs, post-step
  condition (`success() || CACHE_ON_FAILURE`).
- S3. https://github.com/Swatinem/rust-cache/blob/master/src/config.ts — exact key construction,
  path list, workspace resolution.
- S4. https://github.com/Swatinem/rust-cache/blob/master/src/restore.ts — restore-keys fallback,
  `CARGO_INCREMENTAL=0` export, pre-clean on partial match, `isCacheUpToDate`.
- S5. https://github.com/Swatinem/rust-cache/blob/master/src/save.ts — post-save cleanup pipeline,
  macOS workaround, `save-if` gate.
- S6. https://github.com/Swatinem/rust-cache/blob/master/src/cleanup.ts — what exactly is deleted
  from `target/`, registry, git, bin; one-week timestamp pruning; Cargo build-dir V2 handling.
- S7. https://github.com/Swatinem/rust-cache/releases — v2.7.5 (2024-10-12) → v2.9.2 (2026-08-06)
  release notes; v2.9.2 ships Cargo build-dir V2 support (PR #371).
- S8. https://github.com/Swatinem/rust-cache/issues/381 (open) — copy-forward cache-growth report
  with measured numbers.
- S9. https://github.com/Swatinem/rust-cache/issues/330 (open) and #141 — floating `v2` tag lags
  the latest v2.x release.
- S10. https://github.com/Swatinem/rust-cache/issues/214 (open) — runner glibc changes not in key
  (astral-sh/uv breakage incident).
- S11. https://github.com/Swatinem/rust-cache/issues/268 (open) — virtual-root Cargo.toml not
  hashed.
- S12. https://github.com/Swatinem/rust-cache/issues — recent-issue survey (titles/states as of
  2026-09-07): #370, #375, #369, #341, #344, #242, #188, #302, #315, #348.

actions/cache and GitHub platform:

- S13. https://github.com/actions/cache (README at main `3edfce90`) — inputs, cache version,
  cache limits (10 GB), scope note, deprecation/version news, "not taking contributions" notice.
- S14. https://github.com/actions/cache/blob/v4.3.0/action.yml — `post-if: success()`,
  deprecated `save-always`, node20 runtime.
- S15. https://github.com/actions/cache/blob/main/examples.md — the official Rust/Cargo example.
- S16. https://github.com/actions/cache/blob/main/tips-and-workarounds.md — immutable cache /
  update pattern, PR-cache quota thrash + `gh cache delete` cleanup, segment restore timeout.
- S17. https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching —
  key matching, restore-keys semantics, cache version, branch scoping incl. `refs/pull/.../merge`,
  low-trust read-only rules, 10 GB quota + LRU + 7-day eviction, rate limits (200 uploads/min).
- S18. https://doc.rust-lang.org/cargo/reference/build-cache.html — `target/` layout,
  `target/debug/incremental/`, build-dir internal to Cargo and subject to change.

Not verified (web search unavailable in this environment): CI-time benchmarks for this crate,
and any blog-post size surveys of typical Cargo caches. Nothing below relies on those.

---

## 1. rust-cache@v2 mechanics

### 1.1 Cached paths (S1, S3)

One cache entry per (job × env) built from:

- `~/.cargo/bin`, `~/.cargo/.crates.toml`, `~/.cargo/.crates2.json` (if `cache-bin`, default true)
- `~/.cargo/registry` (index + cache + src)
- `~/.cargo/git` (db + checkouts)
- the workspace `target` dir (if `cache-targets`, default true; `workspaces` input defaults to
  `. -> target`)
- extras via `cache-directories`

`~/.cargo/registry/src` is *not* restored-to-worth: cargo recreates it from the compressed
`.crate` archives; only `-sys` crates' unpacked sources are kept (S1, S6 — because `-sys` build
scripts timestamp-check their sources).

### 1.2 Cache key derivation (S3, exact from source)

```
key = prefix-key ("v0-rust" default)
    + "-" + shared-key                         // if set: replaces job-based key entirely
    |   ( + "-" + key input + "-" + GITHUB_JOB // if add-job-id-key, default true
          + "-" + os.type() + "-" + os.arch() )
    + "-" + env-hash                           // if add-rust-environment-hash-key, default true
    + "-" + lock-hash                          // same switch

restoreKey = key up to and including env-hash   // no lock-hash
```

- **env-hash**: sha1 over `rustc -vV` (release/host/commit-hash) of the default toolchain plus
  every `rustup`-installed toolchain (since v2.9.0), plus sorted `KEY=value` of env vars matching
  prefixes `CARGO CC CFLAGS CXX CMAKE RUST` (+ `env-vars` input). RUSTFLAGS/RUSTDOCFLAGS land
  here automatically.
- **lock-hash**: sha1 over (a) every workspace member's parsed `Cargo.toml` — with
  `package.version` normalized to `0.0.0` and path-deps' version/path stripped, so
  workspace-internal version bumps don't churn the key, but *any* other manifest change
  (features, `[lints]`, `[profile]`) does; (b) `Cargo.lock` v3/v4 `[[package]]` rows filtered to
  registry packages only (path-only workspace rows excluded); (c) root and per-workspace
  `rust-toolchain`/`rust-toolchain.toml`; (d) `.cargo/config.toml` files.

Consequences for this repo:

- Default config → **one cache key per job**: `v0-rust-checks-Linux-x64-<env>-<lock>`,
  `v0-rust-test-all-features-...`, `v0-rust-test-no-default-features-...`. The two test legs are
  isolated from each other by the job id — their different feature sets never mix caches unless
  you opt into a `shared-key`.
- Stable-channel updates (weekly on `ubuntu-latest`) change the commit-hash in env-hash → new
  exact key, but restoreKey still prefix-matches the previous cache. Same for `Cargo.lock`
  bumps (Dependabot PRs): S1's "restore from a previous Cargo.lock version" is exactly this
  restoreKey fallback.
- On an exact hit, `isCacheUpToDate()` short-circuits the post step: no re-save, no quota churn
  (S4).

### 1.3 What is cleaned before saving (S5, S6, S1)

Post-step (runs on `success() || CACHE_ON_FAILURE`, S2):

- **target/**: everything that is not a dependency artifact — workspace crates' own artifacts
  (incl. the `macros` proc-macro `.so` and `async_language_server` itself), non-profile files,
  `tests`/trybuild-style nested workspaces — are removed. Per profile dir it keeps only
  `build/`, `.fingerprint/`, `deps/` for dependency packages (keeplists derived from
  `cargo metadata`, name variants for lib/bin target names; handles both Cargo V1 hash-suffix
  and V2 build-dir layouts — V2 support shipped in v2.9.2, S6/S7).
- On a *partial* restore (restoreKey matched, exact key missed) it additionally pre-cleans
  build/fingerprint entries older than one week (S4).
- **registry**: `credentials.toml` deleted; index `.cache` pruned to used packages;
  `registry/cache` pruned to used `.crate` files; `registry/src` kept only for `-sys` crates
  (unless `cache-all-crates`).
- **git**: unused git deps (db + checkouts) pruned.
- **bin**: binaries that existed before the action ran are removed (so rustup toolchains are not
  smuggled into the cache; known nuisance on self-hosted runners, `cache-bin: "false"`
  workaround, S1).
- `CARGO_INCREMENTAL=0` is exported at restore time (S4), so incremental artifacts are mostly
  never created in the first place — the README lists "incremental build artifacts" among what
  is cleaned; in current source incremental dirs are not separately walked, the env var is the
  operative mechanism.

README rationale (S1): workspace crates are excluded because caching them "is generally not
effective" (their artifacts invalidate on every source change anyway); links to issue #37.

### 1.4 PR behavior and policy inputs (S1–S5)

- rust-cache *does* save on PRs by default (`save-if` default `"true"`); where those entries
  land is a GitHub platform rule — see §3.
- `save-if: ${{ github.ref == 'refs/heads/main' }}` is the documented pattern to only save from
  main while every PR still restores from main's caches (S1).
- `cache-on-failure: "true"` makes the post step run on failure (S2, S5).
- `lookup-only` checks existence without downloading (S2).
- `cache-workspace-crates: "true"` (v2.8.0) opts into caching workspace crates; the restore key
  is still dependency-derived, not source-keyed, so restored workspace artifacts are stale on
  every commit (feature request for source-keying: #348, S12). Not recommended for a repo whose
  own code changes every push.

### 1.5 Maintenance status (S7, S12, master log)

Releases: v2.7.5 2024-10-12 → v2.7.8 2025-03-19 → v2.8.0 2025-06-25 → v2.8.1 2025-09-18 →
v2.8.2 2025-11-26 → v2.9.0/2.9.1 2026-03-12 → v2.9.2 2026-08-06; latest master commit
2026-08-17. The 2026 releases fixed real cargo-ecosystem drift: build-dir V2 layout (#371),
timestamp-cleanup early-return (#375/PR #377), unsorted rustc versions in key (#369),
node24 migration. CI now includes zizmor security scanning (v2.8.2 notes). Actively
maintained; single maintainer.

---

## 2. actions/cache@v4 manual mechanics

### 2.1 What you get (S13, S14, S17)

- `path` + `key` (required), `restore-keys` (optional), `lookup-only`,
  `fail-on-cache-miss`, `enableCrossOsArchive`. `cache-hit` output is `true` only on exact key.
- v4.3.0 post-step runs `post-if: success()` — **cache saved only on job success**; the
  `save-always` input is explicitly deprecated as "does not work as intended"; the supported
  escape hatch is the `actions/cache/restore` + `actions/cache/save` split with your own `if:`
  (S14, S13).
- Entries are immutable; "if the provided `key` matches an existing cache, a new cache is not
  created" — same exact-key short-circuit rust-cache implements (S13, S17).
- Cache *version* = hash of the path list + compression tool: same key over different `path`
  lists = different entry; caches don't cross OSes without `enableCrossOsArchive` (S13, S17).
- restore-keys: tried in order, prefix-matched, most-recently-created match wins; searched in
  the current branch scope first, then the default branch (S17).

### 2.2 What you must handle by hand

The official Rust example (S15) is the whole upstream guidance:

```yaml
path: |
  ~/.cargo/bin/
  ~/.cargo/registry/index/
  ~/.cargo/registry/cache/
  ~/.cargo/git/db/
  target/
key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
```

Gaps you must close yourself for the planned CI:

1. **Toolchain in the key.** `hashFiles('**/Cargo.lock')` does not see rustc. On
   `ubuntu-latest` the stable toolchain moves (roughly every 6 weeks, plus image updates);
   without a rustc-version component (e.g. an extra `rustc -vV | hash` step or
   `hashFiles('rust-toolchain.toml')` — the latter only changes if *you* edit it, not when
   stable advances) every toolchain bump produces a full rebuild under an unchanged key, and the
   stale entries linger until eviction.
2. **Per-job disambiguation.** The two test legs build different feature sets of the same
   workspace. One shared key = the three compile jobs race for one immutable entry
   ("matches an existing cache → not created", S13) and share a `target/` mixing
   `--all-features` and `--no-default-features` artifacts; cargo re-checks/rebuilds whichever
   configuration didn't win. Per-job keys (`${{ matrix.features }}` or job suffix) avoid the
   mixing and reproduce rust-cache's default isolation — but then you maintain it in every job.
3. **No cleanup, ever.** Whatever `target/` contains at job end is tarred up: workspace-crate
   artifacts, `target/doc/`, clippy/doc/test artifacts, and — unless you set
   `CARGO_INCREMENTAL=0` yourself — `target/debug/incremental/` (S18 documents incremental as a
   standard part of the build dir). There is no dependency-keeplist pruning of the kind
   rust-cache performs (§1.3). With restore-keys in play, every partial restore copies the whole
   previous tree forward and nothing prunes old artifact generations at any age — the same
   copy-forward failure mode documented for rust-cache in #381 (S8), with weaker protection:
   rust-cache at least strips non-dependency artifacts and >1-week-old fingerprints.
4. **Failure-path policy.** Saving on failed runs (useful for fixing red builds) needs the
   restore/save split with a custom `if:`; v4's `save-always` is deprecated (S14).
5. **PR policy.** Restricting saves to `main` is a hand-written `if:` on the save half (or on
   the whole action), §3.

### 2.3 Version drift note

The task names @v4; the actions/cache README's current guidance targets v4→v5/v6: the backend
migrated to the new cache service (v2 APIs, Feb 2025), v4.2.0+ is the minimum for it, v5 runs
on node24 and needs runner ≥ 2.327.1, v6 is current on main. For hosted `ubuntu-latest` this is
transparent; mechanics verified here at the pinned v4.3.0 tag are unchanged across these
versions (S13, S14).

---

## 3. Shared GitHub Actions cache constraints (apply to both)

All from S17 unless noted:

- **Quota**: 10 GB per repository by default (raisable for a fee, up to 10 TB). Overflow →
  LRU eviction by *last access*; anything unaccessed for 7 days is evicted regardless.
- **Rate limits**: 200 cache uploads/min and 1500 downloads/min per repository.
- **Branch scoping**: a run restores caches created on its own branch, plus the default
  branch, plus (for PRs) the base branch. It can never see caches from child/sibling branches
  or other tags.
- **PR writes are near-waste**: a `pull_request` run saves into `refs/pull/N/merge`, which only
  re-runs of that same PR can restore — not `main`, not other PRs. Each PR push therefore
  *spends* quota on entries whose reuse is limited to later pushes on the same PR.
- **Low-trust triggers are read-only**: fork-PR runs cannot write caches at all (warning in the
  log, job continues); only `push`, `workflow_dispatch`, `schedule`, etc. may write into the
  default-branch scope. Dependabot PRs from inside this repo are `pull_request` runs: they can
  write, but only into their own merge-ref scope.
- **Poisoning surface**: caches are unsigned; anyone who can open a PR can read base-branch
  cache contents, so nothing secret belongs in `~/.cargo` or `target/` (S17). Mitigation for
  both tools is identical: `save-if`/`if:` limited to trusted triggers, pin the action.
- **PR-cache thrash is a documented, cross-tool pattern**: S16 describes PR-created caches
  filling quota while "cannot be used in default branch scope", and ships the canonical
  `gh cache list --ref refs/pull/N/merge` + `gh cache delete` cleanup workflow.

Quota arithmetic for the planned shape (inference from the above sources): with rust-cache's
default job-keyed config, every push to `main` maintains 3 entries (`checks`, leg 1, leg 2) and
every PR can add up to 3 more, merge-ref-scoped. Without a main-only save policy, Dependabot's
weekly PRs alone add ~3 short-lived entries each. With the main-only policy, PRs cost zero
quota and restore from main. The same arithmetic governs a manual per-job key design; a manual
single-shared-key design spends 1 entry per state instead of 3, at the cost of the feature-set
mixing described in §2.2.2.

---

## 4. Risks

### 4.1 rust-cache-specific

- **Copy-forward target growth on partial restores** (#381, open, S8): prefix-restored `target/`
  + name-based cleanup can retain several artifact generations for still-used packages; the
  reporter measured a compressed lineage 206 MB → 13.89 GB (exact hits restoring a 17.82 GB
  object) and a job where save+restore took 9m54s against a 3m50s build. Single-incident, but
  it is the failure mode the restoreKey design permits; the proposed exact-only target mode
  does not exist yet. Mitigation available today: `save-if` on main only + occasional manual
  purge via the Cache API (S1).
- **Floating `v2` tag lags releases** (#330/#141, open, S9): `Swatinem/rust-cache@v2` may not
  point at the newest v2.x. Pin an exact tag or commit SHA (also the zizmor-grade hygiene for a
  third-party action).
- **Runner-image drift not in the key** (#214, open, S10): glibc changes on `ubuntu-latest`
  broke cached builds at astral-sh/uv; rustc version is keyed, the host libc is not. Rare,
  self-inflicted by GitHub image updates, recoverable by manually evicting caches.
- **API rate limits** (#188, open): restore/save calls can hit the GitHub API rate limit
  (see §3's per-repo limits); observed as intermittent `Error: API rate limit exceeded`.
- **Cleanup-code bugs are upstream's to have**: #370 (build-dir V2 + hyphenated names broken
  until v2.9.2), #375 (one-week pruning collected at most one entry per dir until PR #377),
  #369, #341 (macOS `cache-bin`), #302, #315. All fixed by v2.9.2 — evidence of active
  maintenance, and simultaneously evidence that correctness here is nontrivial and you inherit
  its regressions by floating.
- **Workspace/proc-macro/doctest specifics**: no issue reports found on doctests (no
  doctest-specific behavior exists on either side); proc-macro members are handled like any
  workspace crate — their artifacts are *pruned* at save (§1.3), i.e. rebuilt every run. #268
  (virtual-root manifest not hashed) does not apply: this repo's root `Cargo.toml` is a package.

### 4.2 Manual actions/cache-specific

- **Key-construction mistakes are silent.** Missing the toolchain component means guaranteed
  weekly full rebuilds on `ubuntu-latest` (§2.2.1) — the failure is invisible unless you diff
  job times. Missing per-job disambiguation means cross-feature `target/` mixing and a
  save race (§2.2.2). GitHub's own Rust example (S15) includes neither guard and no
  restore-keys at all: verbatim adoption gives a cold rebuild on every toolchain or lockfile
  change.
- **Unbounded entries.** No pruning of `target/` (workspace artifacts, `target/doc/`,
  incremental unless you set `CARGO_INCREMENTAL=0`), no registry pruning (S18, §2.2.3). Bigger
  entries → slower save/restore (archive + upload/download of everything) → more quota per
  state → earlier LRU eviction of the *other* jobs' entries. With 3 cache-writing jobs on a
  10 GB budget, entry size is the variable you control least well by hand.
- **Restore-keys without cleanup is copy-forward with no brakes.** The accumulation mechanism
  analyzed in #381 (S8) applies verbatim to a manual `restore-keys` prefix design; rust-cache's
  keeplist + 1-week pre-clean + `CARGO_INCREMENTAL=0` at least slow it down. An exact-only
  manual design (no restore-keys) avoids it but recompiles all deps on every lockfile bump —
  the trade-off #381 proposes rust-cache adopt, hand-rolled.
- **No ecosystem drift tracking.** rust-cache needed a code change for Cargo's build-dir V2
  layout (S7); a manual config has no such mechanism — its correctness decays quietly as cargo
  changes `target/` internals, which upstream explicitly reserves the right to do (S18:
  "considered internal to Cargo, and is subject to change").
- **Counterweight benefits**: first-party, GitHub-maintained action (though the repo states it
  takes no contributions; security fixes still honored, S13); no third-party supply chain;
  exact control over key and restore policy (you can implement today the exact-only target
  mode rust-cache lacks, S8); immune to upstream cleanup bugs (#375/#370 class).

---

## 5. Comparative verdict for this repo

Shape recap: 3 compiling jobs (checks = fmt/clippy/doc; two test legs differing only in feature
flags), 1 non-compiling job (release), single OS, single pinned-by-`rust-toolchain.toml` stable
toolchain, committed `Cargo.lock`, one heavy C-compiling dep (tree-sitter), Dependabot PRs
weekly.

**The composite wrapper neutralizes the ergonomics difference, and only that.** Both options
become one `<uses>` in `.github/actions/rust-setup` instead of N workflow stanzas, so the
classic rust-cache argument ("no boilerplate per job") disappears: a manual design is equally
central. What the wrapper cannot neutralize:

1. **Key semantics**: rust-cache derives rustc-release/host/commit-hash, env prefixes, and a
   *parsed, workspace-normalized* manifest+lock hash without any extra steps (S3). The manual
   equivalent needs a hand-rolled rustc-hash step and `hashFiles` (coarser: whole-file, and
   blind to toolchain drift) — §2.2.1.
2. **Entry hygiene**: rust-cache strips workspace-crate artifacts, prunes registry/git to the
   used set, avoids incremental artifacts, and pre-cleans on partial restore (S4–S6). Manual
   saves whatever is on disk (S18). On a 10 GB shared quota with 3 concurrent entry-lines this
   is the difference between comfortably fitting and managing eviction yourself.
3. **Ecosystem drift**: build-dir V2 support arrived as a rust-cache release (S7); a manual
   config rots silently.
4. **Cost of ownership flips**: with rust-cache you inherit upstream cleanup regressions (via
   pinning, deliberately, per release notes); with manual you own all of §2.2 yourself,
   permanently.

**What rust-cache would cost here**: third-party supply-chain surface (mitigated: pin SHA;
upstream runs zizmor), the copy-forward growth risk (mitigated: `save-if` main-only + Cache API
purge), and the fact that this repo's own crates are rebuilt every run either way (§1.3) —
savings are bounded to dependency compilation, which for a tree-sitter-heavy dependency tree is
the dominant cold cost, but is not measurable from sources available here [Unverified: actual
job-time deltas for this crate].

**Recommendation** — `Swatinem/rust-cache@v2` (pinned to a specific tag/SHA, e.g. the v2.9.2
tag or its commit), configured in the composite with:

- `save-if: ${{ github.ref == 'refs/heads/main' }}` — PRs and Dependabot legs restore from
  main, write nothing into merge-ref scope (§3 quota math);
- default `add-job-id-key` (per-job isolation for the two feature legs) — no `shared-key`;
- release job: skip the cache step entirely (a composite input like `cache: false`), since it
  invokes no cargo command and would otherwise mint a registry-only entry;
- optionally `cache-on-failure: "true"` on main to keep red builds warm while fixing.

Manual actions/cache@v4 is the right answer only if the deciding criterion is "no third-party
action, period" — the design above (per-job exact keys + toolchain hash + main-only saves +
`CARGO_INCREMENTAL=0` + a cleanup habit) is reproducible, but it is a permanent maintenance
liability that rust-cache currently performs better, per sources S1–S18.
