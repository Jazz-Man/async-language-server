# Rayon vs the current batch engine — prototype + bench (2026-09-18)

Research question: does replacing the per-item `spawn_blocking` load segment
with one Rayon batch under a single `spawn_blocking` hop yield a measurable
throughput win on the oneshot diagnostics bench? Prototype was measured on a
scratch copy of the tree and the tree was restored afterwards; this document
is the only artifact.

## Method

1. Baseline on the clean tree (`develop` @ 44d3c01): `cargo bench --bench
   oneshot_diagnostics -- --sample-size 30`, 2 runs. Criterion: 3 s warmup,
   10 s measurement pinned in the bench source, 30 samples per case.
2. The shipped corpus (256 tiny files) could not cleanly discriminate the
   load segment, so the bench was temporarily enlarged — exactly what was
   done:
   - new case `parallel_large`: 4096 files with a size spread of ~0.5 KiB to
     ~30 KiB (`512 + (i * 7919) % 30_000` bytes), ~64 MB corpus;
   - the per-document handler burn for that case reduced from 20 000 to 500
     iterations so the load segment dominates instead of the stand-in burn;
   - the original 256-file case kept unchanged.
   The enlarged bench ran 2× on the clean library (baseline), then the
   prototype ran 2× plus one stabilizing third run. All invocations
   identical; only the library sources differ between phases.
3. Prototype (hybrid): `rayon = "1"` added temporarily; in
   `refresh_workspace_documents` (`src/server/state/workspace.rs`) the
   `for_each_bounded` load segment was replaced by ONE `tokio
   spawn_blocking` hop executing, inside it,
   `loads.par_iter().map(load_one_sync).collect::<Vec<_>>()` on a pool built
   per refresh via `rayon::ThreadPoolBuilder::new().num_threads(width)`
   (`width` = `diagnostics_parallelism()`, `.max(1)` as the engine does; the
   global pool was not touched). `load_one_sync` is the stamp-probe →
   unchanged-skip gate → `read_to_string` → `insert_document` → stamp-write
   composite as a plain sync fn, the exact twin of the async
   `load_workspace_document`.
4. Preserved semantics: input-order results (`par_iter().map().collect()`
   preserves order), per-file failure → `tracing::warn!("skipping unreadable
   workspace file '{uri}': {error}")` + skip (never fatal), unchanged `urls`
   post-processing (`HashSet` dedup, sort, `retain_documents`).
5. Fidelity check before benching: `cargo nextest run --workspace
   --all-features -- workspace walker state::tests` → 99/99 PASS;
   `--no-default-features` leg → 95/95 PASS. The refresh suites
   (`workspace_refresh_preserves_open_documents`,
   `workspace_refresh_rereads_changed_files_and_keeps_untouched_ones`, walk
   cache, watcher tests) all exercise the hybrid path.
6. Restore: every touched file (`Cargo.toml`, `Cargo.lock`,
   `benches/oneshot_diagnostics.rs`, `src/server/state/workspace.rs`) was
   backed up before the first edit, copied back after benching, and the
   backup deleted. Verified: `git status --short` shows only the three
   pre-existing untracked files (two earlier research docs +
   `rust-perf-youtube.txt`), and `cargo check --workspace --all-targets` is
   green after a forced rebuild of both workspace crates.

Numbers below are criterion mean/median point estimates from
`target/criterion/oneshot_diagnostics/<case>/new/estimates.json` (and the
printed triples for run 1 of each phase). Criterion's `change:` lines
compare consecutive runs across code changes and were ignored; absolute
values only.

## Machine context

- Apple M3 Pro, `hw.ncpu` = 12 (6 performance + 6 efficiency cores),
  18 GB RAM (`hw.memsize` = 19 327 352 832), macOS, `/tmp` on APFS.
- Bench profile: cargo `bench` (optimized); pipeline width fixed at 4 in the
  bench (`WIDTH`), so both variants ran the load segment at parallelism 4.

## What exactly was compared

- Baseline: N per-file futures under `for_each_bounded(loads, 4)`
  (`buffer_unordered(4)`, input-order results); each future does
  `spawn_blocking(file_stamp)` → await → stamp gate →
  `spawn_blocking(read_to_string + insert_document)` → await → stamp write.
  N = 256 or 4096 → 2N tokio blocking-pool hops per refresh, against
  tokio's persistent blocking pool.
- Hybrid: ONE `spawn_blocking` hop per refresh; inside it a fresh
  `ThreadPoolBuilder().num_threads(4)` pool runs
  `loads.par_iter()` over the identical stamp → gate → read+insert → stamp
  composite, collecting in input order. Same width (4), same per-file
  failure handling, same downstream processing.

## Numbers

Mean ms (median in parentheses); 30 samples per case per run.

