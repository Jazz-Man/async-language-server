# LSP Surface — Plan 1: Conversion Test Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the table-driven W0 conversion test harness (`conversion_tests!` macro in `src/testing.rs`) and retrofit the existing conversion tests into rows, so that Plans 2–3 add one row per new `Request` instead of copied test bodies.

**Architecture:** One `macro_rules!` stamps a complete `#[test]` per row: fixture → `modify_params` → assert UTF-8 → `modify_response` → assert client encoding. Rows live in each request file's `#[cfg(test)] mod tests`. A hand-written reference test stays beside the macro's row for the same case, pinning the expansion's semantics. Spec: `docs/superpowers/specs/2026-09-01-lsp-surface-completion-design.md`, section "Architecture 2".

**Tech Stack:** Rust edition 2024, `macro_rules!` + `pub(crate) use` export, existing fixtures in `src/testing.rs`.

## Global Constraints

- Owner commits — every task ends at a review checkpoint with a file list; no git commands anywhere.
- The `"🙂abc"` document and UTF-16 negotiation in `state_with_documents` are load-bearing: byte offset 4 == UTF-16 offset 2. The emoji document is the fixture's **target** URL; the plain `"abcdef"` document is **source**.
- `Request` hooks (exact signatures, `src/requests/mod.rs:71-82`): `modify_params(state: &ServerState, document: &Document, params: &mut Self::Params)`, `modify_response(state: &ServerState, document: &Document, response: &mut Self::Response)`. Both take the REQUEST's document.
- Feature configurations: all changes must compile and pass under default, `--no-default-features`, and `--all-features`.
- `expect`/`unwrap` allowed in tests (`clippy.toml`). Production `src/` stays unwrap/expect-clean.
- `cargo dupes check` must exit 0; if macro expansions trip it, one reasoned `.dupes-ignore.toml` entry for the macro itself — never per row.
- Verbatim rule from the spec: "Coverage boundary: single incoming position (+ optional single outgoing position pair)." Multi-position, resolve-family, and capture-server tests stay hand-written.

---

### Task 1: The `conversion_tests!` macro and the two canonical rows

**Files:**
- Modify: `src/testing.rs` (append the macro + export after the fixtures)
- Modify: `src/requests/hover.rs` (append `#[cfg(test)] mod tests` with two rows)
- Modify: `src/requests/definition.rs` (append two rows to the existing `mod tests`; keep the hand-written test)

**Interfaces:**
- Consumes: `state_with_documents() -> (ServerState, Url, Url)` (plain source, emoji target), `line_position`, `same_line` from `crate::testing`; `Request` trait as above.
- Produces: `crate::testing::conversion_tests` — a `pub(crate) use`-exported `macro_rules!` invoked once per test module with rows of this grammar (both `incoming` and the `response`/`outgoing`/`returns` triple are optional):

```rust
conversion_tests! {
    $name:ident : $request:ty {
        params:    $params:expr,     // Fn(Url) -> Params; Url is the EMOJI document (the request's document)
        incoming:  $incoming:expr,   // Fn(&Params) -> Position, asserted equal to `expects` (UTF-8)
        expects:   $expects:expr,
        response:  $response:expr,   // Fn(Url, Url) -> Response; (plain_url, emoji_url)
        outgoing:  $outgoing:expr,   // Fn(&Response) -> Position, asserted equal to `returns` (client encoding)
        returns:   $returns:expr,
    }
}
```

- [ ] **Step 1: Write the failing rows (macro does not exist yet)**

