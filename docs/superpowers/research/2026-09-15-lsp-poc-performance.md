# LSP Performance Research: lsp-poc sluggishness — globset & ignore under suspicion

Date: 2026-09-15. Method: five-phase evidence-gated research
(log forensics → code audit → measurement → crate research → alternatives
matrix), driven from `/tmp/lsp-poc-perf-brief.md` (transient; this document is
the durable deliverable). Every claim is labeled **[measured]**,
**[inferred]**, or **[speculated]**. Web-sourced claims carry issue/release
references (phase 4 sources inline).

Question: the owner perceived sluggish LSP behavior (lsp-poc in Zed, debug
build; a release build "changed nothing") and suspected the `globset` /
`ignore` crates. Constraint added mid-research: **workspace diagnostics must
stay enabled** — fixes that work by disabling it are out of scope.

---

## 0. Executive verdict

**The owner's hypothesis is refuted.** `globset` 0.4.20 and `ignore` 0.4.33
are the current crates.io releases, fit for purpose, and measure *fastest at
this crate's access patterns* against both alternatives researched
(fast-glob, zlob — §3). The sluggishness has three real, ranked causes:

1. **A full workspace refresh runs synchronously on the executor for every
   `workspace/diagnostic` poll**, and each poll recomputes diagnostics for
   every walker-visible `.md` file with no unchanged-report reuse.
   ≈ **130 ms of blocked single-thread executor per poll** [measured
   components, §2]; 185 serialized polls were captured ≈ **24 s of pure
   executor-blocked recompute** [arithmetic]. The framework's
   `refresh_workspace_documents` walk is synchronous and the consumer's
   `result_id: None` forbids client-side reuse.
2. **Per-keystroke full markdown re-parse + whole-document `String`
   materialization** in the consumer's `didChange → publish` path — tens of
   ms hitches (worst measured file: 34.9 ms parse alone). The framework
   offers no derived-data cache, so every consumer re-parses (§2.4, §4c).
3. **Debug-build crate logging** bridged through `tracing-log`: 84% of the
   5.1 MB log is GlobSet-construction logging. Debug-profile-only; the
   confirmed-unset `RUST_LOG` means release runs were quiet — consistent
   with "release changed nothing" (the compute in (1)/(2) remains).

The release ≈ debug observation was the tell: compute-bound matching would
have sped up in release; I/O, serialized compute, and per-poll work did not
[brief hypothesis, confirmed by §2].

---

## 1. Log forensics (phase 0) — [measured]

`/Users/vasilsokolik/www/lsp-poc/lsp.log`: 43,268 lines / 5.07 MiB, 100%
DEBUG, **zero timestamps** (`without_time()` — rate analysis impossible).

| category (target) | lines | % of bytes |
|---|---|---|
| glob→regex conversions (`globset`) | 37,740 | **81.2%** |
| walk skip events (`ignore::walk`) | 4,232 | 16.0% |
| "built glob set" summaries (`globset`) | 555 | 1.5% |
| gitignore opens (`ignore::gitignore`) | 740 | 1.3% |
| actual LSP traffic | 1 (`didSave` of the log itself) | 0.003% |

- **185 `workspace/diagnostic` requests** — the only request method in the
  window, serialized, one root (`~/www/lsp-poc`). 1 walk per request.
- Per request: 3 gitignore GlobSet compiles — global `~/.gitignore` **twice**
  (once in-span on the request task, once on a walker worker thread) + repo
  `.gitignore`. 204 glob→regex conversions/request, 37,740 total, for 103
  unique patterns (~366 redundant recompiles per pattern).
- Explanation: (b) — construction + tracing overhead, not match/walk volume.
  The walk itself is tiny (~23 skip events per walk).

---

## 2. Code-path audit + measurements (phases 1–2)

Framework paths verified against **v0.10.0** (what lsp-poc pins):
`git diff v0.10.0..develop -- src/workspace src/documents src/server` is one
line (`document.rs`, the tree-sitter 0.27 `captures()` fix) [measured].

### 2.1 Construction sites

- The framework's matcher `GlobSet`s build **once** at startup
  (`src/server/state/mod.rs:120`). Never in the log. [measured]
