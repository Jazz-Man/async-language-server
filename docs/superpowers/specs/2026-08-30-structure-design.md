# Structure Cycle (D7) — domain modules — design

**Date:** 2026-08-30
**Status:** approved design, pre-implementation
**Inputs:** cycle-1 spec D7 registration (kilometer files), owner's restructuring wishes 2026-08-30, file-size survey on `feature/structure` @ `d6108c2` (requests.rs 1223, server_with_state.rs 1038, server_state.rs 946, workspace_diagnostics.rs 565, document.rs 302, document_matcher.rs 178)
**Branch:** `feature/structure`

## Goal

Reorganize `src/` into domain modules with per-`Request` files and split server internals — pure code motion, zero logic change, public API frozen — so the tree stays navigable as LSP coverage grows (the 1223-line `requests.rs` is a fraction of the eventual method set).

## Decisions

- **D1 — Domain modules.** `documents/`, `requests/`, `workspace/`, and a `server/` module root; `error.rs`, `transport.rs`, `tree_sitter_utils.rs`, `oneshot/`, `text_utils/` stay where they are.
- **D2 — `error.rs` stays top-level.** Every domain (documents, workspace, oneshot, server) depends on `ServerError`; placing it under `server/` would invert the dependency direction (domains importing from server). It is cross-cutting plumbing, same tier as `text_utils`. Re-exported through the `server` facade unchanged.
- **D3 — One file per `impl Request`** (~15 files), plus `conversion.rs` for the shared `modify_incoming_*`/`modify_outgoing_*` family and `mod.rs` holding the `Request` trait and `pub use` re-exports so `crate::requests::<Type>` paths (used by the dispatch table) never change.
- **D4 — Server giants: tests out, then code split.** Tests move verbatim as one block into sibling `tests.rs` files (`#[cfg(test)] mod tests;` — the `range_ext` precedent); then `server_state.rs` splits into `state/{mod,documents,workspace,tests}.rs` and `server_with_state.rs` into `with_state/{mod,initialize,tests}.rs`. Multiple `impl ServerState` blocks across files of one module are valid; private fields are visible to descendant modules.
- **D5 — Tests are relocated, not reworked.** The owner has a separate test revision planned; D7 only moves test blocks mechanically (requests' block intact into `requests/tests.rs`; conversion-helper tests into `conversion.rs`; giants' blocks into their `tests.rs`). No distribution, no rewriting.
- **D6 — Public API frozen.** The `pub mod server` facade moves verbatim from `lib.rs` into `src/server/server.rs` (module root, 2018 style) with internal paths updated; external paths (`async_language_server::server::*`, `::oneshot::*`, `::text_utils::*`, `::lsp_types`) are byte-identical. Verified in the final task by diffing the public rustdoc inventory before/after.
- **D7 — Steering and project docs updated with the tree.** `.claude/rules/structure.md` and `CLAUDE.md` describe the old layout; the final task rewrites their file-map sections for the new tree (they steer future agents — a stale map breeds wrong references).

## Target tree

```
src/
  lib.rs                      // facade paths updated only
  error.rs  transport.rs  tree_sitter_utils.rs
  documents/
    mod.rs  document.rs  matcher.rs
  requests/
    mod.rs                    // Request trait + re-exports
    conversion.rs             // modify_* helper family + its tests
    hover.rs completion.rs completion_resolve.rs code_action.rs
    code_action_resolve.rs document_link.rs document_link_resolve.rs
    declaration.rs definition.rs references.rs rename.rs rename_prepare.rs
    document_format.rs document_range_format.rs document_diagnostics.rs
    tests.rs                   // the request tests, moved as one block
  workspace/
    mod.rs  diagnostics.rs  walker.rs
  server/
    server.rs                 // the facade (ex lib.rs `pub mod server` block)
    serve.rs  server_trait.rs  options.rs
    state/    mod.rs  documents.rs  workspace.rs  tests.rs
    with_state/  mod.rs  initialize.rs  tests.rs
  oneshot/   text_utils/      // unchanged
```

## Task order (seven, each battery-gated, pure motion)

1. **`documents/`** — move `document.rs` + `document_matcher.rs`→`matcher.rs`, add `mod.rs` re-exports; internal `crate::document::` references update.
2. **`workspace/`** — move `workspace_diagnostics.rs` + `workspace_walker.rs`→`walker.rs`, `mod.rs`.
3. **`requests/`** — split per D3/D5; dispatch-table paths stable via re-exports.
4. **`server/` root** — facade to `server/server.rs`, move `serve.rs`/`server_trait.rs`/`options.rs`; move the two giants' test blocks out verbatim into `state/tests.rs`/`with_state/tests.rs` (files still monolithic here).
5. **`state/` split** per D4: `mod.rs` (structs, accessors, encoding), `documents.rs` (open/close/change/save, insert, recover, change_char_range, tree_sitter_edit, doc_parser), `workspace.rs` (roots/urls/refresh/remove/folders + path helpers).
6. **`with_state/` split** per D4: `mod.rs` (macros, struct, dispatch table, notifications, handwires), `initialize.rs` (encoding negotiation, capabilities, workspace_folders).
7. **Final**: full battery ×4 configs; rustdoc public-inventory diff (before/after must be empty); `grep` for stale internal paths (`crate::server_state`, `crate::server_with_state`, `crate::document_matcher`, `crate::workspace_walker`, `mod result` → 0); rewrite the file-map sections of `.claude/rules/structure.md` and `CLAUDE.md`.

## Acceptance

- Battery green in four configurations; zero lint allows; no logic diffs anywhere (reviewer-checkable: every hunk is a move, rename, or path edit).
- Public rustdoc inventory identical before/after.
- No file over ~450 lines except the moved-as-is test files (`with_state/tests.rs` ~700); every `impl Request` in its own file.
- `structure.md` + `CLAUDE.md` describe the new tree.

## Provenance

Owner wishes 2026-08-30 (domain modules, per-Request files, server grouping, transport/tree_sitter flat, error.rs analyzed); sizes surveyed at `d6108c2`; test-revision note honored as D5.
