# Walk Cache & Derived Data — Design

Date: 2026-09-16. Branch: `feature/walk-cache`. Research basis:
`docs/superpowers/research/2026-09-16-walk-cache-design-q1.md`,
`docs/superpowers/research/2026-09-16-walk-cache-design-q234.md`,
`docs/superpowers/research/2026-09-16-watcher-registration-design.md`,
`docs/superpowers/research/2026-09-16-derived-data-cache-design.md`.
Status: approved design, pending implementation plan. ONE spec by owner mandate.

## Problem

Two independent costs on every `workspace/diagnostic` poll:

1. **Discovery.** `refresh_workspace_documents`
   (`src/server/state/workspace.rs:76-125`) runs a synchronous ignore-crate walk
   on the executor thread (`walker.files()`, `src/workspace/walker.rs:56-88`
   parks the caller until all walker threads join), re-compiling the gitignore
   chain into GlobSets each time (measured: ~1.06 ms × 2 global + repo chain,
   walk ≈ 3.8 ms on the lsp-poc tree). On a `current_thread` runtime — which
   downstream servers legitimately choose — the whole server freezes for every
   concurrent request during the walk. The same tree is re-walked 185 times in
   the captured research window.
2. **Recompute.** Downstream servers re-derive per-document indexes from
   scratch on every request (lsp-poc: ~124 ms per poll re-parsing 30 files;
   per-target × per-open-doc re-parses in its resolver). The framework offers
   no memoization surface, and the naive consumer alternative is unsafe: a
   `(Url, version)` map serves stale data after `didSave` (which installs fresh
   text while keeping the old version, `documents.rs:340-341`) and `version`
   recycles `0` (close-keep, watched-refresh, dispatch's untracked-disk
   fallback).

## Goals

1. The executor never blocks on the walk: `spawn_blocking` end to end.
2. The walk runs once per invalidation event, not once per poll — when the
   client supports file watching; without that support the framework falls
   back to walk-per-poll (off-executor) and no cache.
3. File-watching events actually arrive: a capability-gated
   `didChangeWatchedFiles` registration (Zed never pushes unregistered).
4. Consumers memoize derived data through a framework-owned, structurally
   invalidated per-document slot (`Document::derived`), safe against the
   `didSave` and version-recycling traps.
5. A file that disappears between polls can never abort the poll: per-file
   load failures become skip + warn + exclude.

## Non-goals

- A public workspace-scan API (deferred until a second downstream consumer
  materializes; the engine is designed for later `pub use` promotion).
- TTL / staleness-window caching (a domain judgment — policy, not plumbing).
- `result_id` / unchanged-report support (consumer-side).
- A Salsa-style query engine (its panic-based cancellation fights
  `CatchUnwindLayer`; cited in the derived-data research).
