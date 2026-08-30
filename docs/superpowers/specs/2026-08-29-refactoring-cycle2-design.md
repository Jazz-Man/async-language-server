# Refactoring Cycle 2 — hygiene and docs — design

**Date:** 2026-08-29
**Status:** approved design, pre-implementation
**Inputs:** technical review `docs/superpowers/reviews/2026-08-29-technical-review.md` (M5/M6/M8/I8/I9), Cycle 1 outcomes (ledger + final-review triage), owner decisions 2026-08-29 (full allow sweep to zero, target clients Zed/Claude Code, breaking changes accepted)
**Branch:** `feature/abstraction`

## Goal

Zero lint-suppression debt: after this cycle `grep -rn 'allow(' src examples` returns nothing, the deprecated `rootUri` path is gone, the README is real crate documentation, and the error rule is reconciled with the wire layer's actual shape.

## Decisions

- **D1 — Full allow sweep to zero.** All 23 remaining `#[allow]`/`#![allow]` entries across `src/` and `examples/` are eliminated by root-fixing what each masks. Zero-allow state becomes an acceptance criterion (grep). No new allows may appear.
- **D2 — `rootUri` fallback removed.** Target clients are Zed (LSP 3.16 via their lsp-types fork) and Claude Code; `workspaceFolders` (LSP 3.6+) is the only root source read. The `#[allow(deprecated)]` disappears with the fallback.
- **D3 — Rule reconciliation via carve-out.** `.claude/rules/error-handling.md` gains an explicit clause: the wire adapter (`LanguageServerWithState` and the workspace-diagnostics request layer) is itself the boundary and constructs protocol-native `ResponseError` values directly (staleness `CONTENT_MODIFIED`, workspace-diagnostics-disabled `METHOD_NOT_FOUND`); domain code — trait impls, state, walkers — stays `ServerError`-only. Rationale: these sites ARE the edge (`err-edge-mapping`'s "map at the edge"); routing through `ServerError::rpc(..).into()` would construct a domain error only to immediately unwrap it into the same wire form.
- **D4 — I2 resolved minimally.** `completion_item_resolve`/`code_action_resolve` are hand-wired (precedent: `workspace_diagnostic`) and finally invoke their `modify_response` hooks, converting edit ranges against the document store (an open document is the typical resolve case). Untracked documents pass through, documented. The full store-or-disk URL-less machinery stays with the symbols cycle (spec `2026-08-28-symbols-design.md` §4).
- **D5 — I8 resolved by removal.** The two `RangeExt` default methods with `unimplemented!()` bodies lose their default bodies entirely — the methods become required for implementors (breaking; the crate's own byte/lsp/tree-sitter impls already implement them). The panicking-default trap and its two `unused_variables` allows disappear together.
- **D6 — M6: `DocumentMatcher` fields privatized.** Construction only via `new` + `with_*` builders; a public `name()` getter replaces the public field (needed for diagnostics). Breaking for struct-literal construction and field access; doctests rewritten builder-only.
- **D7 — Style wave.** All three `src/server_trait.rs` module allows go (`unused_imports`, `unused_variables` — root-fixing whatever they mask — and `must_use_candidate` → `#[must_use]` on `Server` trait methods and wherever clippy requires; semantically right for futures: an un-awaited future is a bug); `needless_pass_by_value` → reference signatures in `server_state` internals and a breaking signature change for `ServerError::rpc` (form chosen by clippy's suggestion, e.g. `impl AsRef<str>` or `String`); `too_many_lines` → split the offending functions into private helpers. This is deliberately NOT the file-reorganization pass (cycle-1 spec D7 stays registered).
- **D8 — dead_code resolved use-or-delete.** For each `#[allow(dead_code)]` site (`document_matcher.rs:95,102`, `server_state.rs:40,96`, `requests.rs:32`): unused members are deleted (YAGNI — re-add when needed), genuinely needed ones are wired. The compiler after allow-removal is the oracle; the per-task brief lists the candidates.

## Design

### 1. Scope and success criteria

**In:** nine cluster tasks — (1) rule reconciliation, (2) wire-layer + I2-minimal, (3) cast safety, (4) mechanics + dead code (incl. I8), (5) rootUri removal, (6) style wave, (7) M6 matcher privatization, (8) README + M8, (9) pinning test + final battery.

**Out:** file reorganization beyond lint-triggered function splits (cycle-1 D7), dependency upgrades (Dependabot/deliberate), the symbols feature (separate cycle).

**Success:** full battery green in three feature configurations plus `--no-default-features --features tree-sitter`; `grep -rn 'allow(' src examples` → **0 lines**; `grep -rn 'deprecated' src` → only spec-quoted doc text, no `#[allow(deprecated)]`; error-rule compliance on all touched code; README renders as complete crate documentation.

### 2. Rule reconciliation (task 1)

`.claude/rules/error-handling.md`'s "One boundary, both directions" section gains the D3 clause verbatim-intent wording; the three wire sites (`server_with_state.rs:72` staleness, `workspace_diagnostics.rs:249` disabled, `:426` same) remain as they are — they become compliant by the rule's own text. No code change in this task.

### 3. Wire-layer + I2-minimal (task 2)

- Hand-wire `completion_item_resolve` and `code_action_resolve` in `LanguageServerWithState` next to the `workspace_diagnostic` precedent: call the `Server` method, then run the resolve impl's `modify_response` with a store-only conversion for `text_edit`/`additional_text_edits` ranges. Resolve params carry no document URL and edits carry none either, so the conversion document is chosen by a documented rule: the **sole tracked document** when exactly one is open (the normal completion-then-resolve flow), otherwise the response passes through unchanged. Trait method docs state this contract.
- `requests.rs`: with the resolve hooks now invoked, `#[allow(dead_code)]` and `#[allow(unused_variables)]` come off the `Request` trait; unused default parameters take `_`-prefixed names in the trait's default signatures.

### 4. Cast safety (task 3)

Five `#[allow(clippy::cast_possible_truncation)]` sites (`tree_sitter_utils.rs:21`, `examples/minimal.rs:39`, `text_utils/position.rs:40`, `text_utils/range_ext/lsp.rs:30,116`): replace `as u32`/`as usize` casts in position math with `TryFrom`/`try_into` conversions carrying an explicit failure path (clamped or erroring per each site's semantics — position math clamps, as the crate's conversion layer already does for out-of-range client positions).

### 5. Mechanics + dead code (task 4)

- I8 per D5 (RangeExt defaults removed; doctest examples unchanged — they exercise the implemented methods).
- `server_with_state.rs:280` vestigial allow dropped (`_params` already underscore-prefixed).
- Dead-code sites per D8: `DocumentMatchers` (`document_matcher.rs:95,102`) and the `server_state.rs:40,96` members — delete unused, wire needed.
- `extra_unused_type_parameters` ×2: drop the unused `<T: Server>` from `insert_document` and `handle_document_save` (both `pub(crate)`; call sites simplified).

### 6. rootUri removal (task 5)

`workspace_folders` keeps only the `workspace_folders` field path. A client that sends neither `workspaceFolders` nor (now unread) `rootUri` gets no workspace roots — documented behavior for an LSP 3.16+ client baseline.

### 7. Style wave (task 6)

Per D7. The `ServerError::rpc` signature change is the cycle's public breaking change alongside D5/D6; commit guidance names all three.

### 8. M6 + README + M8 (tasks 7–8)

- Matcher: private fields, `pub fn name(&self) -> &str`, builders unchanged, doctest rewritten.
- README: quick-start (minimal server, `no_run` fence), API tour (Server trait, matchers, serve/transports, workspace diagnostics, oneshot, text_utils, feature flags), stability section in the fork's voice. README code fences are doctests via `include_str!` — every example compiles in the battery.
- M8: `Transport::into_read_write` `# Errors` rewritten to name connect failures (the current "port is not valid" branch is unreachable).

### 9. Tests + battery (task 9)

- Extend `initialize_ignores_unknown_client_encodings`: a second params set advertising only an unknown kind asserts `position_encoding == Some(UTF16)` (pins the `Encoding::default()` fallback).
- Full battery ×3 configs + the tree-sitter corner config; zero-allow grep; error-rule pass over all touched files.

## Acceptance

- Battery green in all configurations; zero `allow(` in `src/` and `examples/`; zero `#[allow(deprecated)]`.
- I2 (resolve conversion) and I8 (panicking RangeExt defaults) closed; M5/M6/M8/I9 closed.
- Error rule amended per D3 and satisfied on all touched code.
- The cycle's commit guidance names the three breaking changes (`ServerError::rpc` signature, `RangeExt` required methods, `DocumentMatcher` private fields).

## Provenance

Findings reference the 2026-08-29 technical review; allow inventory re-verified on the post-Cycle-1 tree (23 entries, six clusters). Owner decisions 2026-08-29: full sweep (D1), Zed/Claude-Code baseline enabling D2, carve-out over routing (D3), privatization (D6). Related specs: `2026-08-29-refactoring-cycle1-design.md` (D1 two-cycle split), `2026-08-28-symbols-design.md` §4 (full URL-less machinery, untouched here).
