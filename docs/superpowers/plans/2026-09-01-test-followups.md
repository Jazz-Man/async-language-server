# Test Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the three deferred test follow-ups: route documentLink/resolve through the sole-document resolve heuristic, make tree-sitter `RangeExt` error on out-of-range positions like the other flavors, and make `conversion.rs` state the criterion its `modify_*` helpers actually satisfy.

**Architecture:** Three independent changes, one commit each. Task 1 is a dispatch-level fix proven by two red-first dispatch tests (the bug is invisible to hook-level tests — an echo round trip is a fixpoint). Task 2 is a semantics unification proven by red-first error tests. Task 3 is a behavior-preserving refactor proven by the existing battery plus the duplication gate.

**Tech Stack:** Rust (edition 2024, let-chains allowed), `lsp-types` 0.95.1 / `async-lsp` 0.2.4 pinned by `Cargo.lock`, `futures::executor::block_on` for dispatch tests, `tree-sitter` feature gate.

**Spec:** `docs/superpowers/specs/2026-09-01-test-followups-design.md`

## Global Constraints

- Full battery green in all three feature configurations after every task: `cargo build --all-targets`; `cargo test`; `cargo test --no-default-features`; `cargo test --all-features`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- `cargo dupes check` exit 0 after Task 3 (the delegate pair's `.dupes-ignore.toml` entry `796b160440b3f2f3` is removed with its group; no threshold loosening, no new entries unless a NEW group appears, which then gets one reasoned entry).
- **Git is read-only for agents** (hook blocks writes): every task ends with a suggested commit message; the owner commits. Never `git add`/`git commit`.
- English artifacts; LSP-first navigation (grep only for literal text); no lint suppression, no new `clippy.toml`/`Cargo.toml` allow entries.
- `unwrap`/`expect` are allowed in tests (`allow-unwrap-in-tests`, `allow-expect-in-tests`); production `src/` stays clean.
- Tests use the shared fixtures from `crate::testing` (`src/testing.rs`); temp-workspace tests go through `temp_workspace(prefix, name)` — none are needed in this cycle.

---

### Task 1: documentLink/resolve routes through the resolve macro

**Files:**
- Modify: `src/server/with_state/mod.rs` (dispatch tables, lines ~228-249)
- Modify: `src/requests/document_link_resolve.rs` (drop `extract_url`, add W0 tests)
- Test: `src/server/with_state/tests.rs` (two dispatch tests + local capture server + driver)

**Interfaces:**
- Consumes: `implement_resolve_method!` macro (`src/server/with_state/mod.rs:104`) — generates `fn <lsp_method>(&mut self, params) -> BoxFuture<...>` calling `convert_resolve_item::<$request_type, _>` against the sole tracked document; `convert_resolve_item<R, T>(state, Option<&Document>, &mut T, Direction)` from `src/requests/conversion.rs`; `Server::link_resolve(&self, ServerState, DocumentLink) -> impl Future<Output = ServerResult<DocumentLink>> + Send` (`src/server/server_trait.rs:151`, default: echo the link unchanged).
- Produces: `documentLink/resolve` dispatched with sole-document conversion; `DocumentLinkResolve::extract_url` gone (trait default returns `None`). No public API change.

- [ ] **Step 1: Write the two failing dispatch tests**

In `src/server/with_state/tests.rs`:

Extend the `std` import (line 3) to `use std::{collections::HashMap, fs, path::PathBuf, sync::{Arc, Mutex}};`.

Add `DocumentLink` to the `async_lsp::lsp_types` import list (after `DidOpenTextDocumentParams`). Add `same_line, url` to the `crate::testing` import. Add `use crate::text_utils::Encoding;`.

Add the local capture server and driver next to the other local servers (`DisabledServer`/`ConfigurableServer`):

```rust
struct LinkCaptureServer {
    received: Arc<Mutex<Option<Range>>>,
}

impl Server for LinkCaptureServer {
    fn link_resolve(
        &self,
        _state: ServerState,
        link: DocumentLink,
    ) -> impl std::future::Future<Output = ServerResult<DocumentLink>> + Send {
        let received = Arc::clone(&self.received);
        async move {
            *received.lock().expect("capture mutex") = Some(link.range);
            Ok(link)
        }
    }
}

/// Drives documentLink/resolve over real dispatch: opens `documents`,
/// sends a link at UTF-16 position (0,2) with the given target, and returns
/// (what the handler received, what the client got back).
fn drive_link_resolve(documents: &[(&str, &str)], target: Option<Url>) -> (Option<Range>, Range) {
    let received = Arc::new(Mutex::new(None));
    let mut server = LanguageServerWithState::new(
        ClientSocket::new_closed(),
        LinkCaptureServer {
            received: Arc::clone(&received),
        },
    );
    server.state.set_position_encoding(Encoding::UTF16);
    for (name, text) in documents {
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: url(name),
                language_id: "plaintext".into(),
                version: 0,
                text: (*text).into(),
            },
        });
    }

    let resolved =
        futures::executor::block_on(server.document_link_resolve(DocumentLink {
            range: same_line(0, 2, 2),
            target,
            tooltip: None,
            data: None,
        }))
        .expect("link resolves");

    (
        *received.lock().expect("capture mutex"),
        resolved.range,
    )
}
```

Add the two tests (anywhere in the tests section, next to the other `#[test]` functions):

```rust
#[test]
fn link_resolve_converts_against_the_sole_tracked_document() {
    // One tracked document ("🙂abc": byte 4 == UTF-16 unit 2); the link's
    // target points at an untracked URL. Resolve-side conversion keys on
    // the sole document, never the target.
    let (received, returned) =
        drive_link_resolve(&[("only.txt", "🙂abc")], Some(url("untracked.md")));

    assert_eq!(received, Some(same_line(0, 4, 4)));
    assert_eq!(returned, same_line(0, 2, 2));
}

#[test]
fn link_resolve_skips_conversion_without_a_sole_document() {
    // Two tracked documents: the params cannot name the source document,
    // and the target is the OTHER tracked document. Conversion must be
    // skipped — the handler sees the client's UTF-16 positions verbatim,
    // not positions converted against the target's text.
    let (received, returned) = drive_link_resolve(
        &[("a.txt", "🙂abc"), ("b.txt", "🙂🙂")],
        Some(url("b.txt")),
    );

    assert_eq!(received, Some(same_line(0, 2, 2)));
    assert_eq!(returned, same_line(0, 2, 2));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test link_resolve`
Expected: BOTH tests FAIL on the `received` assertion.
- `link_resolve_converts_against_the_sole_tracked_document`: today `extract_url` returns the untracked target, so no conversion runs — the handler receives `same_line(0, 2, 2)` but the test expects `same_line(0, 4, 4)`.
- `link_resolve_skips_conversion_without_a_sole_document`: today the range is converted against the tracked target `"🙂🙂"` — UTF-16 unit 2 becomes byte 4, the handler receives `same_line(0, 4, 4)` but the test expects `same_line(0, 2, 2)`.

- [ ] **Step 3: Move the dispatch line and drop the bogus override**

In `src/server/with_state/mod.rs`, delete this line from the `implement_methods!` table (line ~240):

```rust
        document_link_resolve   => link_resolve          @ crate::requests::DocumentLinkResolve,
```

and add after the `code_action_resolve` entry (line ~233):

```rust
    implement_resolve_method!(
        document_link_resolve => link_resolve @ crate::requests::DocumentLinkResolve
    );
```

In `src/requests/document_link_resolve.rs`, delete the `extract_url` override (lines 16-18) and leave the sibling's explanatory comment in its place, so the impl reads:

```rust
impl Request for DocumentLinkResolve {
    type Params = LspDocumentLink;
    type Response = LspDocumentLink;

    // DocumentLink doesn't contain a source document URI; the resolve
    // dispatch macro supplies the sole tracked document.

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        convert_range(state, document, &mut response.range, Direction::Outgoing);
    }
}
```

The `Url` import becomes unused — the file's first line drops to
`use async_lsp::lsp_types::DocumentLink as LspDocumentLink;`.

- [ ] **Step 4: Run the dispatch tests to verify they pass**

Run: `cargo test link_resolve`
Expected: both dispatch tests PASS. (The resolve macro finds the sole document in test 1 and converts UTF-8↔UTF-16; in test 2 there is no sole document, so positions pass through verbatim.)

- [ ] **Step 5: Add the W0 hook-level pins**

These pin the `Request` hooks directly (the convention every `Request` impl carries). They pass immediately — the hooks are already correct; the bug was dispatch-level. Append to `src/requests/document_link_resolve.rs`:

```rust
#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use async_lsp::lsp_types::{DocumentLink, Range};

    use crate::requests::{Direction, DocumentLinkResolve, convert_resolve_item};
    use crate::server::{ServerOptions, ServerState};
    use crate::testing::{TestServer, open_document, same_line, state_with_documents, url};
    use crate::text_utils::Encoding;

    fn link(range: Range) -> DocumentLink {
        DocumentLink {
            range,
            target: None,
            tooltip: None,
            data: None,
        }
    }

    #[test]
    fn resolve_range_converts_against_the_sole_tracked_document() {
        // Exactly one tracked document ("🙂abc"), UTF-16 negotiated.
        let mut state = ServerState::with_options::<TestServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        state.set_position_encoding(Encoding::UTF16);
        open_document(&mut state, url("only.txt"), "🙂abc");

        let mut item = link(same_line(0, 4, 4));

        let document = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            Some(&document),
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 2, 2));
    }

    #[test]
    fn resolve_range_passes_through_without_a_document() {
        // No document snapshot: the range passes through unchanged.
        let (state, _, _) = state_with_documents();

        let mut item = link(same_line(0, 4, 4));

        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            None,
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 4, 4));
    }

    #[test]
    fn resolve_echo_round_trip_is_identity() {
        // Sole doc "🙂abc", UTF-16 negotiated. The client echoes the link at
        // the UTF-16 position it was delivered: the incoming converter must
        // turn it into UTF-8 for the handler, and the outgoing converter must
        // return the original position — no double conversion.
        let mut state = ServerState::with_options::<TestServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        state.set_position_encoding(Encoding::UTF16);
        open_document(&mut state, url("only.txt"), "🙂abc");

        let mut item = link(same_line(0, 2, 2));

        let sole = state
            .document(&url("only.txt"))
            .expect("sole document is tracked");
        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Incoming,
        );
        convert_resolve_item::<DocumentLinkResolve, _>(
            &state,
            Some(&sole),
            &mut item,
            Direction::Outgoing,
        );

        assert_eq!(item.range, same_line(0, 2, 2));
    }
}
```

Note: `DocumentLink` does NOT derive `Default` (verified: `Debug, Eq, PartialEq, Clone, Deserialize, Serialize` in `lsp-types` 0.95.1) — that is why the `link(range)` helper spells out all four fields instead of `..Default::default()`.

- [ ] **Step 6: Run the full battery**

Run the Global Constraints battery (all three feature configurations + fmt + clippy + doc).
Expected: all green, including `cargo test --no-default-features` (the new tests use no tree-sitter API).

- [ ] **Step 7: Report for commit**

Suggested: `fix: documentLink/resolve converts against the sole tracked document, not the link target`

---

### Task 2: tree-sitter RangeExt errors on out-of-range positions

**Files:**
- Modify: `src/text_utils/range_ext/tree_sitter.rs` (`split_at` lines 10-61, `sub` lines 101-168)
- Test: `src/text_utils/range_ext/tree_sitter_tests.rs`

**Interfaces:**
- Consumes: `RangeError::PositionOutOfRange` (`src/error.rs:23`), the existing `check_text_length` gate.
- Produces: `TsRange::split_at` and `TsRange::sub` return `Err(RangeError::PositionOutOfRange)` when the given relative position is not representable in the range's text. Behavior change on the public `RangeExt` trait (downstream note in the commit message); no internal call sites exist (verified).

- [ ] **Step 1: Write the three failing error tests**

Append to `src/text_utils/range_ext/tree_sitter_tests.rs` (the file's local `r`/`p` builders and the `RangeError` import already exist):

```rust
#[test]
fn split_at_beyond_the_text_returns_position_out_of_range() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .split_at(text, p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
    // A row past the last line is equally out of range.
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .split_at(text, p(2, 0))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
}

#[test]
fn sub_positions_beyond_the_text_return_position_out_of_range() {
    let text = "hello";
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 1), p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
    assert_eq!(
        r(0, p(0, 0), 5, p(0, 5))
            .sub(text, p(0, 9), p(0, 9))
            .unwrap_err(),
        RangeError::PositionOutOfRange
    );
}

#[test]
fn split_at_mismatched_text_length_returns_text_range_mismatch() {
    let text = "short";
    assert_eq!(
        r(0, p(0, 0), 7, p(0, 7))
            .split_at(text, p(0, 2))
            .unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        }
    );
}
```

- [ ] **Step 2: Run them to verify the first two fail (the third passes)**

Run: `cargo test tree_sitter_tests` (tree-sitter is a default feature, so the plain invocation compiles the gated file)
Expected: `split_at_beyond_the_text_returns_position_out_of_range` and `sub_positions_beyond_the_text_return_position_out_of_range` FAIL — the calls return `Ok` with a degenerate split (`at_byte` stuck at `start_byte`), so `unwrap_err` panics. `split_at_mismatched_text_length_returns_text_range_mismatch` already PASSES (`check_text_length` runs first in today's code) — it is the missing direct pin, not a regression test.

- [ ] **Step 3: Error on unfound positions**

In `src/text_utils/range_ext/tree_sitter.rs`, `split_at` — replace the end-of-text block (lines 42-45):

```rust
        // Handle end-of-text case if position wasn't found in loop
        if !found && current_row == at.row && current_col == at.column {
            at_byte = self.end_byte;
        }
```

with:

```rust
        // Handle end-of-text case if position wasn't found in loop;
        // a position that is nowhere in the text is out of range.
        if !found {
            if current_row == at.row && current_col == at.column {
                at_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }
```

In the same file, `sub` — replace the end-of-text block (lines 154-160):

```rust
        // Handle end-of-text case for positions not found in loop
        if !found_from && current_row == from.row && current_col == from.column {
            from_byte = self.end_byte;
        }
        if !found_to && current_row == to.row && current_col == to.column {
            to_byte = self.end_byte;
        }
```

with:

```rust
        // Handle end-of-text cases for positions not found in loop;
        // a position that is nowhere in the text is out of range.
        if !found_from {
            if current_row == from.row && current_col == from.column {
                from_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }
        if !found_to {
            if current_row == to.row && current_col == to.column {
                to_byte = self.end_byte;
            } else {
                return Err(RangeError::PositionOutOfRange);
            }
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test tree_sitter_tests`
Expected: all tree-sitter range tests PASS, including the pre-existing boundary tests (`split_at_boundaries`, `split_off_boundaries` — positions at text end take the end-of-text branch, not the error branch).

- [ ] **Step 5: Run the full battery**

Run the Global Constraints battery. `cargo test --no-default-features` matters here: the file is `#[cfg(feature = "tree-sitter")]` and must simply not compile into that configuration.
Expected: all green.

- [ ] **Step 6: Report for commit**

Suggested: `fix: tree-sitter RangeExt errors on out-of-range positions like the other flavors` with the one-line breaking note `behavior change: out-of-range split_at/sub used to return a degenerate Ok`.

---

### Task 3: conversion.rs states the real criterion

**Files:**
- Modify: `src/requests/conversion.rs` (module doc lines 1-9; delete `modify_incoming_diagnostic` lines 186-192 and `modify_outgoing_diagnostic` lines 194-200)
- Modify: `src/requests/code_action.rs` (imports lines 9-13; call sites lines 26, 36)
- Modify: `src/requests/document_diagnostics.rs` (import line 9; call site line 24)
- Modify: `.dupes-ignore.toml` (remove the entry with fingerprint `796b160440b3f2f3`, lines 41-43)

**Interfaces:**
- Consumes: `convert_diagnostic(&ServerState, &Document, &mut LspDiagnostic, Direction)` (`src/requests/conversion.rs:172`).
- Produces: two fewer `pub(crate)` helpers; the module doc's criterion matches the surviving `modify_*` set. No behavior change.

- [ ] **Step 1: Rewrite the module doc**

Replace lines 1-9 of `src/requests/conversion.rs`:

```rust
//! Centralized position-encoding conversion for the `Request` hooks.
//!
//! Two verb families live here: `convert_*` helpers are
//! direction-parameterized (`Direction::Incoming` = client encoding to
//! UTF-8, before the handler; `Direction::Outgoing` = UTF-8 to the client
//! encoding, after), while the remaining `modify_*` helpers are
//! fixed-direction composites that mix per-document and per-URL conversion —
//! no pure direction pins over a `convert_*` helper remain.
```

- [ ] **Step 2: Delete the two delegates and update the call sites**

Delete from `src/requests/conversion.rs`:

```rust
pub(crate) fn modify_incoming_diagnostic(
    state: &ServerState,
    document: &Document,
    diag: &mut LspDiagnostic,
) {
    convert_diagnostic(state, document, diag, Direction::Incoming);
}

pub(crate) fn modify_outgoing_diagnostic(
    state: &ServerState,
    document: &Document,
    diag: &mut LspDiagnostic,
) {
    convert_diagnostic(state, document, diag, Direction::Outgoing);
}
```

In `src/requests/code_action.rs`, change the import block to:

```rust
    conversion::{Direction, convert_diagnostic, convert_range, convert_workspace_edit},
```

and the two call sites to:

```rust
        for diag in &mut params.context.diagnostics {
            convert_diagnostic(state, document, diag, Direction::Incoming);
        }
```

```rust
                        for diag in diagnostics {
                            convert_diagnostic(state, document, diag, Direction::Outgoing);
                        }
```

In `src/requests/document_diagnostics.rs`, change the import to:

```rust
    conversion::{Direction, convert_diagnostic, modify_outgoing_diagnostic_report_kind_at_url},
```

and the call site (line 24) to:

```rust
                for diag in &mut report.full_document_diagnostic_report.items {
                    convert_diagnostic(state, document, diag, Direction::Outgoing);
                }
```

- [ ] **Step 3: Remove the dupes-ignore entry**

In `.dupes-ignore.toml`, delete lines 41-43:

```toml
[[ignore]]
fingerprint = "796b160440b3f2f3"
reason = "named incoming/outgoing wrappers over the direction-generic convert_diagnostic (T20f); call sites read better than raw Direction arguments"
```

- [ ] **Step 4: Verify tests and the duplication gate**

Run: `cargo test` then `cargo dupes check`
Expected: all tests green (the W0 tests for `code_action` and `document_diagnostics` exercise these paths; behavior identical); `cargo dupes check` exits 0 — the delegate pair's duplicate group dissolves with the deletion, and no new group appears (the three one-line call sites are far below `min_nodes = 15`).

- [ ] **Step 5: Run the full battery**

Run the Global Constraints battery. Expected: all green.

- [ ] **Step 6: Report for commit**

Suggested: `refactor: drop pure direction delegates, conversion doc states the real criterion`

---

## Self-Review (done at plan time)

- **Spec coverage:** Change 1 = macro move + override drop + three W0 tests (spec's exact list) plus two dispatch tests that make the routing fix red-first verifiable; Change 2 = `split_at` + `sub` error paths with the three named tests; Change 3 = delegate deletion, three call sites, module doc rewrite, `.dupes-ignore.toml` entry `796b160440b3f2f3` removal, `cargo dupes check` exit 0. Constraints section mirrors the spec verbatim. Out-of-scope untouched.
- **Placeholders:** none — every step carries exact code, commands, and expected outcomes (including which assertions fail and why).
- **Type consistency:** `convert_resolve_item<R, T>` where `R: Request<Params = T, Response = T>` — `DocumentLinkResolve` satisfies it (`Params = Response = LspDocumentLink`); `drive_link_resolve` returns `(Option<Range>, Range)` matching both tests' destructuring; `DocumentLink` constructed with all four fields (no `Default` derive); `Direction` added to `document_diagnostics.rs` imports (verified absent today).
