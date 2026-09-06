# Performance at scale — Design

**Cycle opened:** 2026-09-06 · **Design finalized:** 2026-09-06 · **Branch:** `feature/perf`
**Inputs:** hotspot research
`docs/superpowers/research/2026-09-06-performance-hotspots-research.md`
(sonnet[1m], read-only, file:line citations, 15 hotspots / 12 candidates);
async-lsp 0.2.4 sources (concurrency.rs, stdio.rs); rust-skills consultation
(`async-spawn-blocking`, `conc-thread-budget`, `async-joinset-structured`,
`perf-profile-first`).
**Status:** presented section-by-section and approved in brainstorm (owner,
2026-09-06), including the async-io verdict (rejected — see Rejected
alternatives).

## Goal

Make the crate fast at very large workspace scale (tens of thousands of
files — PHP Symfony / Magento 2 class) with **all heavy lifting under the
hood**: downstream `Server` implementors get parallelism and incremental
refresh by default, without opting in. Two surfaces:

- **Batch** — `workspace/diagnostic` and `oneshot::workspace_diagnostics`:
  bounded-parallel diagnostics, incremental (mtime+size-gated) refresh,
  no O(N²) report merge, streaming memory shape in oneshot.
- **Interactive** — every request through the dispatch engine (all methods
  equally): one conversion-document snapshot per request instead of four
  clones and up to two synchronous disk reads.

Plus: `serve()`'s request limit adopts async-lsp's core-derived default, a
`ServerOptions::with_diagnostics_parallelism` knob (cores by default), and a
criterion benchmark harness so future claims are measured, not inferred.

Backward compatibility is explicitly not a constraint (owner: the crate has
never run in production, only POC) — though this cycle's public surface
changes are additive (one knob) plus one behavior-visible swap (limit 8 →
cores).

## Owner decisions log

1. **Approach 3 of the scope proposal** ("full throughput rework"), with
   `Arc<Document>` storage split into its own follow-up spec (owner accepted
   the split): this cycle = batch scale + engine hygiene + parse/matcher
   micro-costs + benchmarks.
2. **Cores by default, knob to narrow.** `ServerOptions` gains
   `with_diagnostics_parallelism`; default is `available_parallelism()`.
   The serve() middleware limit becomes `ConcurrencyLayer::default()`
   (also cores) — it **cannot** read `ServerOptions` because the stack is
   built in `serve()` before `initialize` reveals the implementor's options
   (verified: options come from `Server::server_options()` inside the
   wrapper). No knob for the middleware limit.
3. **All methods optimized equally** — hover/definition were examples; the
   engine work lives in the shared dispatch engine (`macros/src/dispatch.rs`),
   covering every URL-anchored and resolve-family request.
4. **Parser reuse (candidate 12) → backlog, disclosed**: the win is only the
   `Parser::new()` allocation [Inference]; parsers are not `Sync`, so
   parallel sections would need ownership games for a marginal prize.
5. **`ServerOptions` growth question retracted by the owner** — future
   options design (flat vs extension point) is deliberately undecided and
   NOT recorded as backlog or memory.
6. **async-lsp's `async-io` feature rejected** (owner asked, verdict
   accepted): its entire footprint is two
   `unsafe impl async_io::IoSafe for PipeStd{in,out}` blocks
   (async-lsp stdio.rs:138-139, 194-195) — runtime glue letting the
   smol/async-std reactor drive the pipe transport. It is ecosystem
   compatibility, not a performance feature; zero intersection with this
   cycle's work; enabling it adds a dead dependency.
7. **Benchmarks wanted by the owner** — criterion harness in scope.

## Verified findings (research evidence, citations in the artifact)

- `refresh_workspace_documents` re-walks all roots and re-reads + fully
  re-parses every matching file on **every** `workspace/diagnostic`
  request; only Open-origin docs skip; no mtime/size/result-id
  incrementality (state/workspace.rs:99-115).
- Diagnostics loops are strictly serial in both surfaces
  (workspace/diagnostics.rs:399-432; oneshot/workspace_diagnostics.rs:227-234);
  oneshot holds every matching file's text in memory up front (:218).
- Report merge is O(N²) `Url` comparisons (diagnostics.rs:501-510) — ~10⁸
  at 10k documents.
- The dispatch engine clones the full `Document` 4× per request and, for
  untracked file URLs, performs 2 synchronous `std::fs` reads + 2 rope
  builds on the executor (macros/src/dispatch.rs:162-203;
  with_state/mod.rs:58-76). Resolve/URL-less requests clone **all**
  tracked documents just to test `len() == 1` (with_state/mod.rs:47-53).
- Parse sites materialize the whole document as a fresh `String` per parse
  (`text_contents()`, document.rs:85-87) although a rope chunk reader
  already exists (`text_reader`, document.rs:70-78).
- Refresh converts every walked path to a `Url` and the matcher converts it
  back to a path — two conversions per file, matching decided after the
  first (workspace.rs:93-94, matcher.rs:168-175).
- async-lsp's `ConcurrencyBuilder::default()` = `available_parallelism()`
  (concurrency.rs:183-185); the crate's `8` traces to the original
  filiptibell repo's `ConcurrencyLayer::new(NonZeroUsize::new(8).unwrap())`
  (fork initial commit a33e243), reason unrecorded anywhere.

## Design

### Architecture

One bounded-parallel batch engine (width = `available_parallelism()`,
narrowable via `ServerOptions`), applied to refresh loads and diagnostics
in both surfaces; a conservative mtime+size gate in front of every
workspace reload; an indexed report merge; a single-snapshot dispatch
engine. Defaults require nothing from downstream implementors. Ordering of
all outputs stays deterministic (index-tagged results + the existing final
sort); `CONTENT_MODIFIED` semantics are unchanged (a version change on any
open document still aborts the whole workspace request with a retry —
parallelism changes throughput, not protocol behavior).

