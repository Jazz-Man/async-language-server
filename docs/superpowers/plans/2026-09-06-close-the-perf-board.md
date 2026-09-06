# Close the perf board — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining performance board: `Document` becomes a cheap-Clone handle (zero public API change), eager parses move onto the blocking pool, tree-sitter query compilation is cached per matcher, the semantic-tokens cache gains lifecycle eviction, the workspace walk parallelizes, and two review follow-ups land.

**Architecture:** The flagship converts `Document` to `Document { inner: Arc<DocumentInner> }` with a construction-immutable `Arc<DocumentMeta>` (uri, language) so a didChange copy-on-write costs refcount bumps, not string allocations — always under the DashMap guard, never interior mutability (the snapshot contract and `CONTENT_MODIFIED` protocol depend on it). Four satellites ride along in the load composite, matcher-level compile cache, eviction hooks, and parallel walk.

**Tech Stack:** Rust edition 2024, tree-sitter 0.26 (`Query` retains `Language`; pointer `Eq`/`Hash`), `ignore` `build_parallel`, tokio `spawn_blocking`.

**Spec:** `docs/superpowers/specs/2026-09-06-close-the-perf-board-design.md` · **Research (required reading, file:line inventory):** `docs/superpowers/research/2026-09-06-document-handle-and-cache-research.md`

## Global Constraints

