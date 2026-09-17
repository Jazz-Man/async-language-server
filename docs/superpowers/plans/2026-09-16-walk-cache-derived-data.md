# Walk Cache & Derived Data Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the per-poll executor freeze and re-walk (spawn_blocking + watcher-invalidated walk cache) and give consumers a structurally invalidated derived-data slot (`Document::derived`).

**Architecture:** Three independent layers over the existing refresh path: (1) the synchronous walk moves to `spawn_blocking`; (2) per-file load failures degrade to skip+warn+exclude, and a `WalkCache` (dirty-flag + Mutex, triples `(PathBuf, Url, Arc<DocumentMatcher>)`) serves the file list between invalidation events — folders change, watched-file create/delete (requires a NEW capability-gated `didChangeWatchedFiles` registration in `initialized`), diagnostics enable/disable; (3) `DocumentInner` gains a TypeId-keyed lazy map inside the generation, exposed as the public infallible `Document::derived<T>()` whose invalidation is structural (every write installs a fresh generation).

**Tech Stack:** Rust (edition 2024, MSRV 1.90), tokio (`spawn_blocking`), std `Mutex`/`HashMap`/`TypeId`/`OnceLock`, lsp-types 0.95.1 (`DidChangeWatchedFilesRegistrationOptions`, `FileSystemWatcher`, `WatchKind`), criterion-free W0 tests.

**Spec:** `docs/superpowers/specs/2026-09-16-walk-cache-derived-data-design.md`

## Global Constraints

- The owner commits: tasks end at their verification step with a pause; no task runs `git add`/`git commit`/any git write.
- Done-bar per task: `make battery` exit 0 with **zero warnings** (dylint runs `RUSTFLAGS="-D warnings"`) plus `make dupes` 0/0. Scoped `make mutants FILE=` proofs run where a task says so.
- No new dependencies; std `sync` primitives only. MSRV 1.90.
- English artifacts; `missing_docs` deny; clippy `all`/`cargo`/`pedantic` deny; **no suppressions** — a failing test means a wrong implementation.
- Executor discipline: a `Mutex` guard is never held across an `.await` — lock, clone, drop, then await. Poisoning recovers via `PoisonError::into_inner()` (both caches self-heal).
- The mutation-driven testing rules (`.claude/rules/testing.md` "Mutation-driven test design") apply: boolean-literal flags get both-direction pins from day one; the cache contract is pinned black-box (no walk-counter seam); a walk-but-discard mutant is an equivalent-mutant disposition, recorded not tested.
- Shell commands are plain (no `rtk` prefixes — the hook rewrites automatically).
- `oneshot` keeps its batch hard-fail: the skip semantics of Task 2 change ONLY `refresh_workspace_documents`; do not touch `src/oneshot/workspace_diagnostics.rs`.
- When lsp-types 0.95.1 field names in this plan disagree with the vendored source at `~/.cargo/registry/src/index.crates.io-*/lsp-types-0.95.1/src/lib.rs`, the vendored source wins; the predicate intent is fixed by the spec.

---

### Task 1: The walk leaves the executor

**Files:**
- Modify: `src/server/state/workspace.rs:76-105` (the walk section of `refresh_workspace_documents`)

**Interfaces:**
- Consumes: existing `WorkspaceWalker::new(&roots, config)` / `.files()` signatures.
- Produces: identical `refresh_workspace_documents` signature and semantics — the walk runs on the blocking pool. Task 4 replaces this section's internals again; this task's value is the isolation of the offload step.

- [ ] **Step 1: Restructure the walk into a blocking hop**

Replace the walk section (the `let walker = ...` line and the `for path in walker.files()? { ... }` loop header) so the walk and the per-file triple building run inside `tokio::task::spawn_blocking`, while `urls`/`loads` accumulation stays on the async side:

