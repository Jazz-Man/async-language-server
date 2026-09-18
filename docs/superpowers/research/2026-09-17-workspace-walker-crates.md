# Workspace walker crates: is `ignore` reinventing the wheel?

Research question, 2026-09-17: are there ready-made crates for optimized,
possibly non-blocking (async) directory traversal that could replace
`ignore::WalkBuilder::build_parallel` in `WorkspaceWalker`
(`src/workspace/walker.rs`)? A new dependency is acceptable only if it
genuinely wins.

Shape we are shopping for: multiple roots, `.gitignore`/`.ignore` chain
respected (toggleable), hidden-file toggle, skip-and-warn on unreadable
entries, deterministic sorted output, running inside one
`tokio::task::spawn_blocking` call (landed) at walk-per-invalidation
frequency (walk-cache task upcoming). Not shopping for: glob matching.

## Candidates

| crate | what / parallelism | gitignore | maintenance | weight | MSRV |
|---|---|---|---|---|---|
| `ignore` 0.4.33 (pinned, `Cargo.toml`) | ripgrep's walker; `build_parallel` = own thread pool per call; batch callback API | **native**: `.ignore`, `.gitignore`, `.git/info/exclude`, global gitignore, parents, overrides, custom names — the reference implementation | active: latest release 2025-10-30+ (our `0.4.33` requirement resolves); BurntSushi | ~59 KB crate; deps globset, regex-automata, walkdir, etc. | none declared |
| `jwalk` 0.8.1 | rayon-parallel walk, streaming + sorted iterator API | **none** (deps are rayon + crossbeam only) | dead-ish: last release 2022-12-15; repo `main` marked `maintenance = "deprecated"`, description "Use `dua-core` instead" | ~1.5 MB dep tree, ~24 K SLoC (rayon + crossbeam) | none declared |
| `walkdir` 2.5.0 | serial iterator; already in our tree as `ignore`'s own dependency | **none** (manual `filter_entry` at best) | maintained, 2024-03 release; 45 M dl/mo | tiny | 1.34 |
| `async-walkdir` 2.1.0 | sequential async `Stream` over `async-fs` (each `read_dir` offloaded to a blocking-pool thread) | **none** (362 SLoC: WalkDir + filtering by hand) | last release 2025-01-27; ~83 K dl/mo | small (async-fs, futures-lite, thiserror) | none declared |
| `tokio::fs::read_dir` / `async-fs` / futures-stream hand-rolls | per-operation spawn_blocking hops; recursion, hidden filter, error tolerance, determinism all manual | **none** | n/a | tokio already present | n/a |

Search sweep ("async directory walker", lib.rs) surfaced only micro-crates —
`fast-walker` 0.2.1, `rsplug-walker` 0.5.1, `itools-walker` 0.0.1, `swdir`,
`nftw` — none with meaningful adoption and none with gitignore semantics.
The one alternative family with native gitignore handling is gitoxide's
worktree walk (`gix` stack) — unverified in this pass; it drags the full
gitoxide dependency weight into a plumbing crate, so not a contender here.

Sources: [jwalk lib.rs](https://lib.rs/crates/jwalk),
[jwalk docs.rs](https://docs.rs/jwalk/latest/jwalk/),
[jwalk main Cargo.toml](https://raw.githubusercontent.com/Byron/jwalk/main/Cargo.toml),
[jwalk crates.io API](https://crates.io/api/v1/crates/jwalk),
[async-walkdir lib.rs](https://lib.rs/crates/async-walkdir),
[walkdir lib.rs](https://lib.rs/crates/walkdir),
[ignore WalkBuilder docs.rs](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html),
[lib.rs search](https://lib.rs/search?q=async+directory+walker).

## The gitignore moat

`ignore` is the only candidate that implements the ignore-file chain
(`.ignore` > `.gitignore` > `.git/info/exclude` > global, nested-wins
precedence, plus hidden/parents toggles). Every async and every parallel
alternative has zero gitignore support: switching means re-implementing
chain evaluation on top of the new walker — rebuilding the wheel we already
have, with worse semantics (whitelist/negation precedence is subtle and
ripgrep-tested).

## Verdict

**Keep `ignore` + `spawn_blocking` + walk-on-invalidation cache.** Nothing
beats it on any axis that matters here:

- **(a) Latency per walk** — traversal is syscall-bound; `build_parallel`
  already saturates with its own thread pool. An async wrapper cannot make
  `readdir` faster, it only changes who blocks. Fresh thread-per-call
  spawning is noise at invalidation frequency behind the cache.
- **(b) Executor friendliness** — already solved by the single batched
  `spawn_blocking` (Task 1). The async alternatives would instead sprinkle a
  blocking-pool hop per directory (`tokio::fs`/`async-fs` offload each op),
  which is strictly worse for one bulk scan.
- **(c) Semantic parity** — unchallenged; see the moat above.

## Side findings

- `ignore`'s current docs.rs/latest documents
  `WalkBuilder::build_incremental` (`IncrementalIgnore`): lazily loaded,
  per-directory cached ignore matchers for filtering without re-walking.
  "Changes to it are not observed — build new matchers to reload," so
  cache invalidation on `.gitignore` edits stays manual. Potentially
  relevant to the walk-cache task; exact introducing version not verified
  in this pass.
- Web fetches through the reader hit stale caches: the crates.io API
  snapshot reported `ignore 0.4.25` (2025-10-30) while our committed
  `ignore = "0.4.33"` requirement demonstrably resolves — treat fetched
  "latest version" numbers as approximate; the gitignore/maintenance facts
  are not affected.
- `jwalk` is a caution: deprecated on the repo main branch without ever
  publishing a deprecation release, so crates.io still looks alive-ish.