- lsp-poc adoption diffs (their repository; the spec's appendix sketches them).
- globset/ignore swaps (refuted by the lsp-poc performance research).

## Feature 1 — walk cache

### Components

- **`WalkCache`** (new, `src/server/state/` — state scope): held on
  `ServerState` as `Arc<WalkCache>`, interior
  `Mutex<WalkCacheInner>` where `WalkCacheInner { entries:
  Option<Vec<WalkedFile>>, dirty: bool }` and `WalkedFile =
  (PathBuf, Url, Arc<DocumentMatcher>)`. Methods: `get_valid() ->
  Option<Vec<WalkedFile>>` (None when dirty or empty), `store(Vec<WalkedFile>)`
  (clears dirty), `invalidate()` (sets dirty), `clear()` (dirty = true, entries
  = None — the folders-changed form).
  **Lock discipline (both new mutexes):** a guard is never held across an
  `.await` — every call site locks, clones the decision or the data, drops the
  guard, and only then awaits (the spawn_blocking store included). Poisoning:
  recovered through `PoisonError::into_inner()` — both caches are self-healing
  (a worst-case stale list is stamp-guarded; a half-written derived map is
  recomputed), so there is no invariant worth a panic over.
- **Watcher registration** (new function in `src/workspace/diagnostics.rs`,
  cloned from the `register_configuration` precedent at `diagnostics.rs:260-286`):
  in `initialized`, gated on the client capability
  `workspace.didChangeWatchedFiles.dynamicRegistration` AND
  `workspace_diagnostics.enabled()`; options are the typed
  `DidChangeWatchedFilesRegistrationOptions` serialized with
  `serde_json::to_value` — one `FileSystemWatcher` per valid matcher `url_glob`
  (filtered by the same `Glob::new` validity rule the matcher applies), kind =
  Create | Change | Delete (7, explicit: omitting Change would regress the
  existing eager tracked-doc refresh in `handle_watched_files_change`),
  registration id `"async-language-server.watchedFiles"`, spawn + warn-on-fail.
- **Refresh flow** (`refresh_workspace_documents`): enabled-gate and roots-gate
  unchanged; then `walk_cache.get_valid()` — hit: build `urls`/`loads` from the
  triples with no walking; miss or dirty: `tokio::task::spawn_blocking` over
  `WorkspaceWalker::new` + `.files()`, build triples
  (`matchers.find_path` + `path_to_url` resolved once per file inside the
  blocking hop), `walk_cache.store(..)`, continue. Per-file loads and the
  retain pass are unchanged except §Failure semantics.
- **Invalidators:** folders change → `clear()` (roots changed; the cache key
  includes them implicitly); `didChangeWatchedFiles` → `invalidate()` (hooked
  in `handle_watched_files_change`, `src/server/state/documents.rs:351-395`,
  next to the existing tracked-doc eager refresh); workspace diagnostics
  Disabled→Enabled transition → `invalidate()`; diagnostics disabled → `clear()`
  and no walking (the existing enabled-gate at `workspace.rs:77` already
  short-circuits). `didOpen`/`didClose` are not invalidators (an open file's
  content is always fresh through its Open origin; the list question is covered
  by the events above).

### Client-support conditional

No dynamic-watching support in the client → no registration → no events →
`get_valid()` never hits → every poll walks (off-executor). The framework
detects this once at `initialized` and simply never enables the cache; the
`workspace/diagnostic` contract is identical for the consumer in both worlds.

### Failure semantics

- Walk error: only root canonicalization is fallible (`walker.files()` is
  de-facto infallible); the error propagates as today and the cache is
  untouched by construction (never written on that path).
- Per-file load error: **warn + skip + exclude from the returned urls** instead
  of the current all-or-nothing abort (a stale-cached file deleted on disk must
  degrade one entry, not poison every future poll). The oneshot path keeps its
  batch hard-fail. This changes documented semantics: `structure.md`'s
  diagnostics section gains the skip sentence, and the plan updates
  `structure.md` accordingly.

## Feature 2 — derived-data slot

### API

```rust
/// Returns the derived value for this document's current content,
/// computing it through `compute` on first access and memoizing it for
/// every later access until the document changes.
#[must_use = "the derived value is the point of the call; dropping it only burns the compute"]
pub fn derived<T>(&self, compute: impl FnOnce(&Document) -> T) -> Arc<T>
where
    T: Send + Sync + 'static,
{ ... }
```

Infallible by design: a fallible derive is expressed by the consumer's own `T`
(`Option<Foo>` / `Result<Foo, E>`) — the framework caches whatever the consumer
decides a failure means.

### Mechanics

- `DocumentInner` gains `derived: Mutex<HashMap<TypeId, Arc<dyn Any + Send +
  Sync>>>` (std only). Key: `TypeId::of::<T>()`; hit → `Arc::clone` + downcast;
  miss → `compute(self)` → insert → return. Concurrent first access on the
  same key may compute twice; the last writer wins the slot — memoization is
  idempotent by contract, and correctness never depends on who won.
- **Structural invalidation:** every mutation path already installs a fresh
  `DocumentInner` generation (copy-on-write writes, `didSave`'s
  text-with-old-version install included) — the new map simply does not
  survive into it, so invalidation has zero hooks and stale mixing across
  generations is unrepresentable. Untracked per-request document snapshots
  (dispatch's disk fallback) start from an empty map and just recompute.
- Cross-request persistence falls out of the store: `state.document(url)`
  clones the current generation, so the map lives until the next write to that
  document.
- arch-lint scope: `documents`. No new dependencies; `--no-default-features`
  clean (the slot is feature-independent).

## Error handling

No new error variants. The load-skip path replaces a propagating error with a
traced warning at the affected site (message names the file and the reason);
the walk-propagation path is unchanged. Both features keep the boundary
discipline: domain code stays `ServerError`-neutral, wire conversion stays in
the existing engines.

## Documentation updates

- `structure.md`: diagnostics section gains the skip sentence ("a per-file
  load failure is traced and skipped — the poll covers the remaining files;
  the oneshot batch keeps its hard fail").
- `tech.md`: MSRV line 1.88 → 1.90 (stale; `Cargo.toml` is authoritative).
- `README.md` (crate docs): `Document::derived` is a public API addition —
  additive, no breaking surface.

## Testing

- **Cache contract, black-box over a real temp workspace** (the no-seam
  decision): create a file on disk → refresh without invalidation → absent
  from urls; watched-create event / folders change → refresh → present;
  delete → refresh → skipped with a warning, the poll succeeds; folders
  change with a removed root → its documents dropped (existing behavior kept).
- **Watcher registration, wire tier:** a fake client advertising
  `dynamicRegistration: true` receives the `RegisterCapability` in
  `initialized` with the matcher-derived globs; without the capability — it
  does not; diagnostics enable/disable transitions re-register / clear.
- **Both-direction mutation pins from day one** (testing.md rules): the dirty
  flag and the skip branch are boolean literals — every test asserts the
  positive and the negative direction (invalidated-refresh-walks,
  not-invalidated-refresh-serves).
- **Derived slot:** hit/miss, two distinct `T` types on one document
  (TypeId independence), generation-swap invalidation, the `didSave` trap
  (fresh text + old version must recompute), untracked-snapshot recompute,
  concurrent first access on distinct keys.
- **Auto-trait pin (api-auto-trait-contract):** a compile-only
  `const _: () = assert_send_sync::<Document>()` lands with the slot —
  `derived` makes `Document`'s Send+Sync status part of the public contract
  (consumers hold documents across awaits and derive from them on any
  thread), and a private-field change that silently drops an auto trait must
  fail at the assertion, not at every downstream spawn.
- Done-bar per task: `make battery` exit 0, zero warnings; `make dupes` 0/0;
  `make mutants FILE=` scoped proofs on the two touched production files
  (`workspace.rs`, `document.rs`) before the cycle closes.

## Breaking changes

- Per-file load failures no longer abort the `workspace/diagnostic` poll
  (skip + warn + exclude). Breaking-change note in the commit message;
  oneshot is explicitly unchanged.
- No public API is removed or signature-changed; `Document::derived` is
  additive.

## Appendix A — lsp-poc adoption sketch (their repository, not this spec's code)

Three call sites switch from `self.parse(&text)` to
`doc.derived(|doc| build(doc.text_contents()))`-shaped calls:
`compute_diagnostics`, the resolver's open-closure, `document_snapshots`.
Their `workspace::Index` keeps its stamp-cached disk files — the framework
cache covers tracked documents only; the split is clean and orthogonal to the
walk cache.