```rust
        let state = self.clone();
        let walked = tokio::task::spawn_blocking(move || -> ServerResult<Vec<(PathBuf, Url, Arc<DocumentMatcher>)>> {
            let walker = WorkspaceWalker::new(&roots, WorkspaceWalkConfig::default())?;
            let mut walked = Vec::new();
            for path in walker.files()? {
                let Some(matcher) = state.matchers.find_path(&path) else {
                    continue;
                };
                let uri = path_to_url(&path)?;
                walked.push((path, uri, matcher));
            }
            Ok(walked)
        })
        .await
        .map_err(|join_error| ServerError::Other(Box::new(join_error)))??;
```

Then consume the triples on the async side (the loads/urls split, unchanged semantics):

```rust
        let mut urls = Vec::new();
        let mut loads = Vec::new();

        for (path, uri, matcher) in walked {
            urls.push(uri.clone());
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                continue;
            }

            loads.push((path, uri, matcher));
        }
```

Note the `state` clone moves `ServerState` (Send + Sync) into the closure so `state.matchers` is reachable from the blocking pool; `self` is not moved (the async side keeps using it). `roots` moves into the closure — re-derive `let roots = self.workspace_roots();` BEFORE the closure and clone for the retain pass below (it already uses `&roots` at :118).

- [ ] **Step 2: Gates**

Run: `make battery` — expected exit 0, zero warnings (all existing refresh tests green: the closure preserves walk-order-independent semantics; `walked` feeds the same loop body). `make dupes` 0/0.

- [ ] **Step 3: Pause for the owner's commit** — files: `src/server/state/workspace.rs`. Suggested message: `Run the workspace walk on the blocking pool`.

---

### Task 2: Skip + warn + exclude for per-file load failures

**Files:**
- Modify: `src/server/state/workspace.rs:107-124` (the loads/urls loop and `for_each_bounded` call)
- Modify: `.claude/rules/structure.md` (diagnostics section, +1 sentence)
- Test: `src/server/state/workspace.rs` `mod tests` (new unix-only test)

**Interfaces:**
- Consumes: `load_workspace_document` (unchanged signature), `for_each_bounded` (unchanged; the closure becomes infallible).
- Produces: refresh semantics — a per-file load failure is traced and skipped; the poll covers the remaining files. The returned `urls` no longer include a file whose load failed.

