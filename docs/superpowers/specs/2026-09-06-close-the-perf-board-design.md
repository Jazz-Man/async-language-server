# Close the perf board — Design

**Cycle opened:** 2026-09-06 · **Design finalized:** 2026-09-06 · **Branch:** continuation of `feature/perf` (re-branching is the owner's)
**Inputs:** research `docs/superpowers/research/2026-09-06-performance-hotspots-research.md`
(perf cycle) and
`docs/superpowers/research/2026-09-06-document-handle-and-cache-research.md`
(this cycle, sonnet[1m], read-only, file:line citations incl. vendored
tree-sitter C sources); rust-skills (`own-arc-shared`, `api-service-clone`,
`api-no-wrapper-params`); the delivered perf cycle's ledger and review trail.
**Status:** presented section-by-section and approved in brainstorm (owner,
2026-09-06), conditional research completed before presentation.

## Goal

Close the remaining performance board in one cycle: make `Document` a
cheap-Clone handle (the flagship — every `document()`/`documents()` clone
becomes one refcount bump with **zero public API change**), move eager
tree-sitter parses off the executor inside the load composite, cache
compiled tree-sitter queries, evict the semantic-tokens cache on document
lifecycle events, parallelize the workspace walk, and land the two
one-liner follow-ups. Every change rides the same constraints as the
delivered perf cycle: behavior contracts (snapshot semantics,
`CONTENT_MODIFIED`, deterministic output order) unchanged unless a section
says otherwise.

## Owner decisions log

1. **Close the whole board** — scope chosen from three options (flagship+
   follow-ups / flagship only / everything); the owner picked everything
   actionable, accepting the larger cycle.
2. **Cache invalidation matrix accepted**: the query-compile cache carries
   no semantic invalidation (a compiled query is a pure function of its
   key — documented, not hand-waved); the semantic-tokens cache gets full
   event-driven eviction; no LRU cap on the compile cache (YAGNI — real
   servers use fixed query sets; a cap is one line if ever needed).
3. **A1' approved conditional on research** — the owner required
   rust-skills consultation AND a codebase investigation before commiting
   to "Document-handle without a breaking change". The research confirmed:
   fields `pub(crate)`, no public constructor, mutation confined to
   `state/documents.rs` under the DashMap guard → zero downstream
   breakage. The promised breaking change of the earlier spec is
   unnecessary and is formally superseded by this design.
4. **Out of scope, externally gated** (owner acknowledged with examples):
   the previousResultIds end-to-end contract (needs a real downstream to
   shape who computes ids) and real-workload profiling (needs the
   downstream to exist; until then all wins are structural).
5. **Parser instance reuse closed by rationale** inside the composite:
   `Parser::new()` per file inside the blocking pool is one small
   allocation [Inference]; no code ships for it.

## Verified findings (this cycle's research)

- `Document`'s fields are `pub(crate)`; there is no public constructor;
  every mutation site lives in `src/server/state/documents.rs` under the
  DashMap guard (research §1–2). The handle conversion cannot break
  downstream construction/mutation because none exists outside the crate.
- `uri` and `language` are construction-immutable; `matcher` and the
  grammar are rewritten in place by didSave (`documents.rs:286`, `:306`) —
  the Inner split must place them accordingly (research §3).
- A naive COW would clone `uri`/`language` strings on every didChange
  batch — a regression on the hottest path. The split design reduces a
  change-batch COW to two Arc bumps + an `i32`: no string allocations.
- `tree_sitter::Query` retains its `Language` (`ts_query_new` /
  `ts_query_delete`, vendored `query.c:3050`/`:3220`); `Language` clone is
  a free wrapper with pointer `Eq`/`Hash`; `Query` is `Send + Sync`. A
  cache keyed by `Language` whose values own a `Language` clone plus the
  `Query` is sound with no lifetime games (research §4).
- The semantic-tokens cache has zero evictions today; eight definite
  removal sites and five staleness/replacement sites exist to hook
  (research §5).
- `ignore::WalkBuilder::build_parallel` is callback/pool-based with no
  ordering guarantee; the walker's sorted contract comes from the
  post-collection sort (`walker.rs:80`), which a parallel walk can keep
  (research §6).
- **Invariant (load-bearing):** the handle stays COW under the DashMap
  guard — no interior mutability — or the documented snapshot contract
  (`state/mod.rs:70-76`) and the `CONTENT_MODIFIED` staleness protocol
  break.

## Design

### Architecture

`Document` becomes a handle around `Arc<DocumentInner>`; all readers keep
working through the unchanged public signature and `Deref`-transparent
field access (crate-internal). Writers rebuild the Inner (copy-on-write)
under the same DashMap guard as today, sharing an immutable meta-Arc so a
change batch costs refcounts, not allocations. Four satellites ride along:
the blocking-pool load composite (reads AND eager parses off the
executor), the query-compile cache, semantic-tokens eviction, and the
parallel walk.

### Components

1. **Document handle** — `Document { inner: Arc<DocumentInner> }`;
   `DocumentInner { meta: Arc<DocumentMeta>, matcher:
   Option<Arc<DocumentMatcher>>, version: i32, text: Rope, tree_sitter_lang,
   tree_sitter_tree }`; `DocumentMeta { uri: Url, language: String }`
   (construction-immutable, shared across COW generations). `Clone` =
   one Arc bump. didChange/didSave/watched-file reloads rebuild the Inner
   under the guard (`Arc::make_mut` semantics are NOT used across shared
   handles — the store always installs a fresh Inner, so outstanding
   handles keep their snapshot). Field accessors on `Document` forward
   through `inner`; `pub(crate)` field visibility is replaced by
   `pub(crate)` accessor forwarding inside the crate.
2. **Load composite** — the refresh and oneshot per-file tasks move
   read + `insert_document` (the eager parse) into their `spawn_blocking`
   closures; the executor never runs a parse. Parser reuse is closed by
   decision 5.
3. **Query-compile cache** — a per-matcher map on `DocumentMatchers`
   (each matcher carries at most one grammar): keyed by the query string,
   each value owning the matcher's `Language` clone + the compiled
   `Query` (grammar-identity soundness per the research: `Language` has
   pointer `Eq`/`Hash`, the `Query` retains it). `Document::query`
   consults the cache through its stored matcher handle; compilation
   happens once per distinct query string. The whole-file `String`
   materialization in `query` is attempted away via tree-sitter's
   `TextProvider` bridge (the `parse_rope` precedent); if the API does
   not cooperate during implementation, the materialization stays and the
   limitation is documented — the compile cache lands regardless
   (compilation is the dominant cost).
4. **Semantic-tokens eviction** — remove the cache entry at the removal
   sites the research enumerated (didClose, watched-file rename/delete,
   refresh-retain removal); content staleness continues to be governed by
   the delta protocol's `result_id` contract.
5. **Parallel walk** — `WalkBuilder::build_parallel` with a collecting
   visitor; the existing post-collection sort preserves the deterministic
   sorted output contract.
6. **Follow-ups** — `gated_setup` temp-dir cleanup (T8 review Minor);
   one doc sentence in the bench noting the group's pinned 10 s
   measurement overrides `--measurement-time`.

### Cache invalidation matrix (spec-mandated section)

| cache | semantic invalidation | lifecycle | growth bound |
|---|---|---|---|
| query-compile | none possible — compiled query is a pure function of `(matcher's grammar, query string)`; keys are stable | value owns the `Language`, so matcher/grammar outlives entries; entries die with the matcher | distinct query strings (fixed sets in practice; no cap — decision 2) |
| semantic-tokens | protocol-managed: delta requests carry `result_id`; server answers full when it cannot delta | event-driven eviction at the eight removal sites | tracked documents |
| FileStamp gate (delivered) | conservative re-probe every refresh | stamp lives on the entry | n/a |
| Document handles | not a cache — snapshot semantics are structural (old handle sees old content) | Arc reclamation | n/a |

### Data flow

Unchanged shapes: requests snapshot handles (now refcount bumps);
didChange rebuilds Inner under the guard; refresh/oneshot load tasks run
read+parse on the blocking pool and install fresh handles; walkers
collect in parallel, sort once.

### Error handling

No new error types. COW is transparent; the composite's blocking closures
already thread `io::Error` through `ServerError::Io` (delivered cycle);
eviction is a map removal; the compile cache compiles on miss with the
existing `QueryError` surface.

## Testing

- Existing snapshot-semantics tests must pass **unmodified** — they pin
  the handle's contract (the research's invariant).
