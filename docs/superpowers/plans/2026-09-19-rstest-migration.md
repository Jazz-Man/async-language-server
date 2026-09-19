# rstest Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate all 269 handwritten test functions to rstest (fixtures + cases), collapsing the three duplication axes, per spec `docs/superpowers/specs/2026-09-19-rstest-migration-design.md`.

**Architecture:** Two fixture homes by tier (`src/testing.rs`, `src/server/testing.rs`); `TempWorkspace` Drop-guard with methods; a state-fixture family plus a plain async `seed_workspace` helper (sequencing); wire tier stays plain fns with local handshake helpers; `conversion_tests!` emission untouched (exemption D5).

**Tech Stack:** rstest 0.27.0 (dev-dep, in `Cargo.toml`), tokio, existing harness modules.

**Spec corrections folded in:** `seed_workspace` is a plain async helper, not a fixture (spec §3.1 amended 2026-09-19 — fixtures resolve before the test body, the refresh must run after the test's writes).

## Global Constraints

Binding for every task (from spec §4–§6; verbatim values):

- **Imports**: `use rstest::{fixture, rstest};` only. Never `use rstest::*` (`wildcard_imports` is pedantic-deny). `#[case]`, `#[values]`, `#[future]`, `#[with]`, `#[default]` are never imported.
- **Location**: all rstest code in `src/` `#[cfg(test)]` modules. No `tests/` directory targets.
- **Async shape**: `#[rstest]` above, `#[tokio::test]` below; async fixtures via `#[future]`. `#[once]` is not used.
- **Naming**: descriptive `#[case::name]` on every case row; bare `case_N` forbidden. For `#[values]` whose values do not slug readably, prefer named case rows or a doc-comment name override.
- **Feature gates**: gated row → `#[cfg_attr(feature = "tree-sitter", case(...))]`; gated test → `#[cfg(feature)]` on the fn.
- **Doc comments** referencing paths must point at real files (dylint `nonexistent-path-in-comment`).
- **Deletion is by mutation oracle** — a family member dies only when no mutator class distinguishes it; textual similarity is never the criterion. Rows replace tests only when members differ in data, not orchestration.
- **D5**: `conversion_tests!` rows are untouched. The fns its emitted code calls (`state_with_documents`, `assert_converted_position`, and friends in `src/testing.rs`) are **load-bearing — do not delete or rename them**.
- **Every conversion task's report carries the name map**: `| old nextest id | new nextest id | disposition (attribute-only / case-row merge / deleted-by-oracle) |`.
- **nextest selectors**: scoped runs use `-E 'test(<name>)'` or `-E 'binary(<target>)'`; a plain target-name filter matches nothing.
- `expect`/`unwrap` are allowed in tests (`clippy.toml`); production `src/` stays clean.
- Each task's done-bar: `make battery` green (both feature legs) — no `[allow]`, no threshold tuning, no suppression.
- No git writes. The owner commits after each task's clean review.
- Controller duty (not implementer): each dispatch brief names the neighboring converted files and the fixtures now available.

**Fixture/API reference (Task 1 produces; all later tasks consume verbatim):**

```rust
// src/testing.rs — guard
pub(crate) struct TempWorkspace { root: PathBuf }
impl TempWorkspace {
    pub(crate) fn write(&self, rel: impl AsRef<Path>, text: &str) -> Url; // write + canonicalize + Url
    pub(crate) fn url(&self, rel: impl AsRef<Path>) -> Url;               // canonicalize existing + Url
    pub(crate) fn root(&self) -> &Path;
}
impl std::ops::Deref for TempWorkspace { type Target = Path; }
impl Drop for TempWorkspace;                                             // fs::remove_dir_all, best-effort

#[fixture] fn workspace(#[default("als")] prefix: &str) -> TempWorkspace;
#[fixture] fn state() -> ServerState;                    // TestServer + closed ClientSocket + default options
#[fixture] fn utf16_state() -> (ServerState, Url, Url);  // wraps state_with_documents (which STAYS)
#[fixture] fn gated_state() -> ServerState;              // + advertise_workspace_diagnostics

pub(crate) struct SeededWorkspace { pub(crate) state: ServerState, pub(crate) urls: Vec<Url> }
pub(crate) async fn seed_workspace(ws: &TempWorkspace) -> SeededWorkspace; // folders + advertise + refresh
```

---

### Task 1: Fixture foundation

**Files:**
- Modify: `src/testing.rs` (add guard, fixtures, `SeededWorkspace` + `seed_workspace`, guard's own rstest tests)
- No production behavior changes; no existing test converted; `temp_workspace` fn stays (call sites remain).

**Interfaces:**
- Produces: the full API reference block above, verbatim.
- Consumes: existing helpers (`state_with_documents`, `advertise_workspace_diagnostics`, `workspace_folder`, `TestServer`).

- [ ] **Step 1: Write the guard and fixtures** (complete code):

```rust
use rstest::fixture;

/// A temporary workspace directory removed on drop — including on
/// unwind, which today's manual `fs::remove_dir_all` tails cannot do.
/// The prefix names the owning test module so a directory that outlives
/// a run (best-effort drop: a directory left with restricted
/// permissions is leaked, attributed by prefix) can be traced back.
pub(crate) struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(prefix: &str) -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-{prefix}-{millis}"));
        fs::create_dir_all(&root).expect("temp workspace can be created");
        Self { root }
    }

    /// Writes `text` under the root (creating parent directories) and
    /// returns the file's canonical URL — write + canonicalize +
    /// `Url::from_file_path` in one call.
    pub(crate) fn write(&self, rel: impl AsRef<Path>, text: &str) -> Url {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("nested dirs can be created");
        }
        fs::write(&path, text).expect("file can be written");
        self.url(rel)
    }

    /// Returns the canonical URL of an existing file under the root.
    pub(crate) fn url(&self, rel: impl AsRef<Path>) -> Url {
        let canonical =
            fs::canonicalize(self.root.join(rel)).expect("file can be canonicalized");
        Url::from_file_path(canonical).expect("path can be converted to a URL")
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl std::ops::Deref for TempWorkspace {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        // Best-effort by design: drop must not panic, and a failed
        // cleanup only leaks a prefixed temp directory.
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Default workspace; tests override the attribution prefix:
/// `#[with("walker")] workspace: TempWorkspace`.
#[fixture]
fn workspace(#[default("als")] prefix: &str) -> TempWorkspace {
    TempWorkspace::new(prefix)
}

#[fixture]
fn state() -> ServerState {
    ServerState::with_options::<TestServer>(ClientSocket::new_closed(), &ServerOptions::default())
}

/// The canonical UTF-16 conversion fixture (`"🙂abc"` document). Wraps
/// `state_with_documents`, which stays: the `conversion_tests!` macro's
/// emitted code calls it (spec D5).
#[fixture]
fn utf16_state() -> (ServerState, Url, Url) {
    state_with_documents()
}

#[fixture]
fn gated_state() -> ServerState {
    let state =
        ServerState::with_options::<TestServer>(ClientSocket::new_closed(), &ServerOptions::default());
    advertise_workspace_diagnostics(&state);
    state
}

/// The state-setup triplet (folders + advertise + refresh) collapsed.
/// A plain async helper, not a fixture: fixtures resolve before the
/// test body, and the refresh must run after the test's writes.
pub(crate) struct SeededWorkspace {
    pub(crate) state: ServerState,
    pub(crate) urls: Vec<Url>,
}

pub(crate) async fn seed_workspace(ws: &TempWorkspace) -> SeededWorkspace {
    let state =
        ServerState::with_options::<TestServer>(ClientSocket::new_closed(), &ServerOptions::default());
    state.set_workspace_folders([workspace_folder(ws)]);
    advertise_workspace_diagnostics(&state);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    SeededWorkspace { state, urls }
}
```

- [ ] **Step 2: Write the guard's own tests** (new, in rstest, in `src/testing.rs`'s `mod tests`):

```rust
use rstest::rstest;

#[rstest]
fn write_returns_canonical_urls_and_creates_nested_parents(
    #[with("testing")] workspace: TempWorkspace,
) {
    let url = workspace.write("nested/deep/a.test", "body");
    let path = url.to_file_path().expect("file URL converts to a path");
    assert!(path.is_file());
    assert_eq!(
        fs::read_to_string(&path).expect("file can be read"),
        "body",
    );
}

#[rstest]
fn drop_removes_the_tree(#[with("testing")] workspace: TempWorkspace) {
    let _url = workspace.write("a.test", "a");
    let root = workspace.root().to_path_buf();
    drop(workspace);
    assert!(!root.exists());
}

#[rstest]
fn guards_with_the_same_prefix_get_distinct_roots() {
    let first = TempWorkspace::new("testing");
    let second = TempWorkspace::new("testing");
    assert_ne!(first.root(), second.root());
}
```

- [ ] **Step 3: Verify scoped** — `cargo nextest run -E 'binary(async_language_server)'` → the three new guard tests PASS; `cargo fmt` ; `cargo clippy --workspace --all-targets -- -D warnings` → clean.
- [ ] **Step 4: `make battery`** → green both legs (nothing else changed).
- [ ] **Step 5: Report** — fixture inventory added, battery output summary. No name map needed (no test renamed).

### Task 2: `src/server/state/tests.rs` (45 tests — the hardest file)

**Files:** Modify `src/server/state/tests.rs` only.

**Interfaces:** Consumes Task 1 API verbatim. Produces: the conversion exemplars all later tasks copy.

- [ ] **Step 1: Convert every test** to `#[rstest]` (`+ #[tokio::test]` where async), applying:
  - disk tests take `#[with("state")] workspace: TempWorkspace`; every `fs::write` + `canonicalize` + `Url::from_file_path` chain becomes one `workspace.write(rel, text)`; every trailing `fs::remove_dir_all` is deleted (the guard owns cleanup);
  - pure-state sync tests inject `state` / `gated_state` / `utf16_state` by parameter where the shape matches exactly;
  - write→triplet→assert tests call `let seeded = seed_workspace(&workspace).await;` after their writes; partial setups (e.g. open-before-refresh) keep explicit calls — orchestration differences stay explicit;
  - local helper `seed_semantic_tokens` stays a local plain fn (mid-test sequencing).

  Exemplar (real test, before → after):

```rust
// before
#[tokio::test]
async fn workspace_refresh_preserves_open_documents() {
    let root = temp_workspace("state", "open-document");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(manifest).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    open_document(&mut state, uri.clone(), "open");
    let urls = state.refresh_workspace_documents().await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls, vec![uri.clone()]);
    assert_eq!(state.document(&uri).unwrap().text_contents(), "open");
    assert_eq!(state.document_workspace_version(&uri), Some(1));
    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

// after
#[rstest]
#[tokio::test]
async fn workspace_refresh_preserves_open_documents(
    #[with("state")] workspace: TempWorkspace,
) {
    let uri = workspace.write("a.test", "disk");
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&workspace)]);
    advertise_workspace_diagnostics(&state);
    open_document(&mut state, uri.clone(), "open");
    let urls = state.refresh_workspace_documents().await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls, vec![uri.clone()]);
    assert_eq!(state.document(&uri).unwrap().text_contents(), "open");
    assert_eq!(state.document_workspace_version(&uri), Some(1));
}
```

  (This test opens before refresh — the triplet helper does not fit it; tests whose shape is write→triplet→assert use `seed_workspace`.)
- [ ] **Step 2: Family/oracle pass**: for each group of similar tests (close/disable variants, watcher/watch families), decide row-merge vs separate vs delete strictly by oracle (Global Constraints). Where rows merge, use descriptive `#[case::name]`.
- [ ] **Step 3: Verify**: `cargo nextest run -E 'binary(async_language_server)'` → no failures vs the pre-task count adjusted for merges/deletions; record counts. `cargo fmt`; clippy clean.
- [ ] **Step 4: `make battery`** → green; note compile-time vs pre-task (spec §9 watch).
- [ ] **Step 5: Report** with the full name map table (45 rows + merges/deletions).

### Task 3: `src/server/with_state/tests.rs` (24 tests)

Same pattern as Task 2 (exemplars from its report). This file drives `LanguageServerWithState` directly — state construction identical, disk tests via guard. Verify + battery + name-map report as Task 2 Steps 3–5.

### Task 4: `text_utils` (54 range_ext twins + encoding 3 + position 1 + conversions 1)

**Files:** `src/text_utils/range_ext/{tree_sitter,lsp,bytes}_tests.rs`, `src/text_utils/{encoding,position,conversions}.rs`.

- [ ] Convert all to `#[rstest]`. The three `r()` twins stay local (per testing rule). Feature gate: `tree_sitter_tests.rs` keeps `#[cfg(feature = "tree-sitter")]` on each fn (or the module) — now as `#[rstest]` fns.
- [ ] One-axis families over encodings/ranges become `#[case::name]` rows where oracle-identical; the local `r` builders may take the row values.
- [ ] Verify: `cargo nextest run -E 'test(range_ext)' ...` then **both legs** — this task carries the feature-gate risk: `make battery` (includes `--no-default-features`) must stay green.
- [ ] Name-map report.

### Task 5: workspace tier (walker 4, diagnostics 16, parallel 2, oneshot 9)

**Files:** `src/workspace/{walker,diagnostics,parallel}.rs`, `src/oneshot/workspace_diagnostics.rs`.

- [ ] Walker exemplar:

```rust
#[rstest]
fn files_produce_the_identical_sorted_output_for_the_same_tree(
    #[with("walker")] workspace: TempWorkspace,
) {
    workspace.write("nested/deep/c.test", "c");
    workspace.write("a.test", "a");
    workspace.write("z.test", "z");
    // ... all fs::write/create_dir_all sites become write() (parents auto-created);
    // assertions keep using walker.roots() canonical paths as today.
```

- [ ] `#[cfg(unix)]` permission tests keep their explicit chmod/restore flow (Drop is best-effort by design; the test restores permissions so the guard CAN clean up — keep the restore).
- [ ] oneshot: disk fixtures via guard; the doctest is untouched (D6).
- [ ] Verify scoped, battery both legs, name-map report.

### Task 6: documents + core modules (matcher 9, document 9, tree_sitter_utils 5, error 8, options 4, inventory 12, state/workspace 1, server_trait 1)

**Files:** the eight modules listed.

- [ ] Convert all to `#[rstest]`. `inventory.rs`'s 12 method-table tests are the prime `#[case::name("<method>")]` candidates — merge only where rows differ purely in the method name/string (verify each pins the same oracle shape first).
- [ ] Feature-gated tests (tree_sitter_utils, document tree paths) per Global Constraints.
- [ ] Verify scoped, battery both legs, name-map report.

### Task 7: `lsp_requests` handwritten tests (~40 fns; macro rows untouched)

**Files:** `src/lsp_requests/*.rs` test modules — **only the handwritten fns** (conversion.rs 5, symbol 2, signature_help 2, rename 2, code_action 2, and the ~30 single-fn modules).

- [ ] Convert handwritten fns to `#[rstest]`; several consume `utf16_state` by injection instead of calling `state_with_documents()` directly.
- [ ] **Do not touch any `conversion_tests! { ... }` block** (D5). `state_with_documents` and every fn the macro's emitted code calls stays (Global Constraints).
- [ ] Verify scoped (`-E 'test(conversion)'` etc.), battery both legs, name-map report (handwritten rows only).

### Task 8: wire tier (24 tests, 7 files in `src/server/tests/`)

**Files:** `src/server/tests/{conversion,dispatch,lifecycle,robustness,staleness,termination,workspace_diagnostics}.rs`.

- [ ] Every test gains `#[rstest]` above `#[tokio::test]`; setup stays explicit `spawn_wire_server(<Server>)` calls (D3). Disk-needing tests inject `#[with("wire-<file>")] workspace: TempWorkspace`.
- [ ] `workspace_diagnostics.rs`: extract the shared Watcher/Refresh handshake tail into a local plain async helper wrapping `spawn_wire_server(<that file's servers>)` + initialize — per file, not in the shared harness.
- [ ] Case rows where tests differ only in data (e.g. dispatch matrix rows) — oracle-checked.
- [ ] Verify scoped (`-E 'binary(async_language_server)'` filtered by fn), battery both legs, name-map report.

### Task 9: Final sweeps

**Files:** `src/testing.rs`, `src/server/testing.rs`, `.dupes-ignore.toml`, `docs/superpowers/plans/2026-09-09-mutants-disposition-table.md`, plus whatever the audits tighten.

- [ ] **R4 audit**: table of every harness helper → `#[fixture]` / plain fn (reason) / deleted. Delete `temp_workspace` (grep: zero call sites remain — all tasks migrated off it).
- [ ] **R5 audit**: for each `pub(crate)` item whose only consumers were test helpers this cycle deleted/localized, tighten or delete. Sibling `tests.rs` modules see parent privates — only cross-module needs justify visibility. Evidence per item (LSP findReferences or grep).
- [ ] **dupes**: `cargo dupes cleanup --dry-run` → review the stale list (dissolved groups), drop them, `make dupes` → 0/0.
- [ ] **Disposition reconciliation**: apply every task's collected name map to the mutants disposition table — rename rows, mark oracle-merged rows, drop dead rows.
- [ ] `make battery`, `make dupes`, `make deny` all green.

### Task 10: testing.md rework via the steering skill (FINAL)

- [ ] Execute through the **steering** skill (draft → conflict-check → approval), not a bare edit. Content per spec §7.6: hard rules (§4), conversion rules (§5), fixture inventory, the `conversion_tests!` exemption text, `-E` selector recipe, Drop-guard convention, SDD-brief duty to name neighboring fixtures. Written from what the cycle actually did — cite real files, real fixtures.
- [ ] `make battery` still green (rules file only).

---

## Self-review notes (plan-author, 2026-09-19)

- Spec coverage: waves 1–6 map to Tasks 1–10 (wave 3 split into Tasks 4–7 by module family for reviewer-sized gates); §6 audits = Task 9; §7 item 6 = Task 10; success criteria 1–7 verifiable at Task 9/10 gates.
- Type consistency: Task 1's API block is the single source; Tasks 2–8 consume it verbatim (`workspace.write` / `seed_workspace` / fixture names checked against each exemplar).
- Known deliberate simplification: exemplars are complete for the patterns, not for all 269 tests — per-task inventories + oracle rules are the contract; implementers apply the pattern file-by-file.