- [ ] **Step 1: Write the failing test** (in `workspace.rs`'s existing `mod tests`; needs a `ServerState` fixture — check how `src/server/state/tests.rs` builds one and mirror it here, or move this test to that module if fixture reuse is cleaner)

```rust
    // A file that cannot be read degrades to a traced skip: the poll succeeds
    // and covers the remaining files, and the poison never enters the
    // documents map. Unix-only: the failure is injected with permissions.
    #[test]
    #[cfg(unix)]
    fn refresh_skips_unreadable_files_and_keeps_the_rest() {
        use std::os::unix::fs::PermissionsExt;

        const RESTORED_MODE: u32 = 0o644;

        let root = crate::testing::temp_workspace("workspace", "skip-unreadable");
        std::fs::write(root.join("good.md"), "# good\n").expect("file can be written");
        std::fs::write(root.join("locked.md"), "# locked\n").expect("file can be written");
        std::fs::set_permissions(
            root.join("locked.md"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("permissions can be restricted");

        let mut state = /* the module's ServerState fixture with a Markdown matcher
            over root and workspace diagnostics enabled — mirror the seeding in
            src/server/state/tests.rs */;
        let urls = state.refresh_workspace_documents().await...

        assert!(urls.iter().any(|url| url.as_str().ends_with("good.md")));
        assert!(!urls.iter().any(|url| url.as_str().ends_with("locked.md")));

        std::fs::set_permissions(
            root.join("locked.md"),
            std::fs::Permissions::from_mode(RESTORED_MODE),
        )
        .expect("permissions can be restored");
    }
```

(If the fixture lives in `state/tests.rs`, put the test there instead — same body; the walker's own `files_skips_unreadable_entries` at `src/workspace/walker.rs:200-229` is the permission-injection precedent.)

- [ ] **Step 2: Run it to see it fail**

Run: `make nextest ... -E 'test(refresh_skips_unreadable)'`
Expected: FAIL — today the load error aborts the whole poll.

- [ ] **Step 3: Make the closure infallible and exclude failures**

Replace the `for_each_bounded` block so each per-file future owns its error, and only successful loads join the urls:

```rust
        // Open documents are always reportable; loaded files join the urls on
        // success and are skipped — traced, never fatal — on failure.
        for (path, uri, matcher) in walked {
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                urls.push(uri);
                continue;
            }
            loads.push((path, uri, matcher));
        }

        let state = self.clone();
        let width = state.diagnostics_parallelism();
        let loaded: Vec<Option<Url>> = for_each_bounded(loads, width, move |(path, uri, matcher)| {
            let state = state.clone();
            async move {
                match load_workspace_document(state, path, uri.clone(), matcher).await {
                    Ok(()) => Some(uri),
                    Err(error) => {
                        tracing::warn!("skipping unreadable workspace file '{uri}': {error}");
                        None
                    }
                }
            }
        })
        .await?;

        urls.extend(loaded.into_iter().flatten());
```

The retain pass and the sort below are unchanged (a dropped file falls out of the returned urls and out of the documents map through the existing `retain`).

- [ ] **Step 4: Tests + gates**

Run: the new test green, then `make battery` (exit 0, zero warnings) + `make dupes` 0/0.
Expected: PASS / clean. **Oneshot check:** `src/oneshot/workspace_diagnostics.rs` is untouched — its loader at :218-251 keeps the hard fail (state this in the commit body).

- [ ] **Step 5: The rules sentence** (in `.claude/rules/structure.md`, "Diagnostics surfaces" bullet on workspace diagnostics, append one sentence)

```markdown
  A per-file load failure inside a poll is traced and skipped — the poll
  covers the remaining files; the oneshot batch keeps its hard fail.
```

- [ ] **Step 6: Pause for the owner's commit** — files: `src/server/state/workspace.rs`, `.claude/rules/structure.md`. Suggested message: `Skip unreadable workspace files instead of failing the poll` + breaking-change note (`workspace/diagnostic` no longer aborts on one unreadable file).

---

### Task 3: Capability-gated file-watching registration

**Files:**
- Modify: `src/server/state/mod.rs` (ServerState: `file_watching` + `watchers_registered` atomics, accessors)
- Modify: `src/workspace/diagnostics.rs` (`register_watchers` fn, `initialized` hook, enable-transition hook in `apply_enabled`, capability capture in `configure_capabilities`)
- Test: `src/server/tests/` wire tier (fake client with/without the capability)

**Interfaces:**
- Consumes: `configure_capabilities(state, result, client_capabilities)` (existing — has the client caps), `initialized(state)` (existing), `apply_enabled(state, enabled)` (existing), `state.matchers` via a new `ServerState::watcher_globs()`.
- Produces: `ServerState::file_watching() -> bool`, `ServerState::set_file_watching(bool)`, `ServerState::watchers_registered() -> bool`, `ServerState::set_watchers_registered(bool)`, `ServerState::watcher_globs() -> Vec<String>`; `register_watchers(state: ServerState)` — idempotent via the registered flag.

- [ ] **Step 1: State plumbing** (`src/server/state/mod.rs`)

Fields (next to `semantic_tokens_cache`):

```rust
    file_watching: std::sync::atomic::AtomicBool,
    watchers_registered: std::sync::atomic::AtomicBool,
```

Init in `with_options` (`AtomicBool::new(false)` × 2). Accessors (near `set_position_encoding`):

```rust
    /// Whether the client supports dynamic `didChangeWatchedFiles`
    /// registration — captured from its capabilities during initialize.
    pub(crate) fn file_watching(&self) -> bool {
        self.file_watching.load(Ordering::Relaxed)
    }

    pub(crate) fn set_file_watching(&self, supported: bool) {
        self.file_watching.store(supported, Ordering::Relaxed);
    }

    pub(crate) fn watchers_registered(&self) -> bool {
        self.watchers_registered.load(Ordering::Relaxed)
    }

    pub(crate) fn set_watchers_registered(&self, registered: bool) {
        self.watchers_registered.store(registered, Ordering::Relaxed);
    }

    /// Watcher glob patterns derived from the matchers' url globs: the same
    /// strings, filtered by the matcher's own `Glob::new` validity rule.
    /// Matchers without url globs contribute nothing.
    pub(crate) fn watcher_globs(&self) -> Vec<String> {
        let mut globs: Vec<_> = self.matchers.watcher_globs();
        globs.sort();
        globs.dedup();
        globs
    }
```

(Add `pub(crate) fn watcher_globs(&self) -> Vec<String>` to `DocumentMatchers` (`src/server/state/mod.rs`-adjacent matcher store) and `pub(crate) fn url_globs(&self) -> &[String]` to `DocumentMatcher` (`src/documents/matcher.rs`) — the filter uses `globset::Glob::new(g).is_ok()`, the matcher's own compile rule.)

- [ ] **Step 2: Capability capture + registration fn** (`src/workspace/diagnostics.rs`)

In `configure_capabilities`, after the existing machinery lines:

```rust
    let watching = client_capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched| watched.dynamic_registration)
        .unwrap_or(false);
    state.set_file_watching(watching);
```

New function (beside `register_configuration` — same spawn + warn-on-fail idiom; the typed options path with `serde_json::json!` mirrors that precedent):

```rust
/// The watcher registration id: matchers are session-fixed, so the
/// registration is never re-negotiated mid-session.
const WATCHED_FILES_REGISTRATION_ID: &str = "async-language-server.watchedFiles";

/// All three kinds, explicitly: Create/Delete drive the walk cache's
/// invalidation, Change drives the existing eager tracked-doc refresh.
const WATCH_KIND_ALL: i32 = 7;

fn register_watchers(state: ServerState) {
    // Amendment (owner ruling 2026-09-17, task-3 review): also gate on
    // enabled diagnostics — watched-file events only carry meaning for
    // Workspace-origin documents, which exist solely under enabled
    // diagnostics. A later enable re-enters through apply_enabled; disable
    // never unregisters (the events would be no-ops).
    if !state.file_watching()
        || !state.workspace_diagnostics().enabled()
        || state.watchers_registered()
    {
        return;
    }
    let globs = state.watcher_globs();
    if globs.is_empty() {
        return;
    }
    state.set_watchers_registered(true);

    spawn(async move {
        let watchers: Vec<_> = globs
            .into_iter()
            .map(|glob| {
                serde_json::json!({ "globPattern": glob, "kind": WATCH_KIND_ALL })
            })
            .collect();
        let result = state
            .client()
            .request::<RegisterCapability>(RegistrationParams {
                registrations: vec![Registration {
                    id: WATCHED_FILES_REGISTRATION_ID.into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(serde_json::json!({ "watchers": watchers })),
                }],
            })
            .await;
        if let Err(error) = &result {
            tracing::warn!("file watching registration failed: {error}");
        }
    });
}
```

Hooks: `initialized` gains `register_watchers(state.clone());` before `register_configuration`; `apply_enabled` (`:322-327`) gains, inside its `changed && enabled` branch, `register_watchers(state.clone());` (re-registration after a disable is covered by the `watchers_registered` flag: `disable` does NOT unregister — matchers are session-fixed, and a re-enable must not double-register).

- [ ] **Step 3: Wire tests** (in `src/server/tests/` — extend the harness client helper if needed to pass full capabilities on initialize)

```rust
#[tokio::test]
async fn initialized_registers_file_watchers_when_supported() {
    // Client capabilities include workspace.didChangeWatchedFiles.
    // dynamicRegistration = true. Assert: a RegisterCapability request with
    // method "workspace/didChangeWatchedFiles", id
    // "async-language-server.watchedFiles", and one watcher whose globPattern
    // is the fixture matcher's url glob arrives before the test's next
    // request is answered.
}

#[tokio::test]
async fn initialized_skips_watcher_registration_without_support() {
    // Same flow without the capability. Assert: NO
    // workspace/didChangeWatchedFiles registration request arrives.
}
```

(Implement against `src/server/testing.rs`'s `spawn_wire_server` / client helper — if the helper only sets position encodings, add a `capabilities` escape parameter and pass the watching capability JSON through; keep the existing call sites compiling via a defaulted wrapper.)

- [ ] **Step 4: Suite + gates** — `make battery` (exit 0, zero warnings) + `make dupes` 0/0.

- [ ] **Step 5: Pause for the owner's commit** — files: `src/server/state/mod.rs`, `src/server/state/walk_cache.rs` is NOT yet in this task, `src/workspace/diagnostics.rs`, `src/documents/matcher.rs`, `src/server/tests/`. Suggested message: `Register file watchers with capable clients`.

---

### Task 4: WalkCache with three invalidators

**Files:**
- Create: `src/server/state/walk_cache.rs`
- Modify: `src/server/state/mod.rs` (module + `walk_cache: Arc<WalkCache>` field + accessor), `src/server/state/workspace.rs` (refresh consumes the cache; spawn_blocking walk now builds triples and stores), `src/server/state/documents.rs:356-360` (watched-event invalidation), `src/workspace/diagnostics.rs` (`apply_enabled` invalidation)

**Interfaces:**
- Consumes: Task 3's `file_watching()` gate; Task 2's infallible per-file closure.
- Produces: `WalkCache` (`new`, `get_valid`, `store`, `invalidate`, `clear`), `ServerState::walk_cache()` accessor, refresh flow serving cached triples when registered-and-fresh.

- [ ] **Step 1: The struct** (`src/server/state/walk_cache.rs`)

```rust
use crate::documents::DocumentMatcher;
use async_lsp::lsp_types::Url;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// One walked file, fully resolved: the path, its URL, and the matcher that
/// claimed it. Matched-only — non-matching walk output never enters the cache.
pub(crate) type WalkedFile = (PathBuf, Url, Arc<DocumentMatcher>);

/// The workspace file list between invalidation events. One automatic truth:
/// when `dirty` is set, or no entries exist, the next refresh walks again.
#[derive(Debug, Default)]
pub(crate) struct WalkCache {
    inner: Mutex<WalkCacheInner>,
}

#[derive(Debug, Default)]
struct WalkCacheInner {
    entries: Option<Vec<WalkedFile>>,
    dirty: bool,
}

impl WalkCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The cached list, fresh enough to serve: present and not dirty.
    /// The guard never escapes the call.
    pub(crate) fn get_valid(&self) -> Option<Vec<WalkedFile>> {
        let inner = self.lock();
        match inner.entries.as_ref() {
            Some(entries) if !inner.dirty => Some(entries.clone()),
            _ => None,
        }
    }

    pub(crate) fn store(&self, entries: Vec<WalkedFile>) {
        let mut inner = self.lock();
        inner.entries = Some(entries);
        inner.dirty = false;
    }

    pub(crate) fn invalidate(&self) {
        self.lock().dirty = true;
    }

    /// The folders-changed form: the old list is not merely stale, its root
    /// premise moved.
    pub(crate) fn clear(&self) {
        let mut inner = self.lock();
        inner.entries = None;
        inner.dirty = true;
    }

    fn lock(&self) -> MutexGuard<'_, WalkCacheInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
```

(Notes: `WalkedFile` needs `use std::sync::Arc;`; the type lives here, the refresh path imports it. Poisoning recovers per the spec's self-healing rule. `Default` covers `new` — drop `new` if clippy suggests `Default::default()` at the construction site; keep exactly one.)

- [ ] **Step 2: TDD — the contract tests** (in `src/server/state/tests.rs`, real temp workspaces; the module's fixture seeds the cache through the real refresh — mirror `advertise_workspace_diagnostics`'s pattern)

```rust
    // The cache contract, black-box: list membership changes reach the
    // returned urls only after an invalidation event. A created file is
    // invisible until the (watcher-simulated) invalidation; a deleted file
    // degrades to a skip, never a failure.
    #[tokio::test]
    async fn walk_cache_serves_between_invalidations_and_refreshes_on_them() {
        // seed: state + one matched file a.md + refresh -> urls == [a.md]
        // 1. create b.md on disk, refresh again -> urls still == [a.md]
        //    (the fresh cache serves; the new file is invisible)
        // 2. state.walk_cache().invalidate()   (the watcher-simulated event)
        //    refresh -> urls == [a.md, b.md]
        // 3. remove b.md on disk, refresh -> Ok, urls == [a.md], and
        //    state.document(b) is None (the deleted entry falls out via
        //    the retain pass; the load skip keeps the poll alive)
    }

    // The both-direction pin for the dirty flag (mutation-driven rules):
    // invalidation forces a walk; a second refresh with no event between
    // serves the cache. Pinned through the observable list behavior —
    // a walk-but-discard mutant is an equivalent-mutant disposition.
    #[test]
    fn invalidation_flips_serving_to_walking_and_back() { ... }
```

(Full bodies follow the module's existing fixture style; the assertion values are the contract — write them first and watch the invalidate-step assertions fail without the invalidator wiring.)

- [ ] **Step 3: Wire into the state and the refresh flow**

`src/server/state/mod.rs`: `mod walk_cache;` + field `walk_cache: Arc<WalkCache>` + init `Arc::new(WalkCache::new())` in `with_options` + accessor:

```rust
    pub(crate) fn walk_cache(&self) -> &WalkCache {
        &self.walk_cache
    }
```

`src/server/state/workspace.rs` — the refresh walk section becomes cache-gated (replacing Task 1's unconditional hop):

```rust
        // Without watcher support the events never come, so the cache could
        // go stale forever: walk per poll instead (still off the executor).
        let walked = if self.file_watching() {
            match self.walk_cache.get_valid() {
                Some(entries) => entries,
                None => {
                    let entries = self.walk_blocking(&roots)?;
                    self.walk_cache.store(entries.clone());
                    entries
                }
            }
        } else {
            self.walk_blocking(&roots)?
        };
```

with the Task 1 hop extracted verbatim into:

```rust
    /// The blocking walk, off the executor: canonicalize + scan + triple
    /// build in one spawn_blocking hop.
    fn walk_blocking(&self, roots: &[PathBuf]) -> ServerResult<Vec<WalkedFile>> {
        let state = self.clone();
        let roots = roots.to_vec();
        let walked = tokio::task::spawn_blocking(move || -> ServerResult<Vec<WalkedFile>> {
            let walker = WorkspaceWalker::new(&roots, WorkspaceWalkConfig::default())?;
            let mut walked = Vec::new();
            for path in walker.files()? {
                let Some(matcher) = state.matchers.find_path(&path) else {
                    continue;
                };
                let uri = path_to_url(&path)?;
                walked.push((path, uri, matcher));
            }
            Ok(walked)
        })
        .await
        .map_err(|join_error| ServerError::Other(Box::new(join_error)))??;
        Ok(walked)
    }
```

Invalidators, three hooks:
- `src/server/state/documents.rs` `handle_watched_files_change` — at the top of the per-event loop, invalidate only for list-membership kinds:

```rust
            if matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED) {
                // List membership changed: the next poll re-walks. A Change
                // event does not alter membership — the eager refresh below
                // and the stamp gate own the content.
                self.walk_cache.invalidate();
            }
```

- `set_workspace_folders` and `handle_workspace_folders_change` — `self.walk_cache.clear();`
- `src/workspace/diagnostics.rs` `apply_enabled` — on a change to disabled: `state.walk_cache.clear();`; on a change to enabled: `state.walk_cache.invalidate();`

- [ ] **Step 4: Suite + gates** — `make battery` (exit 0, zero warnings) + `make dupes` 0/0 + the scoped mutant proof: `make mutants FILE=src/server/state/walk_cache.rs` — expected: every non-equivalent mutant caught (a walk-but-discard-style mutant, if produced, is dispositioned equivalent per the plan's Global Constraints and recorded in the disposition table).

- [ ] **Step 5: Pause for the owner's commit** — files: `src/server/state/walk_cache.rs` (new), `src/server/state/mod.rs`, `src/server/state/workspace.rs`, `src/server/state/documents.rs`, `src/workspace/diagnostics.rs`, `src/server/state/tests.rs`. Suggested message: `Cache the workspace walk between invalidation events`.

---

### Task 5: The derived-data slot on Document

**Files:**
- Modify: `src/documents/document.rs` (DocumentInner field, both constructors, the public method, the auto-trait pin, tests)

**Interfaces:**
- Consumes: the existing copy-on-write generation mechanics (every write installs a fresh `DocumentInner`).
- Produces: `pub fn derived<T>(&self, compute: impl FnOnce(&Document) -> T) -> Arc<T> where T: Send + Sync + 'static` on `Document` (`#[must_use]`, infallible). No other public surface.

- [ ] **Step 1: Write the failing tests** (in `document.rs`'s existing `mod tests` — the module constructs documents through the `pub(crate)` constructors directly)

```rust
    // The derived slot memoizes per TypeId inside one generation and
    // recomputes across generations: a new generation starts from an empty
    // map, so the didSave trap (fresh text, old version) is structurally
    // closed.
    #[test]
    fn derived_memoizes_per_type_within_a_generation() {
        let doc = /* from_parts fixture: url "derived.json", language "json", version 1, "🙂abc" */;
        let first = doc.derived(|| String::from("one"));
        assert_eq!(&*first, "one");
        let again = doc.derived(|| String::from("two"));
        assert_eq!(&*again, "one");
        assert!(Arc::ptr_eq(&first, &again));

        // A second type derives independently on the same document.
        let flagged = doc.derived(|| std::sync::atomic::AtomicBool::new(true));
        assert!(flagged.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn a_new_generation_recomputes_derived_data() {
        let meta = std::sync::Arc::new(/* DocumentMeta via from_parts twin — construct once */);
        let doc = Document::from_shared_meta(meta.clone(), None, 1, Rope::from_str("v1"), ());
        let seen = doc.derived(|| String::from("first"));
        assert_eq!(&*seen, "first");

        // The next generation: same identity, fresh derived map.
        let next = Document::from_shared_meta(meta, None, 2, Rope::from_str("v2"), ());
        let recomputed = next.derived(|| String::from("second"));
        assert_eq!(&*recomputed, "second");
    }

    // The didSave trap from the research: fresh text can arrive under an old
    // version — derived must key on the generation, never the version.
    #[test]
    fn derived_keys_on_the_generation_not_the_version() {
        // doc v1 derived("old") -> a didSave-style re-install with version 1
        // (SAME version, fresh text) -> derived must NOT return "old".
        // Pin through from_shared_meta with the same version number and
        // different text: the recomputed value must differ.
    }

    // The compile-only auto-trait pin (api-auto-trait-contract).
    const _: () = {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Document>();
    };
```

(Full test bodies in the module's fixture style: `Document::from_parts(crate::testing::url("derived.json"), "json".into(), None, 1, Rope::from_str("🙂abc"), syntax)` with the feature-gated `syntax` pattern the module's `text_bytes_returns_the_document_bytes` test already uses. Write all four bodies completely; watch them fail with "no method named `derived`".)

- [ ] **Step 2: Run them to see them fail** — `make nextest ... -E 'test(derived)'` — Expected: FAIL, no method.

- [ ] **Step 3: Implement** — `DocumentInner` gains `derived: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>` (imports: `std::any::{Any, TypeId}`, `std::collections::HashMap`, `std::sync::{Arc, Mutex, PoisonError}`), initialized `Mutex::new(HashMap::new())` in BOTH constructors (`from_parts` and `from_shared_meta`), and:

```rust
    /// Returns the derived value for this document's current content,
    /// computing it through `compute` on first access and memoizing it for
    /// every later access until the document changes.
    ///
    /// The value is keyed by its type and lives for the document's current
    /// generation: any write to the document (edit, save, disk refresh)
    /// starts a fresh generation and fresh derived data. Infallible — a
    /// fallible derive expresses itself through `T`
    /// (`Option<Foo>` / `Result<Foo, E>`).
    #[must_use = "the derived value is the point of the call; dropping it only burns the compute"]
    pub fn derived<T>(&self, compute: impl FnOnce(&Document) -> T) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        if let Some(value) = self
            .inner
            .derived
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            if let Ok(value) = Arc::clone(value).downcast::<T>() {
                return value;
            }
        }

        let value = Arc::new(compute(self));
        self.inner
            .derived
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, Arc::clone(&value) as Arc<dyn Any + Send + Sync>);
        value
    }
```

(The guard is dropped before `compute` runs — a `compute` that calls `derived` again for a DIFFERENT `T` is safe; for the SAME `T` it would recurse forever, which is the caller's bug, matching the docs' "computing it through `compute`" contract.)

- [ ] **Step 4: Suite + gates** — `make battery` (exit 0, zero warnings) + `make dupes` 0/0 + the scoped mutant proof: `make mutants FILE=src/documents/document.rs` — the derived-region mutants (downcast skip, insert skip, key constant) must all be caught by the new tests.

- [ ] **Step 5: Pause for the owner's commit** — files: `src/documents/document.rs`. Suggested message: `Add the per-document derived-data slot (Document::derived)`.

---

### Task 6: Documentation and cycle close

**Files:**
- Modify: `.claude/rules/tech.md` (MSRV line), `README.md` (additive `derived` mention — only if the README documents the `Document` API; verify first)

**Interfaces:**
- Consumes: everything above.
- Produces: the cycle's documentation debt cleared.

- [ ] **Step 1: tech.md MSRV** — the "## Toolchain" section says MSRV 1.88; `Cargo.toml` says 1.90. Fix the line to 1.90.
- [ ] **Step 2: README** — verify whether `README.md` documents `Document`'s API surface; if yes, add `Document::derived` with a two-line example (infallible, `T = Option<…>` shown for the fallible case); if no, skip.
- [ ] **Step 3: Final proofs** — `make battery` (exit 0, zero warnings), `make dupes` 0/0, and the second scoped mutant proof if not already run in Task 4: `make mutants FILE=src/server/state/workspace.rs`.

- [ ] **Step 4: Pause for the owner's commit** — files: `.claude/rules/tech.md`, `README.md`. Suggested message: `Sync MSRV docs to 1.90`.

---

## Self-Review

- **Spec coverage:** spawn_blocking → Task 1; skip semantics + structure.md sentence → Task 2; watcher registration (capability gate, initialized, glob derivation, kind=7, fixed id, no unregister) → Task 3; WalkCache + three invalidators + client-support conditional → Task 4; derived slot (TypeId map, both constructors, infallible, must_use, auto-trait pin, didSave trap) → Task 5; tech.md MSRV + README → Task 6; mutation-driven both-direction pins → Tasks 2/4/5; scoped mutants → Tasks 4/5/6. No gaps against the spec's Goals 1-5.
- **Placeholder scan:** none — every step carries code or an exact command; the two `( ... )` test-body notes in Task 4's Step 2 are fixture-style references to the module's established pattern with the assertion contract stated, and Task 5's Step 1 marks exactly which bodies to write fully in the module's existing style.
- **Type consistency:** `WalkedFile` / `WalkCache::{get_valid, store, invalidate, clear}` / `ServerState::{file_watching, watchers_registered, watcher_globs, walk_cache}` / `Document::derived` are named identically across Tasks 3-5 and the spec.