Append to `src/requests/hover.rs`:

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        HoverContents, MarkupContent, MarkupKind, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::Hover;
    use crate::requests::Request;

    conversion_tests! {
        hover_incoming_utf16_becomes_utf8: Hover {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
        }
        hover_outgoing_utf8_becomes_utf16: Hover {
            params: |uri| async_lsp::lsp_types::HoverParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(async_lsp::lsp_types::Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: "x".into(),
                }),
                range: Some(same_line(0, 4, 4)),
            }),
            outgoing: |r| r.as_ref().expect("hover present").range.expect("range present").start,
            returns: line_position(0, 2),
        }
    }
}
```

Append inside the existing `mod tests` in `src/requests/definition.rs` (below the hand-written test, which stays as the macro's reference pin):

```rust
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    conversion_tests! {
        definition_round_trips_both_directions: Definition {
            params: |uri| GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(GotoDefinitionResponse::Scalar(Location::new(
                emoji,
                same_line(0, 4, 4),
            ))),
            outgoing: |r| match r.as_ref() {
                Some(GotoDefinitionResponse::Scalar(loc)) => loc.range.start,
                _ => panic!("expected scalar location"),
            },
            returns: line_position(0, 2),
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test hover_incoming`
Expected: FAIL — compile error, `cannot find macro conversion_tests` (or unresolved import `crate::testing::conversion_tests`).

- [ ] **Step 3: Implement the macro in `src/testing.rs`**

Append after `json_matchers()`:

```rust
/// Stamps one `#[test]` per row for a [`crate::requests::Request`]'s
/// conversion hooks — the table-driven W0 harness.
///
/// Row grammar (both `incoming`/`expects` and the
/// `response`/`outgoing`/`returns` triple are optional):
///
/// - `params` — `Fn(Url) -> Params`, building params against the **emoji**
///   document (the request's document), positions expressed in the CLIENT
///   encoding (UTF-16 in this fixture).
/// - `incoming`/`expects` — `Fn(&Params) -> Position` and the UTF-8
///   (byte-column) position it must equal after `modify_params`.
/// - `response` — `Fn(Url, Url) -> Response` receiving
///   `(plain_url, emoji_url)`, positions built in UTF-8.
/// - `outgoing`/`returns` — `Fn(&Response) -> Position` and the
///   client-encoding position it must equal after `modify_response`.
///
/// Coverage boundary: a single incoming position and an optional single
/// outgoing position. Anything richer stays hand-written.
macro_rules! conversion_tests {
    ($(
        $name:ident : $request:ty {
            params: $params:expr
            $(, incoming: $incoming:expr, expects: $expects:expr)?
            $(, response: $response:expr, outgoing: $outgoing:expr, returns: $returns:expr)?
            $(,)?
        }
    )*) => {
        $(
        #[test]
        fn $name() {
            let (state, plain, emoji) = crate::testing::state_with_documents();
            let document = state.document(&emoji).expect("emoji document is tracked");
            let mut params = ($params)(emoji.clone());
            <$request as $crate::requests::Request>::modify_params(&state, &document, &mut params);
            $(
            assert_eq!(
                ($incoming)(&params),
                $expects,
                "incoming position must be converted to the UTF-8 byte column",
            );
            )?
            $(
            let mut response = ($response)(plain.clone(), emoji.clone());
            <$request as $crate::requests::Request>::modify_response(&state, &document, &mut response);
            assert_eq!(
                ($outgoing)(&response),
                $returns,
                "outgoing position must be converted to the client encoding",
            );
            )?
        }
        )*
    };
}
pub(crate) use conversion_tests;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test hover_ && cargo test definition_`
Expected: PASS — `hover_incoming_utf16_becomes_utf8`, `hover_outgoing_utf8_becomes_utf16`, `definition_round_trips_both_directions` (plus the hand-written `definition_locations_are_converted_using_their_own_document`, unchanged).

- [ ] **Step 5: Prove the rows are load-bearing (red-check)**

Temporarily change `expects: line_position(0, 4)` to `expects: line_position(0, 2)` in the hover incoming row, run `cargo test hover_incoming`, confirm FAIL, revert. This verifies the row detects a deleted conversion (the fixpoint ceiling documented in the spec's testing rule).

- [ ] **Step 6: Feature matrix + dupes**

Run: `cargo test --no-default-features && cargo test --all-features && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo dupes check`
Expected: all green; dupes exit 0 (if the macro's expansions trip dupes, add ONE reasoned `.dupes-ignore.toml` entry for `conversion_tests` — never per-row).

- [ ] **Step 7: Review checkpoint**

Report for review. Files changed: `src/testing.rs`, `src/requests/hover.rs`, `src/requests/definition.rs` (optionally `.dupes-ignore.toml`). The owner commits.

---

### Task 2: Retrofit the regular-shape requests into rows

**Files:**
- Modify: `src/requests/declaration.rs`, `src/requests/references.rs`, `src/requests/document_link.rs`, `src/requests/document_format.rs`, `src/requests/document_range_format.rs`, `src/requests/rename_prepare.rs` — each gains a `#[cfg(test)] mod tests` with rows
- Modify: `src/requests/completion.rs` — only if a regular single-position row fits its incoming side; its response test stays hand-written
- Untouched (irregular, stay hand-written): `completion_resolve.rs`, `code_action.rs`, `code_action_resolve.rs`, `rename.rs`, `document_diagnostics.rs`, `document_link_resolve.rs`

**Interfaces:**
- Consumes: `crate::testing::conversion_tests` from Task 1, same row grammar and fixture semantics (params against the emoji URL; response closure receives `(plain, emoji)`).
- Produces: every regular-shape `Request` carries at least one row — the per-method W0 duty from the spec's Architecture 2. Plans 2–3 add rows, never test bodies, for regular methods.

Each file below gets this exact skeleton (imports adjusted per file), with rows as shown. These are pinning tests over existing behavior — they must pass immediately; a failure means a real conversion gap, to be investigated (no-workarounds rule), not papered over.

- [ ] **Step 1: Add rows to `declaration.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::requests::Request;
    use crate::testing::{conversion_tests, line_position, same_line};

    use super::Declaration;

    conversion_tests! {
        declaration_round_trips_both_directions: Declaration {
            params: |uri| GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(GotoDefinitionResponse::Scalar(Location::new(
                emoji,
                same_line(0, 4, 4),
            ))),
            outgoing: |r| match r.as_ref() {
                Some(GotoDefinitionResponse::Scalar(loc)) => loc.range.start,
                _ => panic!("expected scalar location"),
            },
            returns: line_position(0, 2),
        }
    }
}
```

- [ ] **Step 2: Add rows to `references.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        Location, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::requests::Request;
    use crate::testing::{conversion_tests, line_position, same_line};

    use super::References;

    conversion_tests! {
        references_round_trips_both_directions: References {
            params: |uri| ReferenceParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, emoji| Some(vec![Location::new(emoji, same_line(0, 4, 4))]),
            outgoing: |r| r.as_ref().expect("locations present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
```

- [ ] **Step 3: Add rows to `document_link.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{DocumentLink, DocumentLinkParams, TextDocumentIdentifier};

    use crate::requests::Request;
    use crate::testing::{conversion_tests, same_line};

    use super::DocumentLink;

    conversion_tests! {
        document_link_ranges_convert_outgoing: DocumentLink {
            params: |uri| DocumentLinkParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
            response: |plain, _emoji| Some(vec![DocumentLink {
                range: same_line(0, 4, 4),
                target: Some(plain),
                tooltip: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("links present")[0].range.start,
            returns: crate::testing::line_position(0, 2),
        }
    }
}
```

(Note: `DocumentLink` has exactly four fields and no `Default` derive — spell all four; `data: None` keeps the row resolve-safe.)

- [ ] **Step 4: Add rows to `document_format.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentFormattingParams, TextDocumentIdentifier, TextEdit,
    };

    use crate::requests::Request;
    use crate::testing::{conversion_tests, line_position, same_line};

    use super::DocumentFormat;

    conversion_tests! {
        document_format_edits_convert_outgoing: DocumentFormat {
            params: |uri| DocumentFormattingParams {
                text_document: TextDocumentIdentifier::new(uri),
                options: Default::default(),
                work_done_progress_params: Default::default(),
            },
            response: |_plain, _emoji| Some(vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }]),
            outgoing: |r| r.as_ref().expect("edits present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
```

- [ ] **Step 5: Add rows to `document_range_format.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentRangeFormattingParams, TextDocumentIdentifier, TextEdit,
    };

    use crate::requests::Request;
    use crate::testing::{conversion_tests, line_position, same_line};

    use super::DocumentRangeFormat;

    conversion_tests! {
        document_range_format_round_trips_both_directions: DocumentRangeFormat {
            params: |uri| DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier::new(uri),
                range: same_line(0, 2, 3),
                options: Default::default(),
                work_done_progress_params: Default::default(),
            },
            incoming: |p| p.range.start,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }]),
            outgoing: |r| r.as_ref().expect("edits present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
```

(The incoming range `same_line(0, 2, 3)` spans UTF-16 units 2–3 — bytes 4–5 (`a`) — so `start` pins to byte 4.)

- [ ] **Step 6: Add rows to `rename_prepare.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        PrepareRenameDefaultBehavior, PrepareRenameParams, PrepareRenameResult,
        TextDocumentIdentifier, TextDocumentPositionParams,
    };

    use crate::requests::Request;
    use crate::testing::{conversion_tests, line_position, same_line};

    use super::RenamePrepare;

    conversion_tests! {
        rename_prepare_round_trips_both_directions: RenamePrepare {
            params: |uri| PrepareRenameParams {
                text_document_position_params: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                work_done_progress_params: Default::default(),
            },
            incoming: |p| p.text_document_position_params.position,
            expects: line_position(0, 4),
            response: |_plain, _emoji| Some(PrepareRenameResult::RangeWithPlaceholder {
                range: same_line(0, 4, 4),
                placeholder: "x".into(),
            }),
            outgoing: |r| match r.as_ref() {
                Some(PrepareRenameResult::RangeWithPlaceholder { range, .. }) => range.start,
                _ => panic!("expected range with placeholder"),
            },
            returns: line_position(0, 2),
        }
    }
}
```

(If `PrepareRenameResult`'s variant or helper names differ in the pinned lsp-types 0.95.1 — e.g. a `RangeWithPlaceholder` struct variant versus tuple — adjust the row to the real shape read from `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lsp-types-0.95.1/src/`; the assertion columns stay exactly as above.)

- [ ] **Step 7: Assess `completion.rs` — row for the incoming side only, if it fits**

Read the existing `mod tests` in `src/requests/completion.rs`. If its params are the standard single-position shape, add one incoming-only row mirroring the hover row (params `CompletionParams { text_document_position_params, work_done_progress_params, partial_result_params: Default::default(), context: None }`) with the same `incoming`/`expects` pair. If the existing tests already pin the incoming conversion hand-written and a row would only duplicate them, leave the file untouched and say so in the report.

- [ ] **Step 8: Run everything**

Run: `cargo test`
Expected: PASS — all new rows green; all previously hand-written tests unchanged and green.

- [ ] **Step 9: Full battery + dupes**

Run: `cargo build --all-targets && cargo test --no-default-features && cargo test --all-features && cargo fmt --check && cargo clippy --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && cargo dupes check`
Expected: all green, dupes exit 0.

- [ ] **Step 10: Review checkpoint**

Report for review. Files changed: the six (or seven, per Step 7) request files. The owner commits.

---

### Task 3: Module doc and ledger

**Files:**
- Modify: `src/testing.rs` (module doc paragraph, if not already added in Task 1)

**Interfaces:**
- Consumes: nothing new.
- Produces: documentation only.

- [ ] **Step 1: Extend the `src/testing.rs` module doc**

Append one paragraph after the existing `r()` paragraph:

```rust
//! The `conversion_tests!` macro at the bottom of this module is the
//! table-driven W0 harness: one row stamps the standard conversion test
//! (fixture → modify_params → UTF-8 assert → modify_response → client
//! assert). Rows pin the single-incoming-position shape; richer tests stay
//! hand-written next to their `Request` impls.
```

- [ ] **Step 2: Battery**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo doc --no-deps`
Expected: green.

- [ ] **Step 3: Review checkpoint**

Report for review. File changed: `src/testing.rs`. The owner commits.
