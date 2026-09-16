# Research: derived-data cache design (framework API for per-document memoization)

Date: 2026-09-16 · Branch context: `feature/dupes` (read-only research) · Feeds the brainstorm for the
derived-data cache cycle. Upstream context: lsp-poc re-derives per-document indexes from scratch per
request; [measured, owner-reported] full re-parse of 30 .md files ≈ 124 ms per `workspace/diagnostic`
poll, and lsp-poc's resolver re-parses every open document per resolved link target.

Framework facts this rests on (all verified against the working tree):

- `Document` (`src/documents/document.rs:37-52`) is a cheap-clone handle over
  `Arc<DocumentInner>`: construction-immutable `Arc<DocumentMeta>` (uri, language), a
  per-generation `version: i32` (`document.rs:169-176`), the rope, and under the feature a
  framework-managed tree-sitter tree. Writes install a fresh generation
  (`Document::from_shared_meta`, `document.rs:93-117`); outstanding clones keep their snapshot.
- `ServerState` (`src/server/state/mod.rs:20-30`) is the cheap-clone interior-mutable handle.
- MSRV is **1.90** (`Cargo.toml:9`, `rust-version = "1.90"`). [note] `tech.md` still says
  1.88 — stale; worth a doc fix in some commit touching it.

---

## R1 — Minimal consumer-side caching vs the framework answer

### Evidence

