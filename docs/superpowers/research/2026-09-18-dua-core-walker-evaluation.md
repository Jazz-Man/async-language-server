# dua-core 4.1.0 as the workspace walker engine

Date: 2026-09-18.
Question: can `dua-core` 4.1.0 replace `ignore` 0.4.33's parallel walk
(`WalkBuilder::build_parallel`) as the engine under `WorkspaceWalker`
(`src/workspace/walker.rs`)?

**Supersedes:** `2026-09-17-workspace-walker-crates.md` — that pass surveyed
the walker-crate landscape and picked `ignore`; this pass deep-evaluates the
one candidate the survey's own lineage pointed at (`jwalk`'s successor, which
is what dua-core 4.x is). The survey's gitignore-moat conclusion stands and is
confirmed in detail below; the file it lives in is retained.

Research only — no source files modified, no git writes.

## What dua-core 4.1.0 actually is

The 4.x crate is not a disk-usage analyzer. Its manifest description reads
"Fast parallel filesystem traversal iterators"
(`dua-core-4.1.0/Cargo.toml:24`), and the module doc describes a
work-stealing parallel traversal with a `descend` predicate
(`dua-core-4.1.0/src/lib.rs:1-26`). The history: dua-cli replaced its jwalk
dependency with a crate-local work-stealing walker in dua-cli 2.40.0
(2026-07-31) — "Replace jwalk with a work-stealing directory walker for up to
40% more scan speed … crate-local walker built on crossbeam-deque and
standard-library threads" (repo `CHANGELOG.md`, commits `8ada93f`), then
extracted that walker as the standalone `dua-core` crate (releases
`dua-core-v3.2.0` 2026-08-25 through `dua-core-v4.1.0` 2026-09-12). The
directory in the repo is `crates/dua-lib/` while the package it contains is
named `dua-core` (`crates/dua-lib/Cargo.toml` at tag `dua-core-v4.1.0`,
`name = "dua-core"`) — the owner's `dua-lib` sighting is the directory name,
not the package.

## Verdict table