- Every globset construction in the log is the `ignore` crate compiling
  gitignore chains **inside each walk**; `WalkBuilder` is re-instantiated per
  `refresh_workspace_documents` call (`src/workspace/walker.rs:56-88`,
  `src/server/state/workspace.rs:86-90`). Upstream documents reuse ("prefer
  one builder over many `Walk` iterators", ripgrep walk.rs:718) and ships no
  cross-walk gitignore cache; `IncrementalIgnore` (0.4.30) targets
  single-path checks only.

### 2.2 The dead capability flag (framework defect)

lsp-poc advertises `workspace_diagnostics: false`
(`server.rs:276-279`) — **dead**: the framework default
`WorkspaceDiagnostics::Enabled` force-overrides the advertised capability
after the implementor's block is built (`options.rs:63-72`,
`diagnostics.rs:145-165`; lsp-poc never overrides `server_options()`,
`server_trait.rs:33`). Hence Zed polls, hence 185 walks. **[measured code
paths; behavioral confirmation via the log]** The one-line consumer opt-out
exists, but the owner requires the feature — the flag still must not lie;
see §4d.

### 2.3 The executor stall

`walker.files()` is a synchronous parallel scan (`build_parallel().run()`
parks the calling thread) plus a full sort, running on the request task's
thread. lsp-poc runs `#[tokio::main(flavor = "current_thread")]`
(`main.rs:20`) — one executor thread; every other in-flight request waits
[inferred from code structure; magnitude measured in §2.5]. Only the
per-file stamp probes and loads are on `spawn_blocking`; the walk is not.

### 2.4 Consumer hot paths (lsp-poc)

- `didChange → publish → compute_diagnostics`: whole-document
  `text_contents()` (String materialization; the framework documents
  `text_reader` as the allocating escape) + **full tree-sitter markdown
  re-parse from scratch** — the framework's incrementally-maintained tree is
  not used for this [measured code paths].
- `document_snapshots` / `resolver` re-parse **every open document** per
  references/rename request and per resolved target — no derived-data cache
  (the code comments admit "the established POC cost"). The disk-file
  `Index` is stamp-cached; open documents are not.
- `document_diagnostics` always returns Full with `result_id: None`
  (`server.rs:320`) — the client cannot reuse anything across polls.
- Single `MarkdownParser` behind `Arc<Mutex<…>>` serializes all parses
  [measured; contention unquantified].

### 2.5 Numbers (release, this machine) — [measured]

| measurement | result |
|---|---|
| compile `~/.gitignore` (307 ignore + 10 whitelist patterns; 100 fall to Regex strategy — the log's 100 conversions/compile) | **1.06 ms** median |
| compile repo `.gitignore` | ~40 µs |
| fresh parallel walk, lsp-poc root (58 walker-visible files) | **3.83 ms** median |
| markdown parse, per-file avg / worst | ~4.1 ms / **34.9 ms** |
| full recompute of walker-visible `.md` (30 files) | **124 ms** |
| criterion `oneshot_diagnostics/parallel` (framework fixture) | 8.3 ms (unchanged across runs) |
| new `glob_walk/build_gitignore_scale_set` (100 synthetic mid-path-`**` patterns) | 1.11 ms |
| new `glob_walk/match_corpus_prebuilt` (2000 paths) | 55.7 µs (~28 ns/path) |
| new `glob_walk/fresh_parallel_walk` (500 files / 50 dirs) | 2.90 ms |

**Per-poll composition**: ~1.1 ms request-thread gitignore compile + ~3.8 ms
walk + ~124 ms recompute ≈ **130 ms executor-blocked** [measured components;
composition arithmetic]. `.md` counts reconciled: 61 on disk → 54 outside
`vendors/` → 30 walker-visible per poll [measured].

---

## 3. Crate research + alternatives matrix (phases 3–4)

### 3.1 globset / ignore verdicts — fit-for-purpose; misused at the call site

- Pins are current (both released 2026-08-04; identical in lsp-poc's lock and
  the framework). **An upgrade buys nothing** [measured via crates.io].
- globset's `converted to regex` debug lines fire only inside `GlobSet::new`
  for Regex-strategy globs — the log volume is a **rebuild-frequency
  signature** [verified in source]. Single-glob `compile_matcher` logs
  nothing — why the matcher's one `**/*.md` glob never appears.
- Known upstream perf gaps do not apply: #1086/#3488 (case-insensitive
  literals → regex) — no `case_insensitive` configuration here; #2789 (no
  glob-based walk pruning) — inherent design, not misuse.
- Full matrix: `/tmp/lsp-poc-perf-phase4.md` (transient); summary below.

### 3.2 Alternatives — measured (release; 275-path corpus, real `~/.gitignore` = 317 patterns)