**The zero-API baseline exists and works.** A consumer can today hold
`Mutex<HashMap<(Url, i32), Arc<T>>>` keyed by `Document::version()`. lsp-poc already built the
disk-file half of exactly this: `workspace::Index` (`lsp-poc/crates/lsp-poc/src/workspace/mod.rs:44-48`)
caches `HashMap<Url, Entry>` with an `(mtime, size)` stamp gate
(`workspace/mod.rs:139-148`) — a stamp-keyed consumer cache, proving the pattern is buildable
consumer-side. What it does **not** have is any caching of open documents: every request re-parses
(`server.rs:38-41` `parse()`, called from `compute_diagnostics` `server.rs:86-97`,
`document_snapshots` `server.rs:63-84`, and the resolver's `open` closure `server.rs:51-55`).
[measured, code-verified] Consumers demonstrably build the easy half and skip the open-document half.

**Version is not a sufficient invalidation key — two framework-specific traps.** A consumer-keyed
`(Url, version)` map goes stale in exactly the cases the framework knows about and the consumer
cannot see:

1. **The didSave hole.** `handle_document_save`
   (`src/server/state/documents.rs:287-349`) installs fresh text — from `params.text` or a disk
   re-read — while **keeping the old version number**
   (`documents.rs:340-341`: `from_shared_meta(..., old.version, text, ...)`). Text changed, version
   did not. The framework's own semantic-tokens state is evicted on this path precisely for that
   reason (`documents.rs:346`). A version-keyed consumer cache serves stale data after a save whose
   content differs from the in-memory snapshot.
2. **Version resets to 0 on identity-replacing events.** `didClose` with workspace diagnostics
   enabled keeps a disk snapshot at version 0 (`documents.rs:119-120`); watched-file refresh of a
   Workspace document re-inserts at version 0 (`documents.rs:383`); the dispatch's
   untracked-URL fallback builds per-request version-0 snapshots
   (`src/server/with_state/mod.rs:51-70`). `(Url, 0)` recurs across *different contents*, so a
   consumer map keyed `(Url, i32)` collides stale entries across origin changes.

**The framework already owns this discipline once.** `ServerState.semantic_tokens_cache`
(`state/mod.rs:29`, a per-URL `DashMap`) is evicted at **seven** call sites: fresh installs
(`documents.rs:78`), close-remove (`114`, `123`), didSave (`346`), watched-file delete (`371`),
rename/delete-file removal (`428`), and `retain_documents` (`444-446`). The invalidation knowledge
already lives in the framework; a second consumer re-derives it by hand or gets traps 1-2 wrong.
[Note] `didChange` deliberately does *not* evict the semantic-tokens state (`documents.rs:129-239`
has no eviction) because that state is protocol delta input, not memoized output — the new cache is
a different animal: its hits must be exact.

**The stale-install race resolves differently by shape.** The dispatch's staleness probe
(`macros/src/dispatch.rs:177-203`: clone-free version snapshot before the handler,
`CONTENT_MODIFIED` if it moved by response time) rejects the *response*; it says nothing about
artifact caching inside the handler. The race to prevent is pair-mixing: read cached data tagged
version v, then consume it against a snapshot of version v+1 (a didChange landing between the two
reads). A framework API can make that state unrepresentable by binding the entry to the snapshot it
was computed from (see R2c) instead of asking every consumer to check `doc.version() == entry.version`
in the right order.

### Conclusion

The framework cache is justified — not by lines of code (the consumer map is ~30 lines; the
framework version is more), but by **invalidation ownership**: the didSave hole and the
version-reset collisions are invisible at the consumer's altitude and the framework has already
paid for learning them (seven eviction hooks). The performance win exists in *both* shapes — the
baseline also fixes lsp-poc's per-target × per-doc multiplication; what only the framework shape
gives is that no consumer can get the invalidation subtly wrong. The owner's lean survives the
evidence.

### Recommendation

Build the framework cache. R2c is the shape that makes the traps disappear structurally rather
than by hook discipline; R2b is the fallback if the per-generation mechanics prove objectionable.
Documentation-of-the-pattern alone is insufficient given two consumers are already expected
(lsp-poc diagnostics/resolver now, downstream servers later).

---

## R2 — API shape candidates

Evaluated against: typing criterion (removes a representable invalid state, not ceremony),
thread-safety (Send+Sync, multi-threaded runtimes), testability, lsp-poc call-site fit, MSRV 1.90,
no new dependencies.

### (a) Consumer-side map keyed `(Url, version)` — the zero-API baseline

- Invalidation: none provided. Consumer must additionally evict on didSave (trap 1) and handle
  version-0 recurrences (trap 2) — both invisible without reading framework internals.
- Stale-install race: preventable (probe `doc.version()` at use time against the same snapshot
  used for consumption) but by convention, not construction.
- Memory: unbounded per version per document unless the consumer also implements slot-replace or
  per-URL eviction.
- lsp-poc fit: mechanical, but every call site repeats the probe; the didSave trap is the one that
  will eventually bite.

**Verdict:** insufficient as the answer; correct as the reference implementation the framework
shape must beat. A doc-only outcome lands here — rejected in R1.

### (b) ServerState type-map slot — state-level store keyed `(Url, key)`, value `(version, Arc<dyn Any + Send + Sync>)`

Mechanics (Rust, verified against the codebase's own patterns):

- Storage: one `Arc<DashMap<(Url, Key), (i32, Arc<dyn Any + Send + Sync>)>>` on `ServerState`
  next to `semantic_tokens_cache` (`state/mod.rs:29` is the direct precedent).
- Key choice matters. `&'static str` keys need a manual key↔type discipline: a downcast failure on
  hit is a latent bug; mitigations are debug_assert, treat-as-miss-and-overwrite, or a composite
  `(TypeId, Option<&'static str>)` key. `TypeId`-only keying (one type = one slot per document,
  newtype wrappers for same-type artifacts) removes the collision failure mode entirely — the
  downcast then cannot fail by construction, which clippy + error-handling.md like (no `expect`
  needed if a mismatched slot is treated as miss-and-replace).
- Lookup contract: hit iff `entry.version == document.version()` where `document` is the snapshot
  the caller passes in and will consume against — this pins the (data, snapshot) pair.
- Compute outside the lock (never hold the DashMap shard across `compute`): two racers may both
  compute; last install wins; both values are equally valid for pure computes. A regressed install
  (older version overwriting newer) is harmless — the version probe just misses — but a
  compare-and-install (`new >= stored`) keeps the slot from regressing.
- Invalidation: the framework must hook the **same seven eviction sites** as the semantic-tokens
  cache, plus slot-replace semantics (one slot per `(Url, key)`, insert overwrites) to bound memory
  at `documents × keys`. Without the hooks, traps 1-2 return.
- **The untracked-document hazard is (b)'s structural weakness.** The dispatch fallback
  (`with_state/mod.rs:51-70`) hands out per-request version-0 snapshots of untracked disk files.
  Keyed `(Url, 0)` in a state-level map, request 1 caches the file's index; the file changes on
  disk; request 2 probes `(Url, 0)` → stale hit. Closing this needs either a membership probe
  (`document_version(url)` vs the snapshot — an extra DashMap lookup per call), an mtime/size stamp
  per entry (recreating lsp-poc's `FileStamp` machinery — `workspace/mod.rs:20`), or
  recompute-always for untracked docs (a tracking-state check leaking into the API contract).
- Memory bounds: `documents × keys` live entries including untracked-URL residue until evicted;
  bounded but requires the full hook surface to stay bounded in practice.
- Testability: unit-testable against `state_with_documents` (`crate::testing`); the hook surface
  needs one test per eviction site (seven W0 tests asserting slot drops).

**Verdict:** workable, precedented (it generalizes `semantic_tokens_cache`), but it re-creates the
entire eviction-hook surface and carries the untracked-document hazard, which needs *more*
machinery (stamps or probes) to close. The version check is load-bearing at every call.

### (c) Document-attached slot — per-generation cache on `DocumentInner`

The brief asked to argue against this from snapshot immutability. The argument **does not hold**:
the existing design is copy-on-write with construction-immutable identity
(`document.rs:54-60`), and a lazily-initialized, write-once-per-key slot inside a generation is
effectively immutable data — it preserves the read-only contract exactly the way the framework's
own tree-sitter `Tree` on `DocumentInner` does (populated at install/parse, read thereafter). The
sound variant:

- Storage: a lazily-used map on `DocumentInner` — e.g. `Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>`
  allocated empty per generation (one small alloc per generation, comparable to the existing
  per-generation `Arc`); or a `OnceLock` wrapper if zero-cost-until-first-use is wanted. Debug
  elides payloads (mechanical; `DocumentInner` already derives `Debug`).
- API: a method on `Document` — `pub fn derived<T: Send + Sync + 'static>(&self, compute: impl FnOnce(&Document) -> T) -> Arc<T>`.
  No state, no keys in the signature; the key is `TypeId::of::<T>()`. Downcast mismatch is
  unrepresentable by construction; if a same-type different-compute collision is still feared, a
  miss-and-replace branch makes it a silent recompute — no `expect`, no `# Panics`.
- **Invalidation is structural, not hooked.** Every content-replacing event already installs a
  fresh generation: `insert_document` (didOpen, watched refresh, close-keep — `documents.rs:58-79`,
  `119-120`, `383`), didChange install (`documents.rs:219-225`), **didSave install**
  (`documents.rs:340-341` — the trap that kills (a) and needs a hook in (b) is free here), recovery
  re-parse (`documents.rs:276-283`). The cache dies with the generation; refcounts free it when the
  store replaces it and outstanding clones drop. Zero eviction hooks, zero new call sites.
- **Cross-request persistence comes free from the store.** `state.document(url)` clones the store's
  *current* generation; until the next write, every request's snapshot shares one
  `Arc<DocumentInner>` — so a per-generation cache is a cross-request cache. This is the
  load-bearing insight: "per-generation" is not "per-request".
- Stale-install race: unrepresentable. Entry and text are the same object; no version comparison
  can be skipped or mis-ordered because there is no version comparison. The (data, snapshot) pair
  is one allocation. This is the typing criterion satisfied literally.
- Untracked/ephemeral snapshots (`with_state/mod.rs:51-70`): each per-request snapshot carries its
  own empty cache → recompute per request — identical to today's behavior, no stale-hit hazard, no
  membership probes. (b)'s worst case is (c)'s non-event.
- Concurrency: probe-under-lock, compute outside the lock, insert-or-take. Two racers may compute
  the same value (same trade as (b); singleflight rejected as ceremony for a pure function). If
  `std::sync::OnceLock::get_or_init` is used per slot instead, concurrent first callers block on the
  winner — acceptable (bounded by one compute) — and a panicking initializer leaves the slot
  uninitialized for retry (std-documented). One contract to document: `compute` must not re-enter
  the same slot on the same generation (re-entrancy would deadlock a blocking once-cell; the
  probe/compute/insert shape with a plain Mutex cannot deadlock but may double-compute).
- Lifetime caveat, stated honestly: an artifact lives as long as *any* clone of its generation —
  a consumer stashing `Document` handles indefinitely pins their caches. lsp-poc drops handles at
  request end; not a real exposure here, worth a doc sentence.
- Known cost: a `Mutex<HashMap>` field on every `DocumentInner`, including generations that never
  use it (~48 bytes idle). Acceptable; the alternative (`Option<Box<...>>` lazy) is available if the
  brainstorm cares.
- lsp-poc fit (sketch — the framework keeps `Arc<Mutex<MarkdownParser>>`, `server.rs:26,38-41`):

  ```rust
  // before (per target × per doc):          // after:
  state.document(open_url).and_then(|d| {    state.document(open_url).map(|d| {
      self.parse(&d.text_contents())             d.derived(|d| self.parse(&d.text_contents()))
  })                                         })
  ```

  The whole-file `String` from `text_contents()` also disappears on hits — an extra win the
  version-keyed consumer map does not give (it must still read text to compute on miss, but on hit
  skips it too — the extra win is really: no re-allocation on the compute path *and* no map/probe
  plumbing).

**Verdict:** strongest shape. The invalidation traps of R1 become structurally impossible; the
memory-bounds story is refcount-driven and hook-free; the untracked-document hazard is a non-event;
the API is one method on an existing public type.

### (d) Per-document generation counter consumers poll (the middle path)

Precedent: `WorkspaceDiagnosticsState.generation: AtomicU64` (`src/workspace/diagnostics.rs:38`,
`next_generation`/`current_generation` `129-135`), used to discard superseded in-flight
configuration responses (`242`, `312`, `324`).

- Shape: per-URL `AtomicU64` bumped on every generation install; consumers keep their own
  `(Url, gen)` maps and poll.
- Assessment: it hands back *all* the mechanics (b) exists to centralize — thread-safety, slot
  bounds, the didSave question (bump-on-save? consumers must know the counter bumps on save), the
  untracked-file question (no URL to bump). The race window narrows but the pair-mixing hazard
  stays consumer-side. The existing precedent uses a generation counter for *response* staleness of
  async config reads — a different problem from artifact memoization; transplanting it here buys a
  signal while keeping all the plumbing.
- Testability: trivially testable, but the tests would pin consumer behavior the framework doesn't
  provide.

**Verdict:** weakest. It is (a) with a better invalidation hint, not a framework-owned cache.
Kept for the record; recommend against.

### Failure-caching question (applies to b and c, open for the brainstorm)

`compute` in the sketch is infallible (`FnOnce(&Document) -> T`); lsp-poc's parse returns
`Option`. Options: consumer wraps (`T = Arc<Option<MdIndex>>` via a newtype or plain Option —
`Option<T>: Send + Sync` when `T` is), or the API offers a fallible variant. Keeping the framework
API infallible means no `# Errors` section and no error-type involvement (thiserror discipline
untouched); failure-caching then stays the consumer's choice. Recommend infallible-only for v1.

### Test strategy (shared)

Lowest tier that can see each behavior — all W0 (`testing.md` two-tier table; no protocol surface
changes, so no wire tests):

1. hit: two calls, same generation → `compute` runs once (AtomicUsize counter in the closure).
2. generation bump: didChange (or direct `handle_document_change`) → recompute; old clone keeps old
   value (snapshot semantics — the pin that matters most).
3. didSave: new text installed at unchanged version → recompute (the R1 trap-1 regression test).
4. close-keep / watched refresh: version 0 re-insert → recompute (trap 2).
5. concurrency: N tasks first-call one generation → compute runs once-or-twice (assert bounded,
   not exactly once, unless a blocking once-cell is chosen), value consistent.
6. panic in `compute` propagates and leaves the slot uninitialized (if once-cell semantics chosen).

---

## R3 — Ecosystem survey (brief)

| system | mechanism | source | take for this crate |
|---|---|---|---|
| rust-analyzer | Salsa query database: inputs + pure functions, memoized, smart reuse; global revision counter; cancellation by panicking with `Canceled::throw`; `AnalysisHost::apply_change` transactional state with `Analysis` as immutable snapshot | salsa README ("A generic framework for on-demand, incrementalized computation", queries `K -> V`, "results of queries are memoized") github.com/salsa-rs/salsa; rust-analyzer `docs/book/src/contributing/architecture.md` (base-db section; Cancellation section; `ide` section) | The heavyweight precedent — and its two ideas both already have light analogues here: revision counter ≈ per-generation version; immutable snapshot ≈ `Document` clone. We will **not** embed a query engine: dependency tracking across a query graph, panic-based cancellation (which would fight `CatchUnwindLayer` in `serve.rs`), and salsa's build complexity are an IDE-engine budget; the need here is one artifact per document generation. |
| gopls | Long-running process, in-memory caching with pre-calculation; "Cache invalidation" is a named design difficulty (mapping files→packages to know what to update); later memory-pressure work moved to a hybrid on-disk/in-memory scheme | gopls `doc/design/design.md` (Basic design decisions → Caching: *in memory*; Difficulties → Cache invalidation; the 2023 future-note on memory) github.com/golang/tools | Independent confirmation that (i) in-memory memoization of derived artifacts is the standard long-running-LSP design, (ii) invalidation is the hard part — exactly the part this crate can own centrally because its generation model is already write-through. |
| typescript-language-server + tsserver | tsls tracks per-file versions (`LspDocument.version`, `applyEdit(version, change)`, null version is an error) and forwards deltas to tsserver; tsls itself holds **no** derived-artifact cache — memoization lives inside the tsserver language service | typescript-language-server `src/document.ts` (version accessor, `applyEdit`, `onDidChangeTextDocument`) github.com/typescript-language-server | [source] for the version-tracking and delegation design; [inference, based on observed protocol usage] that tsserver memoizes per-file semantic state keyed by file version internally — tsls sends only changes and never re-opens files for edits, which only pays off if the service holds derived state. Pattern match: the framework (tsserver) owns the cache; the feature layer (tsls) does not. |
| LSP frameworks with a built-in per-document cache API | none found in the surveyed set | [inference, based on observed patterns] tower-lsp/async-lsp provide no document store at all (async-lsp's lack of document management is this crate's reason to exist) | No API to copy wholesale; the design space here is ours. |
| this crate | `semantic_tokens_cache`: per-URL framework-internal state, seven eviction hooks | `state/mod.rs:29`, `documents.rs:78,114,123,346,371,428,445` | The in-repo precedent: the framework already maintains per-document server-owned state; a derived-data cache generalizes it — or replaces its hook discipline with generations. |

---

## R4 — Integration surface

### Layer placement (arch-lint)

Scopes in `arch-lint.toml:24-53`. The cache machinery belongs in the **`documents` scope**
(`src/documents/**`): it is a field + methods on `DocumentInner`/`Document`, depends on nothing but
std and `crate::documents` itself — none of the denied edges (`documents → lsp-requests/workspace/oneshot`
denied at `arch-lint.toml:61-65`) are touched. If a `ServerState` convenience wrapper is added
later, it is a thin delegate in the `server` scope; `server → documents` is an existing allowed
direction (`ServerState` already holds `DashMap<Url, DocumentEntry>`, `state/mod.rs:22`). No new
scopes, no deny-rule changes, no `lsp_macros` involvement.

### Server trait / dispatch surface

**None.** No trait methods, no capabilities, no `lsp_dispatch!` rows, no `#[lsp_request]` structs —
the three-place pattern does not apply because no LSP method is added. The cache is consumed inside
existing `Server` implementations. Feature gates: none required (works under
`--no-default-features`; lsp-poc's parser is its own `tree_sitter_md`, not the framework grammar).
Public surface: one additive method on the already-exported `Document` type — the fork-friendly
kind of break (none).

### lsp-poc adoption diff (sketch, 3 sites + parser)

1. `compute_diagnostics` (`lsp-poc/.../server.rs:86-97`): `self.parse(&doc.text_contents())` →
   `doc.derived(|d| self.parse(&d.text_contents()))` (or a small `fn index(&self, &Document) ->
   Arc<MdIndex>` helper wrapping the `Option`).
2. references/rename scans — `document_snapshots` (`server.rs:63-84`) and the duplicated
   `resolve_from` closures (`server.rs:144-151`, `222-229`): the per-doc parse becomes the same
   helper; the `open` closure inside `resolver` (`server.rs:51-55`) and the two copies collapse to
   it. This is the site that removes the per-target × per-doc multiplication
   (`workspace/mod.rs:114-137` calls `open` once per candidate per target).
3. `definition_at` / `references_at` / `rename_prepare_at` / `rename_at` (`server.rs:102-239`): the
   leading `state.document(url)` + parse pair becomes snapshot + helper — two lines each.
4. Unchanged: `workspace::Index` keeps stamp-caching *disk* files — the framework cache covers
   tracked (open/workspace) documents only; untracked disk resolution stays lsp-poc's business
   (until the walk-cache/on-demand-load cycle, which is orthogonal: discovery feeds the store, the
   derived cache reads the store). [note] `Index` never evicts entries for deleted files
   (`workspace/mod.rs:139-167` overwrites on stamp change, removes never) — pre-existing, mention
   only.

Expected effect at the measured site: the 124 ms per-poll re-parse of 30 files becomes first-poll
parse + per-document hits afterwards; the resolver's N-target × M-doc parses become M parses per
generation (first target warms, remaining targets probe).

---

## Consolidated recommendation (input for the brainstorm)

| question | recommendation | confidence |
|---|---|---|
| Framework cache at all? | Yes — invalidation ownership (didSave hole, version-0 recurrences) is the justification; perf is necessary but not sufficient | high |
| Shape | **(c)** per-generation, `TypeId`-keyed, lazily-allocated slot on `DocumentInner`, public method `Document::derived<T>(&self, compute: impl FnOnce(&Document) -> T) -> Arc<T>` | high |
| Fallback | (b) state-level `DashMap<(Url, TypeId), (i32, Arc<dyn Any>)>` with the full seven-hook eviction surface + slot-replace — only if per-generation storage is rejected | medium |
| Key discipline | `TypeId` only (one type = one slot per generation; newtypes for multiple same-type artifacts); downcast mismatch treated as miss-and-replace, never `expect` | high |
| Generation counter (d) | reject — signal without ownership | high |
| Failure caching | API stays infallible; consumers wrap (`Option`/`Result` in `T`); fallible variant deferred | medium |
| Singleflight / dedup of concurrent computes | skip (pure computes; document the possibility of double-compute) | medium |
| compute async? | no — sync `FnOnce`, matching every lsp-poc call site; async compute is a different cycle | high |
| Docs contract | document: generation lifetime (= cache lifetime), re-entrancy, stale-clone pinning, untracked-snapshot recompute-per-request | high |
| Tests | six W0 tests (R2 list), incl. the didSave recompute regression test | high |
| MSRV note | `rust-version = 1.90` in `Cargo.toml`; `tech.md`'s 1.88 is stale | fact |

### Sources

- salsa README: https://github.com/salsa-rs/salsa (master, README.md)
- rust-analyzer architecture: https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md
- gopls design: https://github.com/golang/tools/blob/master/gopls/doc/design/design.md
- typescript-language-server document.ts: https://github.com/typescript-language-server/typescript-language-server/blob/master/src/document.ts
- All file:line references verified against the working tree on 2026-09-16.