- **No git commands, no commits** — every task ends with a *checkpoint* (file group); the owner commits. **Pause after every clean review** and hand the checkpoint over; resume only on the owner's go.
- English artifacts; no `use … as Name` (`as _` fine); imports at use sites; never `#[allow]` — lint-clean semantics-identical forms (the plan's snippets may trip pedantic lints; fix the form, flag the deviation).
- **The COW-under-guard invariant**: no interior mutability inside `Document`; the store installs a fresh `Inner` under the DashMap guard. Outstanding handles must keep seeing their snapshot.
- Existing snapshot-semantics tests, wire tests, and doctests pass **unmodified** unless a task says which and why.
- The full battery in all three feature configurations after every task; tree-sitter-gated code compiles under `--no-default-features`.
- Piped output: check `${pipestatus[1]}` (zsh). Keep context lean (`| tail -N`).

**Baseline:** 256 / 222 / 256 (0 failed). New tests land with their tasks; report exact counts.

## File Structure

| file | responsibility | task |
|---|---|---|
| `src/documents/document.rs` | handle + Inner/Meta split, accessors, query-cache consult | 1, 3 |
| `src/server/state/documents.rs` | construction + COW mutation sites, eviction hooks | 1, 4 |
| `src/server/state/mod.rs` | `DocumentEntry` unchanged shape (Document value) | — |
| `src/server/state/workspace.rs` | refresh composite (parse into spawn_blocking) | 2 |
| `src/oneshot/{server,workspace_diagnostics}.rs` | oneshot composite | 2 |
| `src/documents/matcher.rs` | per-matcher compile cache | 3 |
| `src/workspace/walker.rs` | parallel walk | 5 |
| `src/server/state/tests.rs` (or inline) | COW snapshot pin | 1 |
| `benches/oneshot_diagnostics.rs` | one doc sentence | 6 |
| `src/workspace/diagnostics.rs` (tests) | `gated_setup` cleanup | 6 |
| `README.md`/rules docs | docs sync | 7 |

---

### Task 1: Document handle with Inner split (flagship)

**Files:**
- Modify: `src/documents/document.rs` (struct split + accessors)
- Modify: `src/server/state/documents.rs` (construction + COW mutation sites, per research §1's inventory)
- Test: COW snapshot pin (inline where state tests live) + existing snapshot tests unmodified.

**Interfaces:**
- Produces (consumed everywhere): `Document { inner: Arc<DocumentInner> }` implementing `Clone` (one Arc bump); `DocumentInner { meta: Arc<DocumentMeta>, matcher: Option<Arc<DocumentMatcher>>, version: i32, text: Rope, #[cfg(feature = "tree-sitter")] tree_sitter_lang: Option<Language>, tree_sitter_tree: Option<Tree> }`; `DocumentMeta { uri: Url, language: String }`; crate-private constructor `Document::from_parts(uri, language, matcher, version, text, trees...)`; public surface (`uri()`, `language()`, `version()`, `text()`, `text_reader()`, `text_contents()`, `text_bytes()`, `query()`) unchanged.

- [ ] **Step 1: Write the failing COW pin**

```rust
    #[test]
    fn document_clones_keep_their_snapshot_across_changes() {
        let state = crate::testing::state_with_documents();
        let url = crate::testing::url("file:///testing/demo.txt");
        let before = state.document(&url).expect("document is tracked");
        let text_before = before.text_contents();

        crate::testing::open_document(&state, &url, 2, "completely new contents\n");

        let after = state.document(&url).expect("document still tracked");
        assert_eq!(before.text_contents(), text_before);
        assert_eq!(after.version(), 2);
        assert_ne!(after.text_contents(), text_before);
    }
```

(Adapt fixture calls to the local test module's style; the assertion set is the contract: the outstanding `before` handle never observes the write.)

- [ ] **Step 2: Run — expect failure** at the semantic level only if snapshots are already broken; compilation passes against the current struct (the test is contract-first; it may already pass — then it stays as the regression pin and the task proceeds).

- [ ] **Step 3: Implement the split**

```rust
use std::sync::Arc;

/// A snapshot of one open document (see the type-level docs above).
///
/// A cheap handle: cloning bumps a refcount. Writes never mutate a shared
/// inner — the store installs a fresh generation under its guard, so every
/// outstanding clone keeps the content it was created with.
#[derive(Debug, Clone)]
pub struct Document {
    inner: Arc<DocumentInner>,
}

#[derive(Debug)]
struct DocumentInner {
    meta: Arc<DocumentMeta>,
    matcher: Option<Arc<DocumentMatcher>>,
    version: i32,
    text: Rope,
    #[cfg(feature = "tree-sitter")]
    tree_sitter_lang: Option<tree_sitter::Language>,
    #[cfg(feature = "tree-sitter")]
    tree_sitter_tree: Option<tree_sitter::Tree>,
}

/// Construction-immutable identity: shared untouched across write
/// generations so a copy-on-write costs refcounts, not string clones.
#[derive(Debug)]
struct DocumentMeta {
    uri: Url,
    language: String,
}
```

Convert every existing pub accessor to forward through `self.inner.…`; add the crate-private constructor taking the flat parts (building `Arc::new(DocumentMeta { uri, language })` internally). All direct field accesses inside the crate (per research §1's inventory: construction in `state/documents.rs::insert_document`, the disk-read fallback in `with_state/mod.rs`, COW writes in `handle_document_change`/`replace_full_text`/`handle_document_save`/watched-file reload) switch to the constructor or a rebuild:

```rust
    // COW write pattern (didChange, under the DashMap guard):
    let old = entry.document.inner_arc(); // pub(crate) fn inner_arc(&self) -> &Arc<DocumentInner>
    entry.document = Document::from_parts(
        old.meta.clone(),            // shared, no string clones
        old.matcher.clone(),
        new_version,
        new_rope,
        old.tree_sitter_lang.clone(), // after the incremental edit applied to a rebuilt tree
        Some(new_tree),
    );
```

didSave's in-place matcher/grammar rewrite (research §3, `documents.rs:286`/`:306`) becomes a rebuild sharing `meta` and refreshing only the mutable half. No `Arc::make_mut` across shared handles — the store always installs a fresh generation.

- [ ] **Step 4: Verify** — `cargo test --workspace && cargo test --workspace --no-default-features && cargo test --workspace --all-features` (existing snapshot/wire tests unmodified; ≈257/223/257 with the new pin); `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`.

- [ ] **Step 5: Checkpoint** — `src/documents/document.rs`, `src/server/state/documents.rs`, `src/server/with_state/mod.rs` (if the disk-read constructor site moved), test module. **Pause for the owner's commit.**

---

### Task 2: Load composite — parses onto the blocking pool

**Files:**
- Modify: `src/server/state/workspace.rs` (refresh load task)
- Modify: `src/oneshot/workspace_diagnostics.rs` (per-item open)

**Interfaces:**
- Consumes: Task 1's constructor (fresh-generation installs from any thread — the store is `Send + Sync`).
- Produces: none (internal shape).

- [ ] **Step 1: Refresh composite** — move `insert_document` (the eager parse) inside the existing `spawn_blocking` closure so the closure reads, parses, and installs in one blocking hop (state insert is DashMap-safe from the pool). The mtime gate order is unchanged: probe → read → parse+install → stamp.
- [ ] **Step 2: Oneshot composite** — wrap the per-item `read + bootstrap-clone.open_document` in one `spawn_blocking` (the per-task `OneshotServer` clone is `Send`; `did_open` is synchronous); `document_diagnostics` stays on the executor. The `Handle::try_current` guard and inline fallback stay.
- [ ] **Step 3: Verify** — batteries ×3 unchanged counts; the structural width tests (engine + oneshot) still pass unmodified (they gate handler concurrency, not parse placement).
- [ ] **Step 4: Checkpoint** — the two files. **Pause.**

---

### Task 3: Query-compile cache (+ text-materialization attempt)

**Files:**
- Modify: `src/documents/matcher.rs` (per-matcher cache)
- Modify: `src/documents/document.rs` (`query()` consults the cache)
- Feature-gated; tests inline.

**Interfaces:**
- Produces: `DocumentMatcher::compiled_query(&self, source: &str) -> Result<Arc<tree_sitter::Query>, QueryError>`-shaped consult used by `Document::query` (exact visibility `pub(crate)`; matchers are crate-internal handles).

- [ ] **Step 1: Failing test** — same source string twice yields the same compiled query (`Arc::ptr_eq`); a compile error still surfaces `QueryError::InvalidQuery`; (feature-gated, mirrors the matcher tests' fixtures).
- [ ] **Step 2: Implement** — interior-mutable map on the matcher (`DashMap<String, Arc<Query>>` or `RwLock<HashMap<…>>`, pick one and stay consistent); value owns the compiled query for the matcher's grammar (grammar-identity soundness per research §4). `Document::query` clones the `Arc<Query>` instead of compiling.
- [ ] **Step 3: TextProvider attempt** — try bridging the rope via tree-sitter's text provider so `query` stops materializing the whole-file `String`; if the API does not cooperate, keep the materialization and add the one-line limitation comment (the spec sanctions the fallback; compilation is the dominant cost).
- [ ] **Step 4: Verify** — ×3 configs; `--no-default-features` compiles (gated); clippy+fmt.
- [ ] **Step 5: Checkpoint** — two files. **Pause.**

---

### Task 4: Semantic-tokens eviction

**Files:**
- Modify: `src/server/state/documents.rs` (removal sites per research §5's eight-site list)

- [ ] **Step 1: Failing test** — full-delta flow caches an entry (existing tests show the shape); `didClose` removes it; watched-file rename/delete removes it.
- [ ] **Step 2: Implement** — `self.semantic_tokens_cache.remove(&url)` at each removal site the research enumerated (didClose paths, rename/delete, refresh-retain removal).
- [ ] **Step 3: Verify** — ×3; clippy+fmt.
- [ ] **Step 4: Checkpoint** — one file. **Pause.**

---

### Task 5: Parallel walk

**Files:**
- Modify: `src/workspace/walker.rs`

- [ ] **Step 1: Failing test** — determinism pin: the same tree walks to the identical sorted `Vec` as today (golden copy from the current implementation, temp workspace fixture).
- [ ] **Step 2: Implement** — `WalkBuilder::build_parallel` with a collecting visitor (per research §6: callback/pool API, no ordering); keep the post-collection sort — it restores the contract.
- [ ] **Step 3: Verify** — ×3; clippy+fmt; the oneshot and refresh tests (deterministic outputs) stay green unmodified.
- [ ] **Step 4: Checkpoint** — one file. **Pause.**

---

### Task 6: Review follow-ups (two one-liners)

**Files:**
- Modify: `src/workspace/diagnostics.rs` (tests: `gated_setup` cleans its temp dir — return the root and `fs::remove_dir_all` on the test's exit path)
- Modify: `benches/oneshot_diagnostics.rs` (module-doc sentence: the group pins a 10 s measurement time; `--measurement-time` on the CLI will not change it)

- [ ] **Step 1:** apply both; `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` stay green (bench has no test-cfg allowances).
- [ ] **Step 2: Checkpoint** — two files. **Pause.**

---

### Task 7: Docs sync

**Files:**
- Modify: `.claude/rules/structure.md` (Document handle + meta split; blocking-pool loads incl. parses; matcher compile cache; parallel walk), `README.md` (one Tour sentence for the handle), `.claude/rules/tech.md` only if anything battery-relevant changed (it should not).

- [ ] **Step 1:** apply, keeping each file's voice; wrap-aware token grep for stale claims (`Document is a snapshot clone`, `single-threaded walk`) — 0 hits after edits.
- [ ] **Step 2:** `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` exit 0.
- [ ] **Step 3: Checkpoint** — docs. **Pause.**

---

### Task 8: Final battery

- [ ] **Step 1:** the seven-command battery ×3 configurations; report exact counts (≈261/227/261 with the new tests).
- [ ] **Step 2:** greps — no `text_contents()` calls added in `document.rs::query` if the TextProvider attempt landed; `parse_rope` still the only parse path; no `Arc::make_mut` in state code.
- [ ] **Step 3:** report all checkpoints; the owner commits. Final whole-branch review follows per SDD.

---

## Self-review notes

- Spec coverage: components 1→T1(+T2's install pattern), 2→T2, 3→T3, 4→T4, 5→T5, 6→T6; matrix→spec section (T3/T4 implement its two actionable rows); docs→T7; decision 5 (parser reuse) needs no task by design.
- Type consistency: `from_parts`/`inner_arc` named identically in T1's pattern and Interfaces; `compiled_query` consult named in T3's Interfaces and Step 2.
- Ordering: T1 first (shared files); T2 depends on T1; T3–T6 after T1 (T3 touches document.rs); T7–T8 last. No dead_code traps (every new item has an immediate consumer: accessors are used crate-wide, the cache by `query`, eviction by existing events).
