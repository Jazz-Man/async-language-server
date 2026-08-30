# Structure Cycle (D7) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize `src/` into domain modules per spec `docs/superpowers/specs/2026-08-30-structure-design.md` — pure code motion, zero logic change, public API frozen, verified by battery + rustdoc-inventory diff.

**Architecture:** The file MOVES happen ONCE, UPFRONT, as a user-executed `git mv` batch (rename-preserving history; agents never move files). All tasks below are content-only surgery on the already-moved tree: module declarations, re-exports, import-path rewrites, file splits of moved monoliths.

**Tech Stack:** Rust 2024; `mod.rs`-style module roots.

## Pre-flight (user, before Task 1): the move batch

The user runs and commits (agents must not):

```bash
mkdir -p src/documents src/workspace src/requests src/server/state src/server/with_state
git mv src/document.rs            src/documents/document.rs
git mv src/document_matcher.rs    src/documents/matcher.rs
git mv src/workspace_diagnostics.rs src/workspace/diagnostics.rs
git mv src/workspace_walker.rs    src/workspace/walker.rs
git mv src/requests.rs            src/requests/mod.rs
git mv src/serve.rs               src/server/serve.rs
git mv src/server_trait.rs        src/server/server_trait.rs
git mv src/server_options.rs      src/server/options.rs
git mv src/server_state.rs        src/server/state/mod.rs
git mv src/server_with_state.rs   src/server/with_state/mod.rs
git add -A && git commit -m "restructure: move files to domain layout (compilation restored in follow-ups)"
```

This commit is deliberately red (module paths unresolved until Task 1). Every destination is FINAL — no file moves again in this cycle.

## Global Constraints

- **Pure motion:** every hunk is a module declaration, a re-export, a path edit, or a verbatim block move. No signature, logic, doc-text, or test-content changes (test blocks move VERBATIM — the owner has a separate test revision planned; do not distribute or rewrite them beyond the requests/conversion split the spec assigns).
- **Public API frozen:** the `pub mod server` facade's external surface is byte-identical; `oneshot`, `text_utils`, `lsp_types` paths untouched.
- **Zero lint allows** (tree is at zero — keep it). After Task 1, the battery is green in all four configurations after every task.
- **No git operations and no file moves in tasks.** If anything looks like it needs a move, STOP and report to the controller — the user moves files.
- English; rustfmt-clean.

---

### Task 0: Capture the public-API baseline

**Files:** none (measurement only).

- [ ] **Step 1: Baseline inventory**

On the tree BEFORE the move batch (the controller captures this when handing the user the batch, or the implementer of Task 1 runs it against the pre-batch commit via `git show` — simplest: controller runs it right before the user commits the batch):

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && \
  grep -rho 'id="method\.[a-z_]*"\|id="struct\.[A-Za-z]*"\|id="enum\.[A-Za-z]*"\|id="fn\.[a-z_]*"\|id="type\.[A-Za-z]*"' \
  target/doc/async_language_server/ | sort -u > /tmp/api-before.txt
wc -l /tmp/api-before.txt
```

(Task 5 diffs against this file — it must survive the whole cycle.)

---

### Task 1: Restore compilation — module roots + path rewrites (all domains)

**Files:**
- Create: `src/documents/mod.rs`, `src/workspace/mod.rs`, `src/server/mod.rs`
- Modify: `src/lib.rs`, every file with `crate::<old-module>::` references (brace-import forms included)

- [ ] **Step 1: Domain mod.rs files**

`src/documents/mod.rs`:

```rust
mod document;
mod matcher;

pub(crate) use document::{Document, DocumentReader};
#[cfg(feature = "tree-sitter")]
pub(crate) use document::DocumentQueryCapture;
pub(crate) use matcher::{DocumentMatcher, DocumentMatchers};
```

`src/workspace/mod.rs`:

```rust
mod diagnostics;
mod walker;