- New: COW structural test (clone a handle → didChange → old handle sees
  pre-change text, fresh `document()` sees post-change); compile-cache
  test (same query string → same compiled query identity; different
  grammar → different entry) where observable, else a hit/miss counter;
  eviction tests (didClose/rename → entry gone); parallel-walk
  determinism test (same input → same sorted output as serial).
- Full battery ×3 configurations; the criterion bench continues to guard
  the batch pipeline.

## Documentation sync

structure.md (Document handle + Inner split note; blocking-pool loads;
query cache; parallel walk), testing.md (new test families if patterns
change), tech.md (unchanged battery). All in English.

## Rejected alternatives

- **A1 — `Arc<Document>` in public signatures**: violates
  `api-no-wrapper-params` (sharing is not the API — the value is);
  breaks move-sites. Superseded by the handle.
- **A2 — `ArcSwap<Document>`**: lock-free retry machinery for a
  didChange-frequency write path; overkill.
- **LRU cap on the compile cache**: YAGNI (decision 2).
- **Interior mutability inside the handle**: breaks the snapshot
  contract and `CONTENT_MODIFIED` (the research's invariant).
- **previousResultIds contract / real-workload profiling**: out of scope,
  externally gated (decision 4) — they open the NEXT cycle after the
  first real downstream exists.