| Gate | dua-core 4.1.0 | ignore 0.4.33 (incumbent) | Verdict | Evidence |
|---|---|---|---|---|
| R1 ignore semantics | **None.** Zero ignore-file machinery: no `.gitignore`, no `.ignore`, no global, no `.git/info/exclude`, no hidden-file filter, no git awareness anywhere in the crate. Only traversal filtering is the caller's `descend` predicate for directories | Full chain: `.ignore`, `.gitignore`, `.git/info/exclude`, global gitignore (`core.excludesFile`), parents, custom names, hidden toggle, each individually toggleable | **FAIL** (as a drop-in) | dua-core `lib.rs` full read — no ignore/git references; deps are `crossbeam` only. Incumbent: `ignore-0.4.33/src/walk.rs:862-957` |
| R2 per-entry error tolerance | Yes — errors are iterator items, not terminators: directory-open failures yield one `Err` item and the walk continues; per-entry read errors ride inside `Ok` batches | Yes — `Err(entry)` per entry; our walker skips + `tracing::warn` and continues | **PASS** | dua-core `lib.rs:131-134` (Batch type doc), `:1061-1077` (error sent, then `finish_pending`), `:572-574`/`:695-698` (consumer yields it and continues). Incumbent: `src/workspace/walker.rs:76-79`, test at `:197-230` |
| R3 determinism | "Sibling order is unspecified in both modes" — no ordering contract to fight; sort-after-collection unaffected | Parallel order unordered by nature; we already sort after collection | **PASS** | dua-core `lib.rs:6`; incumbent: `src/workspace/walker.rs:84-87` |
| R4 threading model | Own pool: crossbeam-deque work-stealing + Parker/Unparker, `threads` caller-supplied, `threads.max(1)`, threads named `dua-fs-walk-{i}`. Fully synchronous iterator API; pool joined on drop. Embeddable in one `spawn_blocking` | Own pool per `build_parallel()` call; `threads()` knob; synchronous callback API inside one `spawn_blocking` | **PASS** | dua-core `lib.rs:784-822` (`start_pool`), `:927-938` (`Drop for Pool` joins). Incumbent: `src/server/state/workspace.rs:159-177` |
| R5 API surface | **Per-entry iterators, not aggregates**: `walk()` → `Walk: Iterator<Item = io::Result<Entry>>`; `walk_roots()`/`walk_root_entries()` → `RootWalk: Iterator<Item = (usize, RootEvent)>` with per-root `Finished`; `stream_roots()` for incremental root submission; `Entry::path() -> PathBuf` | `WalkParallel` batch-callback over `DirEntry`-like entries; `Walk` serial iterator | **PASS** — the 4.x rewrite turned the crate into a general traversal library | dua-core `lib.rs:435-479` (`walk`), `:602-617` (`walk_roots`), `:630-645` (`walk_root_entries`), `:381-395` (`stream_roots`), `:276-281` (`RootEvent`), `:744-750` (`Entry::path`), `:200-216` (`Entry` fields) |
| R6 dependency hygiene | MIT; edition 2024; `rust-version = "1.88"` (< our MSRV 1.90). Runtime deps: `crossbeam` only (+ platform-gated `libc`/`windows-sys`; `tempfile` dev-only). Tree check: single `crossbeam-utils 0.8.21`, shared with `ignore` and `dashmap` — no duplicate versions. `deny.toml` allow-list has MIT/Apache-2.0/Unicode-3.0 — plausible pass | MIT/Apache (unpacked); deps globset, crossbeam-deque, regex-automata, walkdir, same-file, memchr, log, winapi-util — all already in tree | **PASS** | `dua-core-4.1.0/Cargo.toml:12-52`; `cargo tree -i crossbeam-utils` output (recorded below); `Cargo.lock:410-413` (crates.io + checksum); `deny.toml:28-36` |
| R8 custom ignore | Programmatic pruning **during traversal**: yes — the `descend` predicate prunes children of a rejected directory (the entry itself is still yielded, so a consumer-side filter completes the pattern). Custom ignore **file names / automatic ignore-file reading**: none — nothing reads ignore files; matching is entirely the caller's job | `add_custom_ignore_filename` (custom names, highest precedence); `OverrideBuilder` globs applied during traversal (prunes) | **CONDITIONAL** — the pruning hook exists; the ignore semantics do not | dua-core `lib.rs:128-130` (`Descend` doc), `:375-378`/`:1199-1211` (descend consulted per directory). dua-cli's own layering: `src/common.rs` `iter_from_paths` — one `is_excluded` closure shared between `descend` and an iterator `.filter()`, matching via `gix::ignore::Search` |
| Git-less behavior | N/A — no git-related behavior exists at all; no `require_git`-style knob (there are no git rules to gate) | `require_git` defaults to `true`; git-family rules (`.gitignore`, `.git/info/exclude`, global) apply only when a `.git`/`.jj` dir exists at or above the walk root; `.ignore` files apply regardless of git presence | N/A vs incumbent default | dua-core: absent by full read. Incumbent: `ignore-0.4.33/src/dir.rs:800` (`require_git: true` default), `:242-243` (`.git`/`.jj` probe), `:560-561` (`any_git` gate), `:572-578` (`.ignore` ungated) |

## Gate details

### R1 — ignore semantics (the deciding gate)

A full read of `dua-core-4.1.0/src/lib.rs` (the crate's only module, 2320
lines) finds no reference to ignore files, git, hidden entries, or entry-name
filtering of any kind. The only traversal control is the caller-supplied
`descend` predicate (`lib.rs:128-130`), which decides whether a directory's
children are traversed; rejected directories are still yielded
(`lib.rs:8-9`), and the predicate is only consulted for directories
(`lib.rs:1199-1211`) — per-file decisions happen after the yield.

Our `WorkspaceWalkConfig` exposes exactly two toggles
(`src/workspace/walker.rs:15-34`): `with_hidden_files` and
`with_ignore_files`, and the golden-path test pins hidden-file skipping and
`.ignore`-file matching as observable walk output
(`src/workspace/walker.rs:121-195`). Under dua-core, both toggles would
filter nothing: the ignore chain they stand for would have to be
re-implemented — per-directory `.gitignore`/`.ignore` discovery during the
walk, nested-wins precedence, negation (`!pattern`) semantics, anchored
patterns, `core.excludesFile` global rules read from `$HOME/.gitconfig` /
XDG paths, `.git/info/exclude`, and git-presence gating. That is precisely
the "gitignore moat" the superseded survey identified, now confirmed against
the candidate itself.