pub(crate) use diagnostics::{
    apply_initialization_options, configure_capabilities, did_change_configuration, initialized,
    workspace_diagnostic,
};
pub(crate) use walker::{WorkspaceWalkConfig, WorkspaceWalker, path_to_url};
```

`src/server/mod.rs`: the ENTIRE body of lib.rs's `pub mod server { … }` block (doc comment included) — same `pub use` statements with source paths updated — plus:

```rust
mod options;
mod serve;
mod server_trait;
mod state;
mod with_state;
```

`src/lib.rs` drops the moved mod declarations (`mod document;`, `mod document_matcher;`, `mod serve;`, `mod server_options;`, `mod server_state;`, `mod server_trait;`, `mod server_with_state;`, `mod workspace_diagnostics;`, `mod workspace_walker;`) in favor of `mod documents; mod workspace;`, and the inline `pub mod server { … }` block becomes `pub mod server;`.

- [ ] **Step 2: Rewrite every stale path**

Compiler is the oracle. The rewrite map: `crate::document::`→`crate::documents::`, `crate::document_matcher::`→`crate::documents::`, `crate::workspace_diagnostics::`→`crate::workspace::`, `crate::workspace_walker::`→`crate::workspace::`, `crate::serve::`/`crate::server_trait::`/`crate::server_options::`/`crate::server_state::`/`crate::server_with_state::`→their `crate::server::…` destinations (most consumers already import via the facade and need nothing). `crate::requests::` paths are ALREADY stable (mod.rs at `src/requests/mod.rs`). Grep to empty after: `grep -rn 'crate::document::\|crate::document_matcher::\|crate::workspace_diagnostics::\|crate::workspace_walker::\|crate::serve::\|crate::server_trait::\|crate::server_options::\|crate::server_state::\|crate::server_with_state::' src` → 0.

- [ ] **Step 3: Verify**

Run: `cargo build --all-targets && cargo test && cargo test --no-default-features && cargo test --no-default-features --features tree-sitter && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: green — the tree compiles again with the new layout, monoliths intact.

---

### Task 2: Giants' tests out, verbatim

**Files:**
- Create: `src/server/state/tests.rs`, `src/server/with_state/tests.rs`
- Modify: `src/server/state/mod.rs`, `src/server/with_state/mod.rs` (each gains `#[cfg(test)] mod tests;`, loses the inline block)

- [ ] **Step 1: Extract both test blocks verbatim**

The `#[cfg(test)] mod tests { … }` block of `state/mod.rs` (ex-server_state.rs) moves BYTE-FOR-BYTE into `state/tests.rs` (as `mod tests` content — keep the inner items identical; the `use` lists stay inside). Same for `with_state/mod.rs` → `with_state/tests.rs`. Zero content edits — the range_ext precedent (`lsp_tests.rs`).

- [ ] **Step 2: Verify** — full battery ×4; both giants shrink to code-only (~620/~340 lines).

---

### Task 3: `requests/` — one file per `impl Request`

**Files:**
- Create: `src/requests/conversion.rs` + 15 per-impl files; the request tests move as ONE block to `src/requests/tests.rs`
- Modify: `src/requests/mod.rs` (keeps the trait + re-exports; loses the moved content)

- [ ] **Step 1: Extract `conversion.rs`**

The `modify_incoming_*` / `modify_outgoing_*` family (mod.rs lines ~45-377, ~24 fns) moves to `conversion.rs` as `pub(crate)`; the conversion-helper TESTS move with them to the bottom of `conversion.rs`. Per-impl files import from `super::conversion`.

- [ ] **Step 2: One file per impl**

```
hover.rs completion.rs completion_resolve.rs code_action.rs code_action_resolve.rs
document_link.rs document_link_resolve.rs declaration.rs definition.rs references.rs
rename.rs rename_prepare.rs document_format.rs document_range_format.rs document_diagnostics.rs
```

Each holds its request struct + `impl Request for X` + its `modify_*` overrides; `completion_resolve.rs`/`code_action_resolve.rs` also hold their `convert_*_resolve` pairs (incoming + outgoing). The remaining request tests (everything except the conversion ones) move as ONE verbatim block to `src/requests/tests.rs`.

- [ ] **Step 3: mod.rs = trait + re-exports**