### Components

1. **serve() limit** — `src/server/serve.rs`: replace
   `ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS)` with
   `ConcurrencyLayer::default()`; delete the constant. The tripwire test
   `at_most_eight_requests_run_concurrently` computes the limit from
   `available_parallelism()` instead of assuming 8 (its deadlock-absence
   semantics and abort-on-join behavior are unchanged).
2. **Batch engine** — `pub(crate) async fn for_each_bounded<T, R>(items,
   width, f) -> Vec<R>` in a new `src/workspace/parallel.rs`: spawns the
   per-item futures on a `JoinSet` with at most `width` in flight, tags
   results with their index, restores input order. Used by
   `workspace_diagnostic_items` (diagnostics stage and the refresh read
   stage) and by the oneshot loops. `ServerOptions::with_diagnostics_parallelism(usize)`
   (default cores) flows from the implementor's `server_options()` into
   both call sites. Oneshot stops pre-collecting all texts: files stream
   through the same bounded engine (read + open + parse + diagnose per
   item), bounding peak memory at width documents instead of M.
3. **Incremental refresh** — `DocumentEntry` records `(mtime, size)` from
   `fs::metadata` at load; `refresh_workspace_documents` probes metadata
   (cheap) and skips read+insert+parse for unchanged Workspace-origin
   docs; any doubt (missing metadata, changed stamp) re-reads. Refresh
   also matches on the walked `Path` before building a `Url` (one
   conversion per file, non-matching files cost nothing).
4. **Report merge** — index reports by `Url` (`HashMap<Url, usize>`) in
   `push_workspace_report`; the existing final sort remains the output
   normalizer.
5. **Dispatch engine** — `macros/src/dispatch.rs`: the version snapshot
   and the staleness re-check read the version through a clone-free
   `ServerState::document_version(url)` accessor instead of full
   `Document` clones; the conversion document is computed once per
   request, and after the staleness check passes it is guaranteed valid
   (a version mismatch already returned `CONTENT_MODIFIED`), so response
   conversion reuses it — one full clone per request instead of four, and
   untracked-URL disk reads 2 → ≤1. Sole-document detection counts without
   collecting (a `document_count()` on `ServerState`) instead of cloning
   every `Document`. Mirror tests in the macro suite are updated to the
   new emission shape.
6. **Parse path** — parse sites take the rope chunk reader
   (`Document::text_reader`) instead of `text_contents()`'s whole-file
   `String` (insert, incremental re-parse, full-replace, didSave).
7. **Benchmarks** — `benches/` + criterion dev-dependency: oneshot
   diagnostics over N synthetic documents with a CPU-bound synthetic
   handler; `harness = false`; compiles in all three feature
   configurations; not part of the CI battery (on-demand, like
   `cargo dupes`). Structural parallelism is pinned by a deterministic
   gate test (N gated handlers, width N — all enter; width 1 — strictly
   sequential), not by wall-clock asserts.

### Data flow

`workspace/diagnostic`: walk (metadata-level) → mtime gate →
bounded-parallel read+parse of changed files → bounded-parallel
diagnostics → indexed merge → sort → report. Oneshot: the same engine,
clientless, streaming. Interactive (every method): extract URL →
version snapshot → one conversion document → handler → staleness →
response conversion reusing the snapshot.

### Error handling

No new error types. The mtime gate is silent and conservative (doubt ⇒
re-read). `CONTENT_MODIFIED` retry semantics unchanged. Fire-and-forget
client requests unchanged. The knob is additive. The one behavior-visible
change — request limit 8 → cores — is stated for the owner's commit
message per product.md.

## Rejected alternatives

- **`Arc<Document>` storage now** (kills all snapshot clones, breaks
  `document()`'s return type) — its own follow-up spec, sanctioned as a
  breaking change by the owner's no-compatibility stance.
- **previousResultIds end-to-end contract** — needs downstream cooperation
  (stable `result_id`s); deferred until a real downstream exists.
- **Parser instance reuse (candidate 12)** — decision 4.
- **Parallel walk (`ignore::build_parallel`)** — with the mtime gate the
  walk is metadata-only; reads dominate [Inference]; revisit with bench
  data.
- **async-lsp `async-io` feature** — decision 6.
- **Extension slot / generics on `ServerOptions`** — owner retracted the
  question (decision 5); flat struct stands until the owner decides
  otherwise.
- **Async IO in notification handlers** — the `std::fs` reads in
   didClose/didChange/didSave/watched-files handlers are deliberate:
   LSP and async-lsp require synchronous notification handlers
   (documented in-code, `src/server/state/documents.rs:230-237`). Not a
   defect; the dispatch-engine and refresh reads (request paths) are the
   ones this cycle fixes.
- **Wall-clock asserts in CI** — flaky; structural gates + on-demand
  criterion benches instead (`perf-profile-first`).

## Testing

- Updated: tripwire (dynamic limit), macro mirror tests (single-snapshot
  emission), oneshot tests (streaming shape).
- New: mtime gate (a counting server observes read/parse counts — touch a
  file ⇒ re-read; untouched ⇒ skipped), structural parallelism gates
  (deterministic, channel-gated, `WIRE_TIMEOUT`-style bounds), criterion
  bench compiles in all three configurations.
- Full battery in all three feature configurations; real temp workspaces;
  no sleeps for synchronization.

## Backlog (recorded in the SDD ledger, not this cycle)

`Arc<Document>` store (own spec) · previousResultIds contract · tree-sitter
query cache · semantic-tokens cache eviction · parser reuse · parallel
walk · real-workload profiling on the owner's first PHP-scale downstream.