| scenario | globset 0.4.20 | fast-glob 1.1.1 | zlob 1.6.5 |
|---|---|---|---|
| full-set build (317 patterns) | 619 µs | n/a (no build) | n/a (lazy) |
| match-all, 307 patterns × 275 paths | **53 µs** | 1131 µs (**21× slower**, no set API) | 346 µs (**6.5× slower**, per-glob batch FFI) |
| single `**/*.md`, per path | **45.8 ns** | 4.7× slower | 5.1× slower per-path (its batch mode is 4.8× faster but pattern-major — the inverse access pattern) |

Parity findings [all measured]: fast-glob's `*` never crosses `/` (POSIX) —
1 divergent pair in 87,175 (`*.log` × a deep path), but the framework's
matcher was written against globset's crossing default
(`src/documents/matcher.rs:173`), so adoption silently changes which URLs
match; fast-glob also NOTs leading-`!` patterns. zlob (a **Zig-written C
library with a Rust binding**) needs `ZLOB_PERIOD` for dotfile parity, needs
**zig 0.16.0 + libclang in every consumer build**, MSRV 1.85, single
maintainer, 7 months old.

**Verdict: STAY with globset.** The candidates lose on the rows that matter
(the matcher's access pattern: many globs × one path), carry semantics risk,
and zlob adds a foreign build toolchain to a git-dependency crate. The real
lever is caching the built GlobSet per matcher / across walks, not swapping.
*Mandatory statement*: neither is a walker — the `ignore` walker is out of
this comparison's scope by construction.

---

## 4. Ranked bottlenecks and fix roadmap (workspace diagnostics stays ON)

Fix tiers: config change < code fix < dependency swap. No swap is warranted.

| # | bottleneck | evidence | fix tier |
|---|---|---|---|
| 1 | Sync walk on the single executor per poll (§2.3) | 3.8 ms walk + 1.1 ms compile per poll, every other request stalls [inferred stall; components measured] | **Framework code**: offload `refresh_workspace_documents` walk to `spawn_blocking`; cache the walk result across polls (invalidate on folder/generation change) |
| 2 | Full per-poll recompute with `result_id: None` (§2.4) | 124 ms per poll [measured] | **Consumer code**: stable `result_id` + Unchanged reports; **framework design follow-up** (c) below |
| 3 | Per-keystroke full re-parse + `text_contents()` (§2.4) | worst-file parse 34.9 ms [measured] | **Consumer code**: reuse the framework tree / `text_reader`; cache `MdIndex` per document version (mirror the disk `Index`) |
| 4 | gitignore GlobSet rebuild per walk (§1, §2.1) | 2×1.06 ms + ~40 µs per request [measured]; ~366 recompiles/pattern in the window | **Framework code**: fold into #1's walk cache; `IncrementalIgnore` for targeted checks |
| 5 | Dead `workspace_diagnostics` capability (§2.2) | force-override chain [measured] | **Framework design**: honor the consumer's flag or document the override (not a disable path — the feature stays) |
| 6 | Debug-bridged crate logging (§1) | 84% of log bytes, ~28 KB/request | **Config**: debug-profile artifact; enable timestamps (drop `without_time`) for future forensics |

**Framework design follow-up (c)**: a version-keyed derived-data cache hook
on `Document`/`ServerState` so consumers stop re-parsing per request. This is
the API gap that *caused* the POC's pattern; it deserves its own
brainstorming → spec cycle before any code.

---

## 5. Open questions

1. Zed's `workspace/diagnostic` polling cadence — needs a re-run with
   timestamps (log has none).
2. Whether the mutex on the shared `MarkdownParser` measurably serializes
   concurrent requests — unquantified; likely subsumed by fixes 1-2.
3. Ownership of the `result_id` scheme: consumer-implemented now, or a
   framework helper (paired with the derived-data cache design).

## 6. Artifacts

- In-repo: `benches/oneshot_diagnostics.rs` gained the `glob_walk` group
  (rebuild / match-throughput / fresh-walk costs) — the reproducible form of
  §2.5's numbers; `make bench` runs it on demand.
- Transient scratch (not durable): `/tmp/lsp-poc-perf-phase{0,1,3,4}.md`,
  `/tmp/perf-probe`, `/tmp/perf-compare`.

---

*Verdict: the crates are innocent; the per-request lifecycle is guilty. Fix
the lifecycle — offload, cache, reuse — and keep workspace diagnostics on.*
