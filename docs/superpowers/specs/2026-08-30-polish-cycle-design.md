# Polish Cycle — design

**Date:** 2026-08-30
**Status:** approved design, pre-implementation
**Inputs:** deferred-Minor queues of the Cycle 1 and Cycle 2 final reviews (ledger `.superpowers/sdd/progress.md`), verified against the committed tree at `915d5b8`
**Branch:** `feature/abstraction`

## Goal

Close the six deferred polish items in one mini-cycle: one behavioral hardening (resolve snapshot) and five mechanical cleanups. No public API changes, no breaking changes.

## Decisions

- **D1 — Resolve snapshot taken once.** The hand-wired `completion_item_resolve`/`code_action_resolve` methods capture the sole tracked document as `Option<Document>` (cheap snapshot clone) once, before the handler; both the incoming and outgoing converters receive `Option<&Document>` instead of re-deriving it from the store. The did_open/did_close race between the two conversions is closed by construction: both sides see the same snapshot. Converter signatures become `fn convert_*_resolve(state: &ServerState, document: Option<&Document>, …)` (`state` retained for the encoding check).
- **D2 — Pass-through test builds its own two-multibyte fixture** rather than the shared `state_with_documents()` (whose ASCII source would mask a wrong sole-document picker); the shared fixture is untouched, so its six dependent tests keep their semantics.
- **D3 — `Encoding` shadow removed.** The inherent `pub const fn default()` (`encoding.rs:37`) is deleted; `impl Default for Encoding` returns `Self::UTF16` directly. All three call sites are runtime contexts; the doctest calls `Encoding::default()` through the trait. No public API change (`Default::default()` remains available; const-callability of `Encoding::default()` is dropped — no const callers exist).
- **D4 — Test-only constructor deleted.** `ServerState::new` (currently `#[cfg(test)]`) is removed; its ten call sites (`server_state.rs` tests ×7, `requests.rs` tests ×3) inline `ServerState::with_options::<T>(ClientSocket::new_closed(), &ServerOptions::default())`.
- **D5 — Headroom restored.** The tree-sitter `InputEdit` construction block inside `handle_document_change` moves into a private helper next to `change_char_range`, dropping the parent from 99/100 counted lines to roughly 65.
- **D6 — Rule wording unified.** `error-handling.md`: one label for the non-adapter population ("All domain code — trait impls, state, walkers —"); the carve-out parenthetical pluralizes to "the staleness `CONTENT_MODIFIED` replies" (two sites exist).

## Design

### Task 1: Resolve snapshot (D1)

`src/server_with_state.rs` — in both hand-wired methods, before the handler call:

```rust
let sole = {
    let documents = state.documents();
    (documents.len() == 1).then(|| documents[0].clone())
};
```

`src/requests.rs` — all four converters: `document: Option<&Document>` parameter replaces the internal `documents()`/`let [document] … else` derivation; body becomes `if let Some(document) = document { …delegate to the Request hook… }` after the existing UTF-8 early return. Call sites pass `sole.as_ref()`. The three existing resolve tests update to build `Some(&document)`/`None` directly; the round-trip test now runs both converters against one captured `Option` — pinning the same-snapshot contract.

### Task 2: Mechanics + tests (D2–D4)

- Pass-through test: own state (`ServerState::with_options::<T>(…, &ServerOptions::default())`, UTF-16), two documents both with multibyte text (e.g. `"🙂abc"` and `"🙂def"`).
- `Encoding`: delete the inherent `const fn default`; `impl Default` body → `Self::UTF16`; the doctest's `Encoding::default()` assertion stays (trait call).
- Constructor inlining per D4 (ten sites, plus any import additions the tests need: `ServerOptions`).

### Task 3: Headroom (D5)

Extract the feature-gated `InputEdit` block from `handle_document_change` into `fn tree_sitter_edit(…)` (private, `#[cfg(feature = "tree-sitter")]`), placed with the other private helpers. Pure extraction — no behavior change; the Cycle-1 regression tests still exercise the moved code.

### Task 4: Rule wording (D6)

Two one-line edits in `.claude/rules/error-handling.md`.

## Acceptance

- Battery green in four configurations (default, no-default, all-features, no-default+tree-sitter); `grep -rn 'allow(' src examples` → 0; the resolve round-trip test green under the snapshot design.
- `grep -n 'pub const fn default' src/text_utils/encoding.rs` → 0; `grep -n 'cfg(test)' src/server_state.rs` shows only the tests module.
- `handle_document_change` under 80 counted lines (clippy `too_many_lines` threshold 100).
- No public API or behavior changes beyond D1's race-window tightening (which makes previously one-way-corrupted cases identity).

## Provenance

Items trace to the final reviews of Cycles 1 and 2 (deferred as Minors with file:line evidence); all six re-verified on the tree at `915d5b8` on 2026-08-30.