`src/requests/mod.rs` keeps the `Request` trait, declares `mod conversion; mod hover; … #[cfg(test)] mod tests;`, and re-exports (`pub(crate) use`) every request type + the four converters so `crate::requests::<X>` paths — dispatch table, `workspace/diagnostics.rs`, `server/with_state` — are untouched.

- [ ] **Step 4: Verify** — full battery ×4; `grep -rn 'crate::requests::' src/server src/workspace` unchanged (zero edits outside `src/requests/` and `lib.rs`).

---

### Task 4: split `state/`

**Files:**
- Create: `src/server/state/documents.rs`, `src/server/state/workspace.rs`
- Modify: `src/server/state/mod.rs` (keeps structs/accessors; loses the moved impls)

- [ ] **Step 1: Split by the spec map**

- stays in `mod.rs`: `ServerState`, `DocumentEntry`, `DocumentOrigin`; the public accessor impl (`client`, `document`, `documents`); `get/set_position_encoding`; `workspace_diagnostics()`, `set_workspace_diagnostics_enabled`.
- → `documents.rs`: `insert_document`, `handle_document_open/close/change/save`, `recover_failed_incremental_update`, `change_char_range`, `tree_sitter_edit` (feature-gated), `doc_parser` (feature-gated).
- → `workspace.rs`: `set_workspace_folders`, `handle_workspace_folders_change`, `workspace_roots`, `document_urls`, `document_workspace_version`, `refresh_workspace_documents`, `remove_workspace_documents{,_in_roots}`, `url_is_in_roots`, `workspace_folder_path`.

Multiple `impl ServerState` blocks across files of one module are valid; private fields are visible to descendant modules. Cross-file helpers get minimal visibility (`pub(super)` first). Re-export from `mod.rs` whatever siblings/facade consumed before — paths outside `state/` do not change.

- [ ] **Step 2: Verify** — full battery ×4.

---

### Task 5: split `with_state/`

**Files:**
- Create: `src/server/with_state/initialize.rs`
- Modify: `src/server/with_state/mod.rs`

- [ ] **Step 1: Split**

- stays in `mod.rs`: `POSITION_ENCODING_PREFERRED_ORDER`, `implement_method!`/`implement_methods!`, `LanguageServerWithState` + `new`, notification handlers (`initialized`, `did_change_configuration`, `did_change_workspace_folders`, `did_open/close/change/save`), hand-wired `workspace_diagnostic` + both resolve methods, the dispatch table.
- → `initialize.rs`: the `initialize` impl block (negotiation loop, capabilities merge, tracing summary) + the `workspace_folders` helper.

Same rules as Task 4.

- [ ] **Step 2: Verify** — full battery ×4.

---

### Task 6: Final — API-inventory diff, docs rewrite, cycle battery

**Files:**
- Modify: `.claude/rules/structure.md`, `CLAUDE.md` (file-map sections only)

- [ ] **Step 1: Public-inventory diff**

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && \
  grep -rho 'id="method\.[a-z_]*"\|id="struct\.[A-Za-z]*"\|id="enum\.[A-Za-z]*"\|id="fn\.[a-z_]*"\|id="type\.[A-Za-z]*"' \
  target/doc/async_language_server/ | sort -u > /tmp/api-after.txt
diff /tmp/api-before.txt /tmp/api-after.txt   # must be EMPTY
```

- [ ] **Step 2: Stale-path grep**

`grep -rn 'crate::server_state\|crate::server_with_state\|crate::document_matcher\|crate::workspace_walker\|crate::workspace_diagnostics\|crate::server_trait\|crate::server_options\|mod result' src` → 0 lines.

- [ ] **Step 3: Rewrite the steering + project doc file-maps**

`.claude/rules/structure.md` and `CLAUDE.md` reference the old files (`src/server_trait.rs`, `src/requests.rs`, …) — update paths to the new tree; prose (three-places pattern, UTF-8 invariant, layer description) otherwise untouched.

- [ ] **Step 4: Full battery + report** — ×4 configs, fmt, clippy, doc; zero-allow grep; report the empty API-diff. Commits are the user's.