The dua-cli repo corroborates the architecture: ignore support lives **above**
dua-core, in the CLI. `--ignore-from` (gitignore-syntax pattern files, "the
equivalent of rsync's --exclude-from", repo `CHANGELOG.md`) and the
interactive gitignore-aware cleanup marking (v2.35.0, 2026-06-16) are
implemented in `src/common.rs`/`src/traverse.rs` with `gix::ignore::Search`
(gitoxide) doing the matching — dua-core traverses, the application layers
ignore semantics. The brief's expectation that "closed issues in dua-cli
mention ignore support" did not survive verification: GitHub issue search
over `Byron/dua-cli` returns zero hits for `gitignore` and `jwalk` queries
(179 issues total, all closed, 0 open as of 2026-09-18); the ignore-support
history lives in the CHANGELOG and `src/common.rs` instead, cited above.

### R2 — per-entry error tolerance

`type Batch = io::Result<Vec<io::Result<Entry>>>` — "an outer error means
the directory could not be opened; inner errors come from reading or
converting individual directory entries" (`lib.rs:131-134`). On a directory
open failure the producer sends the error batch and then releases its
pending count, so traversal continues (`read_dir_parallel`,
`lib.rs:1061-1077`). The consumer iterators surface errors as items and keep
going (`lib.rs:572-574`, `lib.rs:695-698`). This maps one-to-one onto the
incumbent's skip-and-warn closure (`src/workspace/walker.rs:76-79`). Note
the entry-level errors arrive in chunks: several `Err` items can arrive in
one `Ok(batch)` — no semantic difference for a skip-and-warn consumer.

### R3 — determinism

"Sibling order is unspecified in both modes" (`lib.rs:6`). Nothing in the
crate sorts, and nothing requires the consumer to preserve order; our
post-collection `files.sort()` (`src/workspace/walker.rs:84-87`) is
independent of delivery order by construction, and the existing golden test
already asserts exactly that contract. No ordering coupling to fight.

### R4 — threading model

`start_pool` spawns exactly `threads.max(1)` OS threads named
`dua-fs-walk-{idx}`, on crossbeam-deque `Worker`/`Injector`/`Stealer` queues
with `Parker`/`Unparker` for parking (`lib.rs:784-822`); a successful steal
ramps up one more worker. The thread count is the caller's argument — we
would pass our existing width (CPU core count), same budget as today. The
API is a plain synchronous iterator; `Drop for Pool` stops and joins all
workers, releasing those blocked on the bounded output channel
(`lib.rs:927-938`), so a walk dropped mid-iteration inside one
`tokio::task::spawn_blocking` call terminates cleanly — the single-hop
embedding we use today (`src/server/state/workspace.rs:159-177`) carries
over unchanged. Fresh pool-per-walk matches the incumbent's
`build_parallel()` behavior; both are noise at walk-per-invalidation
frequency, and the walk is cached between invalidations since `5ad77cd`
(`src/server/state/walk_cache.rs`).

Panic paths worth recording (documented invariants, not dealbreakers):
worker spawn failure panics (`lib.rs:813`,
`expect("filesystem worker thread can be spawned")`), and directory-id
allocation panics past `u32::MAX` directories within one walk
(`lib.rs:184-188`, `lib.rs:1382-1390`) — unreachable for LSP workspaces.

### R5 — API surface (was flagged CRITICAL)

Not aggregate-centric anymore. The public exports are the traversal
primitives themselves:

- `walk(root, threads, order, options, descend) -> Walk` — single root
  (`lib.rs:435-479`), `Walk: Iterator<Item = io::Result<Entry>>` with
  `restart()` and `next_cancellable()` (`lib.rs:486-528`).
- `walk_roots([(index, path)], …) -> RootWalk` — multi-root, each event
  tagged with the root index, per-root `Finished` events
  (`lib.rs:602-715`). This is the natural replacement for our per-root
  loop.
- `walk_root_entries` — same, over pre-collected entries (`lib.rs:630-645`).
- `stream_roots(threads, order, options) -> (RootSender, RootWalk)` —
  submit roots while walking (`lib.rs:375-395`).
- `Entry` carries `depth`, `file_name`, `file_type`, `metadata:
  Option<io::Result<Metadata>>`, `parent_path`, directory ids
  (`lib.rs:200-216`); `Entry::path()` reassembles the full path
  (`lib.rs:744-750`). Collecting path-shaped output is trivial.
- `Options::skip_metadata()` drops per-entry `stat` calls — our walk needs
  only the file type (`src/workspace/walker.rs:68`), so we would run
  type-only, the crate's cheapest mode (`lib.rs:1047-1059`).

The disk-usage aggregate layer (sizes, hard-link dedup, allocated-size
accounting) is all in dua-cli, not in the published crate.

### R6 — dependency hygiene

From the normalized registry manifest (`dua-core-4.1.0/Cargo.toml`): MIT
(`:26`), edition 2024 (`:13`), `rust-version = "1.88"` (`:14`) — below our
MSRV 1.90 (`Cargo.toml:9`), so no toolchain pressure. Runtime dependencies:
`crossbeam 0.8` (`:37-38`), `libc` on macOS (`:43-44`), `windows-sys` on
Windows (`:46-52`); `tempfile` is dev-only (`:40-41`). No jwalk, no rayon,
no ignore-crate internals.

Local tree (cargo tree, 2026-09-18):

```
crossbeam v0.8.5
└── dua-core v4.1.0
    └── async-language-server

crossbeam-utils v0.8.21        (single version)
├── crossbeam v0.8.5           ← dua-core
├── crossbeam-channel v0.5.17  ← crossbeam
├── crossbeam-deque v0.8.6     ← crossbeam AND ignore v0.4.33
├── crossbeam-epoch v0.9.21
├── crossbeam-queue v0.3.14
└── dashmap v6.2.1
```

`ignore` already pulls `crossbeam-deque` (`Cargo.lock:700-713`); dua-core's
arrival introduces no duplicate versions anywhere. `deny.toml`'s license
allow-list already covers MIT/Apache-2.0/Unicode-3.0 (`deny.toml:28-36`), so
`make deny` plausibly passes [Inference — from the manifest licenses of
dua-core/crossbeam; not executed, per the no-build scope].

One hygiene item cuts the other way: `dua-core` is currently declared in
`Cargo.toml` (`:48`, `:53`) but imported by no file under `src/` — it was
staged for this research. Under the recommendation below it should be
removed again in its own commit (not done here: research-only brief).

### R8 — custom ignore

Two distinct capabilities, and dua-core splits them:

1. **Programmatic override globs applied during traversal (pruning)** —
   supported natively, via `descend`: returning `false` "prunes descendants
   but still emits the entry itself" (`lib.rs:128-130`). Pruning is what
   saves the `readdir` of `node_modules` etc. A glob layer would combine the
   `descend` prune (directories) with a post-yield filter (files and the
   yielded-but-pruned directories).
2. **Custom ignore file names / automatic ignore-file reading** — absent.
   Nothing opens `.ignore`/`.gitignore`/anything. Every ignore-file
   semantic (discovery, parsing, precedence, negation, anchoring) is the
   caller's job.

The dua-cli blueprint for layering (their `WalkOptions::iter_from_paths`,
`src/common.rs`): one `is_excluded` closure built over
`gix::ignore::Search`, shared by the `descend` predicate (prune the walk)
and an iterator `.filter()` (drop the yielded entry), with the doc comment
"Excluding an entry means pruning it from the walk *and* from the emitted
events, so the predicate is shared between the two." Their matching scope is
narrower than ours must be: only explicit `--ignore-from` pattern files —
no per-directory chain, no global gitignore, no require_git gating.

### Git-less behavior

dua-core has no git dimension at all, so there is nothing to honor and no
knob to set — the question dissolves rather than failing. The incumbent
answers it concretely: with `require_git` at its default `true`
(`ignore-0.4.33/src/dir.rs:800`), git-family rules apply only inside a repo
— a `.git` (or `.jj`) directory must exist at or above the root
(`dir.rs:242-243`, `dir.rs:560-561`) — while `.ignore` files are honored
everywhere regardless (`dir.rs:572-578`). Our walker config leaves
`require_git` at that default (`src/workspace/walker.rs:91-100` never
touches it), so a git-less workspace today still gets `.ignore` + hidden
filtering, which dua-core could not reproduce without new code.

## Maintenance summary

- **Release cadence**: unusually active. dua-cli `v2.45.0` 2026-09-12,
  `dua-core v4.1.0` 2026-09-12, `dua-core v4.0.0` 2026-09-07,
  `dua-core v3.3.1` 2026-08-30, `v3.3.0` 2026-08-28, `v3.2.0` 2026-08-25,
  `v2.42.1` 2026-08-15 ("30% faster on macOS") — six plus tagged releases
  inside four weeks (GitHub releases, Byron/dua-cli).
- **Published standalone**: yes — `dua-core 4.1.0` resolves from
  crates.io with a checksum in our own `Cargo.lock:410-413`.
- **Issue health**: 179 issues total, **0 open**, all closed
  (list_issues, 2026-09-18).
- **jwalk lineage**: `Byron/jwalk` is `archived: true`, last push
  2026-08-05 (`pushed_at` in repo metadata) — matching the owner's date.
  The connecting statement is in dua-cli's own CHANGELOG: 2.40.0
  (2026-07-31) "Replace jwalk and Rayon with a crate-local walker built on
  crossbeam-deque and standard-library threads" (commit `8ada93f`); earlier
  entries show jwalk as dua's traversal engine back to 2020 (v2.3.x) and in
  parallel deletion (PR #353). jwalk's repo description itself points "Use
  `dua-core` instead" (as recorded in the superseded survey).
- **Perf claim caveat**: the headline numbers (40%, 30%) are measured
  against dua's *previous* engine, jwalk — not against `ignore`'s
  `WalkParallel`. No benchmark comparing dua-core with `ignore` exists in
  either repo that this pass could find. Combined with the walk cache since
  `5ad77cd`, traversal speed is not the axis where this decision should be
  made anyway.

## Recommendation: KEEP-IGNORE

The gates that fail are the ones this crate exists for. `WorkspaceWalker`'s
tested contract is the ignore chain plus the hidden toggle
(`src/workspace/walker.rs:121-195` pins both as golden output), and dua-core
4.1.0 implements none of it — R1 fails not marginally but absolutely: there
is no ignore subsystem to configure, only an absence. Adopting it means
re-implementing ripgrep-grade ignore semantics on top of a traversal engine,
which is the exact "rebuilding the wheel we already have" the superseded
survey warned about, now verified against the source.

The counterweight is thin: a cleaner per-entry iterator API than
`WalkParallel`'s callback (R5), error handling equal to what we have (R2),
and an unmeasured-vs-incumbent speed claim behind a cache that already
decoupled walk cost from invalidations. None of that pays for the chain
rebuild.

**ADOPT-WITH-LAYERING cost, for the record.** If ignore semantics were ever
dropped from the contract, or reduced to explicit pattern lists, dua-core is
the right engine and the layering is well-templated by dua-cli
(`src/common.rs`): build a matcher once per walk (globset — already a
direct dependency, `Cargo.toml:34`), share it between `descend` (prune
directories during traversal) and a post-yield filter (drop files and
pruned-but-yielded directories), wire `Order::Completion`,
`Options::default().skip_metadata()`, and a `RootWalk` over indexed roots in
place of the per-root `WalkBuilder` loop. Estimated size: of the order of
`walker.rs` today — roughly 100-150 lines including tests — **but** the
globset route covers only flat pattern lists: no nested `.gitignore`
discovery, no negation precedence across directories, no global gitignore,
no `.git/info/exclude`, no require_git gating. Restoring the incumbent's
full chain on top of dua-core means re-creating `ignore`'s
per-directory matcher inheritance (`src/dir.rs`, ~1000 lines of
precedence-tested logic) or pulling in a second ignore engine — dua-cli
solves this with `gix::ignore::Search`, which would drag the gitoxide stack
into this plumbing crate against its dependency policy. That trade is not
worth making for a fork whose matcher contract is already served.

## Sources

Local (file:line):

- `src/workspace/walker.rs` — current engine, config toggles, golden tests
- `src/server/state/workspace.rs` — `walk_blocking` spawn_blocking hop,
  walk-cache use
- `dua-core-4.1.0/src/lib.rs`, `dua-core-4.1.0/Cargo.toml`,
  `dua-core-4.1.0/CHANGELOG.md` (registry copy)
- `ignore-0.4.33/src/walk.rs`, `ignore-0.4.33/src/dir.rs`,
  `ignore-0.4.33/src/overrides.rs` (registry copy)
- `Cargo.toml`, `Cargo.lock`, `deny.toml` (this repo)
- `cargo tree` output, 2026-09-18 (quoted in R6)

Remote (GitHub MCP, Byron/dua-cli unless noted):

- Repo tree / `crates/` layout: default-branch tree `65c197d`;
  `crates/dua-lib/Cargo.toml` at tag `dua-core-v4.1.0`
- Releases: tags `v2.45.0`, `dua-core-v4.1.0`, `dua-core-v4.0.0`,
  `dua-core-v3.3.1`, `dua-core-v3.3.0`, `dua-core-v3.2.0`, `v2.42.1`
  (github.com/Byron/dua-cli/releases)
- `src/common.rs` (`IgnorePatterns`, `iter_from_paths`, descend+filter
  sharing), `src/aggregate.rs`, `src/traverse.rs` (consumer-side pattern
  checks) at commit `65c197d`
- `CHANGELOG.md` (root, commit `65c197d`): jwalk replacement in 2.40.0,
  `--ignore-from`, gitignore-aware cleanup marking (v2.35.0)
- Issues: `list_issues` — 179 total, 0 open; searches for
  `gitignore`/`jwalk`: 0 results
- Byron/jwalk repo metadata: `archived: true`, `pushed_at` 2026-08-05