| case | variant | run 1 | run 2 | run 3 |
|---|---|---|---|---|
| `parallel` (256 files, full burn) | baseline (clean tree) | 8.046 (CI 8.002–8.095)† | 8.362 (8.393) | — |
| `parallel` | baseline (enlarged bench) | 8.107 (8.057) | 8.684 (8.486) | — |
| `parallel` | hybrid | 8.323 (—) | 8.487 (8.384) | 8.211 (8.184) |
| `parallel_large` (4096 files, spread) | baseline | 60.609 (59.745) | 59.122 (59.071) | — |
| `parallel_large` | hybrid | 60.205 (—) | 65.222 (60.557) | 60.179 (60.264) |

† mean point estimate from the printed criterion triple; per-run `estimates.json`
for that run was later overwritten, so its median was not captured. Cells marked
(—) likewise: only the printed triple was recorded for those runs.

Anchors: the untouched `glob_walk` cases stayed flat across all phases
(`build_gitignore_scale_set` ≈ 1.11–1.15 ms, `match_corpus_prebuilt` ≈
54–56 µs, `fresh_parallel_walk` ≈ 2.8–3.0 ms), and run-to-run drift on
identical code was 2.5% on `parallel_large` (59.1 → 60.6) and ~4–7% on
`parallel`. The hybrid's run 2 (65.2, wide CI) is machine variance; its
median (60.56) sits with the other runs.

## Where the time went

[Inference, from the numbers] At 4096 files the iteration is ~60 ms; the
hybrid removed all 8192 per-refresh tokio hop round-trips and replaced them
with one hop plus Rayon work-stealing at the same 4-thread width, and
throughput did not move. That says the two-hop scheduling overhead was never
the bottleneck: with 4 loads in flight the hop costs were already overlapped.
The segment is dominated by syscalls (4096 `stat` probes + 4096 open/read),
~64 MB of read + `Rope` construction, `DashMap` inserts, and the walk
itself — costs identical in both variants. Notably the hybrid additionally
pays a fresh `ThreadPool` construction (4 thread spawns) per refresh and
still matched the baseline, so even a cached-pool variant has at most a
marginal win here. Parse cost is ~0 in this bench (the bench matcher carries
no tree-sitter grammar, so `insert_document` only builds a `Rope`); a
grammar-carrying matcher would add per-file CPU parse work that either
engine can overlap at width — untested here.

## Dependency hygiene (if adopted)

- Versions: `rayon 1.12.0` + `rayon-core 1.13.0` were already resolved in
  `Cargo.lock` — criterion 0.8.2 (dev-dependency) already pulls rayon. The
  prototype's lock diff was exactly one line (the `rayon` edge on our crate);
  zero new packages, so `bans`/`licenses` exposure is unchanged from today.
- License: `MIT OR Apache-2.0` for both crates (read from the local registry
  sources) — on `deny.toml`'s allow-list.
- MSRV: `rust-version = 1.80` (rayon 1.12.0 and rayon-core 1.13.0) vs crate
  MSRV 1.90 — compatible.
- Duplicates: exactly one version each of `rayon`, `rayon-core`,
  `crossbeam-deque/-epoch/-utils` in the lock — `multiple-versions = "deny"`
  unaffected. `make deny` plausibly passes; not run (the lock contents are
  byte-identical to today's except our crate's dependency list).
- The real adoption delta is architectural: rayon would move from a
  dev-transitive dependency to a runtime dependency of the shipped library.

## Verdict: KEEP

The measured delta is zero to marginally negative: baseline `parallel_large`
59.1–60.6 ms vs hybrid 60.2 ms in its stable runs — inside the 2.5%+ noise
floor, with the hybrid carrying an extra per-refresh pool-construction cost.
The load segment's cost is syscall/IO/copy work that the existing
width-bounded engine already overlaps; the hop overhead the hybrid removes
was not on the critical path. Keeping rayon would add a runtime dependency
and a second scheduling system for no measured gain on this bench. Whether
that trade is worth it anyway is the brainstorm's call, not this research's.

Fidelity caveat for any future adoption discussion: the hybrid changes panic
containment — today a panic in one file's blocking task fails that file
(warn + skip); under Rayon a panic unwinds the whole batch and fails the
refresh. Untouched in the prototype (no panic path exists in the read
composite on valid input; panics on external input are forbidden by house
rules anyway), but a real adoption must decide it deliberately.

## rust-skills rules applied

- `conc-rayon-par-iter` — `par_iter()` for CPU-bound data parallelism; the
  rule's own caution (small collections may lose; scheduling overhead can
  dominate; tune granularity) is what the measurement confirmed for the
  IO-heavy shape of this workload.
- `conc-pattern-choice` — the pattern choice is measured before and after,
  not assumed; this document is that measurement.
- `async-spawn-blocking` — the blocking composite stays off the executor in
  both variants; the hybrid honors the rule's "pick one owner for
  parallelism" by bounding the Rayon pool at the same width rather than
  layering an unbounded `par_iter` over unbounded `spawn_blocking`.
