# Testing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved testing design — requests-harness extraction, the `RangeError` typing pass, the W2/W3 wire-tier integration client with the full 15-test catalog, the unit-tier gap fills, and the testing steering document.

**Architecture:** Four code phases in dependency order (pure motion → typing → wire tier → gap fills) plus one documentation phase. The wire tier drives the real middleware stack through a `pub(crate)` helper extracted from `serve()`; its client side is a hand-rolled raw JSON-RPC client with no async-lsp involvement.

**Tech Stack:** Rust (edition 2024, pinned stable toolchain), async-lsp 0.2.4, tokio (existing features only), futures, serde_json, thiserror, tree-sitter + tree-sitter-json (dev, feature-gated).

**Spec:** `docs/superpowers/specs/2026-08-31-testing-implementation-design.md`

## Global Constraints

- **Type first, test second**: before writing any test, ask "can this invalid state be removed by a type?". A type must remove a representable invalid state or separate a genuinely confusable pair. No tests for quantity.
- Full battery gates every phase: `cargo build --all-targets`; `cargo test`; `cargo test --no-default-features`; `cargo test --all-features`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- No new dependencies. No `#[allow]`/`--cap-lints`/suppression of any kind: a failing check is investigated to root cause.
- **Git is read-only for agents** (repo hook blocks writes): every task ends by reporting a suggested commit message; the owner commits. Never run `git add`/`git commit`.
- All written artifacts (code, comments, docs, commit messages) in English.
- Use the LSP tools for code navigation and the `rust-skills` rules for Rust decisions.
- Tests never use wall-clock sleeps; every cross-task await is bounded (`tokio::time::timeout`, 5 s unless stated; the `time` feature is added to the existing tokio dev-dependency — futures-rs has no time support at all, the research's `FutureExt::timeout` premise was wrong).
- `Encoding` is `Copy` (`src/text_utils/encoding.rs:17`).
- Line references below were verified at plan time against `develop` @ `1fc97a3`; if drift is found, re-anchor before editing.

---

### Task 1: Extract the requests test harness

**Files:**
- Create: `src/requests/testing.rs`
- Modify: `src/requests/mod.rs:22-23`
- Delete: `src/requests/tests.rs`
- Modify: `src/requests/definition.rs`, `src/requests/rename.rs`, `src/requests/completion.rs`, `src/requests/completion_resolve.rs`, `src/requests/code_action.rs`, `src/requests/document_diagnostics.rs` (append test modules)

**Interfaces:**
- Produces: `crate::requests::testing::{TestServer, url, p, r, open_document, state_with_documents}` — `pub(crate)`, `#[cfg(test)]`-gated, signatures identical to today's private helpers.

- [ ] **Step 1: Create `src/requests/testing.rs`**

```rust
//! Test-only baseline for per-request conversion tests.
//!
//! Declared as `#[cfg(test)] mod testing;` in `mod.rs`; never compiled into
//! non-test builds. The `"🙂abc"` document and the UTF-16 encoding are
//! load-bearing: U+1F642 is 4 UTF-8 bytes but 2 UTF-16 units, so byte
//! offset 4 == UTF-16 offset 2 — that identity is what these tests assert.

use async_lsp::{
    ClientSocket,
    lsp_types::{DidOpenTextDocumentParams, Position, Range, TextDocumentItem, Url},
};

use crate::{
    server::{Server, ServerOptions, ServerState},
    text_utils::Encoding,
};

pub(crate) struct TestServer;

impl Server for TestServer {}

pub(crate) fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}

pub(crate) const fn p(line: u32, character: u32) -> Position {
    Position { line, character }
}

pub(crate) const fn r(line: u32, start: u32, end: u32) -> Range {
    Range {
        start: p(line, start),
        end: p(line, end),
    }
}

pub(crate) fn open_document(state: &mut ServerState, uri: Url, text: impl Into<String>) {
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri, "test".into(), 1, text.into()),
    });
}

pub(crate) fn state_with_documents() -> (ServerState, Url, Url) {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_position_encoding(Encoding::UTF16);

    let source = url("source.txt");
    let target = url("target.txt");
    open_document(&mut state, source.clone(), "abcdef");
    open_document(&mut state, target.clone(), "🙂abc");

    (state, source, target)
}
```

- [ ] **Step 2: Rewire `src/requests/mod.rs`**

Replace lines 22-23:

```rust
#[cfg(test)]
mod tests;
```

with:

```rust
#[cfg(test)]
mod testing;
```

- [ ] **Step 3: Move the nine tests into their request files**

Move each test below VERBATIM from `src/requests/tests.rs` (only the `use` imports change: `use crate::requests::testing::{...}` plus the request types via `super::...` as shown). Append at the end of the destination file:

| test (tests.rs lines) | destination | imports to add in its `mod tests` |
|---|---|---|
| `definition_locations_are_converted_using_their_own_document` (62-77) | `definition.rs` | `use std::collections::HashMap;` not needed; `use async_lsp::lsp_types::{GotoDefinitionResponse, Location};` `use crate::requests::testing::{r, state_with_documents};` `use super::{Definition, Request};` |
| `workspace_edits_are_converted_using_their_own_document` (79-96) | `rename.rs` | `use std::collections::HashMap;` `use async_lsp::lsp_types::{TextEdit, WorkspaceEdit};` `use crate::requests::testing::{r, state_with_documents};` `use super::{Rename, Request};` |
| `rename_edits_fall_back_to_request_document_when_target_is_unknown` (182-200) | `rename.rs` | same as above plus `url` |
| `completion_additional_text_edits_are_converted` (98-117) | `completion.rs` | `use async_lsp::lsp_types::{CompletionItem, CompletionResponse, TextEdit};` `use crate::requests::testing::{r, state_with_documents};` `use super::{Completion, Request};` |
| `code_action_context_diagnostics_are_converted` (119-142) | `code_action.rs` | `use async_lsp::lsp_types::{CodeActionContext, CodeActionParams, Diagnostic, PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams};` `use crate::requests::testing::{r, state_with_documents};` `use super::{CodeAction, Request};` |
| `document_diagnostic_related_documents_are_converted_using_their_own_document` (144-180) | `document_diagnostics.rs` | `use std::collections::HashMap;` `use async_lsp::lsp_types::{Diagnostic, DocumentDiagnosticReport, DocumentDiagnosticReportKind, DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport};` `use crate::requests::testing::{r, state_with_documents};` `use super::{DocumentDiagnostics, Request};` |
| `resolve_edits_convert_against_the_sole_tracked_document` (202-230) | `completion_resolve.rs` | see block below |
| `resolve_edits_pass_through_without_a_document` (232-252) | `completion_resolve.rs` | see block below |
| `resolve_echo_round_trip_is_identity` (254-286) | `completion_resolve.rs` | see block below |

Each destination file gets exactly this shape:

```rust
#[cfg(test)]
mod tests {
    // … the imports from the table above …
    // … the moved test functions, bodies unchanged …

    // For completion_resolve.rs specifically:
    // use async_lsp::lsp_types::{CompletionItem, CompletionTextEdit as LspCompletionTextEdit, TextEdit};
    // use crate::requests::testing::{open_document, r, state_with_documents, url};
    // use crate::server::{ServerOptions, ServerState};
    // use crate::text_utils::Encoding;
    // use async_lsp::ClientSocket;
    // use super::{convert_completion_resolve, convert_incoming_completion_resolve};
    // plus `use super::testing::TestServer;` via `use crate::requests::testing::TestServer;`
    // NOTE: the two sole-document tests construct their own state with
    // `ServerState::with_options::<TestServer>(ClientSocket::new_closed(), &ServerOptions::default())`
    // — keep `TestServer` imported for them.
}
```

Bodies are copied from `src/requests/tests.rs` unchanged, with exactly two textual adjustments: `super::convert_completion_resolve` → `convert_completion_resolve` (already in scope via the `use super::...`), and `super::convert_incoming_completion_resolve` → `convert_incoming_completion_resolve`.

- [ ] **Step 4: Delete `src/requests/tests.rs`**

- [ ] **Step 5: Verify**

Run: `cargo test --lib requests::`
Expected: 9 passed (same tests, new paths).
Run: `cargo fmt` then `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Report for commit**

Suggested message: `refactor: distribute requests tests in-domain behind a cfg(test) harness (requests::testing)`

---

### Task 2: Delete the doctest-duplicated tests

**Files:**
- Modify: `src/text_utils/conversions.rs` (delete `converts_utf8_columns_to_utf16`, lines 87-95)
- Modify: `src/text_utils/range_ext/bytes_tests.rs` (delete 8 tests)

**Interfaces:** none (test-only deletions; the doctests that carry the coverage remain).

- [ ] **Step 1: Delete from `src/text_utils/conversions.rs`** the whole `converts_utf8_columns_to_utf16` test (lines 87-95) — byte-identical to the `position_to_encoding` doctest at lines 9-18 which runs in all three feature configurations.

- [ ] **Step 2: Delete from `src/text_utils/range_ext/bytes_tests.rs`** these 8 tests (each byte-identical to the `sub_delimited`/`sub_delimited_tri` doctests in `range_ext/mod.rs:104-151`): `basic_sub_delimited` (48), `basic_sub_delimited_tri` (55), `sub_delimited_delimiter_at_start` (82), `sub_delimited_delimiter_at_end` (89), `sub_delimited_no_delimiter` (96), `sub_delimited_empty_text` (103), `sub_delimited_tri_partial` (110), `sub_delimited_tri_no_delimiters` (118).

- [ ] **Step 3: Leave the pointer comment** where the deleted block was in `bytes_tests.rs`:

```rust
// Delimiter cases live in the `sub_delimited` / `sub_delimited_tri`
// doctests in `mod.rs`; they are not duplicated here.
```

Do NOT touch `lsp_tests.rs` / `tree_sitter_tests.rs` (nothing duplicates them).

- [ ] **Step 4: Verify**

Run: `cargo test --lib text_utils:: && cargo test --doc`
Expected: all pass; test count drops by exactly 9.

- [ ] **Step 5: Report for commit**

Suggested message: `test: drop nine unit tests byte-duplicated by doctests`

---

### Task 3: Add `RangeError` to `src/error.rs`

**Files:**
- Modify: `src/error.rs` (add enum after `ServerResult` alias)
- Modify: `src/server/mod.rs:26` (extend the error re-export)
- Modify: `src/text_utils/mod.rs:16` (re-export for `RangeExt` consumers)

**Interfaces:**
- Produces: `pub enum RangeError { PositionOutOfRange, StartAfterEnd, NotSingleLine, DelimiterNotSingleByte { delimiter: char }, TextRangeMismatch { text_len: usize, range_len: usize } }` — `Debug, Clone, Copy, PartialEq, Eq, Error`, `#[non_exhaustive]`. Import paths: `async_language_server::server::RangeError` and `async_language_server::text_utils::RangeError`.

- [ ] **Step 1: Add the enum to `src/error.rs`** (after the `ServerResult` alias, before `ServerError`):

```rust
/// Failures of [`RangeExt`](crate::text_utils::RangeExt) operations.
///
/// A leaf-utility error without protocol semantics: it never crosses the
/// wire itself and is mapped by the caller at their own boundary
/// (absorbable into [`ServerError::Other`] by boxing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RangeError {
    /// The position lies beyond the end of the range.
    #[error("position lies beyond the end of the range")]
    PositionOutOfRange,
    /// The subrange start lies after its end.
    #[error("subrange start lies after its end")]
    StartAfterEnd,
    /// `shrink` was called on a range that spans multiple lines.
    #[error("shrink requires a single-line range")]
    NotSingleLine,
    /// The delimiter is not a single-byte UTF-8 character.
    #[error("delimiter {delimiter:?} is not a single-byte UTF-8 character")]
    DelimiterNotSingleByte {
        /// The offending delimiter.
        delimiter: char,
    },
    /// The text is not the exact text of the range.
    #[error("text length {text_len} does not match range length {range_len}")]
    TextRangeMismatch {
        /// Length of the text in bytes.
        text_len: usize,
        /// Length of the range.
        range_len: usize,
    },
}
```

- [ ] **Step 2: Export it.** In `src/server/mod.rs:26` change:

```rust
pub use crate::error::{ServerError, ServerErrorCode, ServerResult};
```

to:

```rust
pub use crate::error::{RangeError, ServerError, ServerErrorCode, ServerResult};
```

In `src/text_utils/mod.rs` next to `pub use self::range_ext::RangeExt;` add:

```rust
pub use crate::error::RangeError;
```

- [ ] **Step 3: Verify the layer police still holds**

Run: `cargo test --test architecture`
Expected: pass (`src/error.rs` sits outside every `[[scopes]]` glob; `text_utils → crate::error` matches no deny rule). If it fails, STOP and investigate the scope globs in `arch-lint.toml` — do not suppress.

Run: `cargo build --all-targets && cargo clippy --all-targets -- -D warnings`
Expected: clean (`missing_docs` satisfied by the doc comments above).

- [ ] **Step 4: Report for commit**

Suggested message: `feat: add RangeError, the typed failure of RangeExt (breaking: part 1 of 2)`

---

### Task 4: Make `RangeExt` fallible

**Files:**
- Modify: `src/text_utils/range_ext/mod.rs` (trait + shared checks + doctests)
- Modify: `src/text_utils/range_ext/bytes.rs`
- Modify: `src/text_utils/range_ext/lsp.rs`
- Modify: `src/text_utils/range_ext/tree_sitter.rs`
- Modify: `src/text_utils/range_ext/bytes_tests.rs`, `lsp_tests.rs`, `tree_sitter_tests.rs` (mechanical pass)

**Interfaces:**
- Consumes: `RangeError` from Task 3.
- Produces: all `RangeExt` methods return `Result<_, RangeError>`; `split_off_left/right` remain provided methods. `#[must_use]` is dropped from the now-`Result`-returning methods (`Result` is already `#[must_use]`; keeping the attribute trips `double_must_use`).

- [ ] **Step 1: Replace the trait and add the shared checks** in `mod.rs`. The new trait (docs preserved, `# Panics` sections removed since nothing panics anymore):

```rust
use crate::error::RangeError;

fn check_delimiter(delimiter: char) -> Result<(), RangeError> {
    if delimiter.len_utf8() == 1 {
        Ok(())
    } else {
        Err(RangeError::DelimiterNotSingleByte { delimiter })
    }
}

fn check_text_length(text_len: usize, range_len: usize) -> Result<(), RangeError> {
    if text_len == range_len {
        Ok(())
    } else {
        Err(RangeError::TextRangeMismatch { text_len, range_len })
    }
}

pub trait RangeExt: Sized {
    /// The position type used by this kind of range.
    type Position;

    /// Splits the given range into two parts at the specified position.
    ///
    /// - The `text` parameter must be the exact text corresponding to this range.
    ///   It is used for tree-sitter ranges, where both line+col and byte offsets are needed.
    /// - The `at` position is _relative_ to the start of the range.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::PositionOutOfRange`] if `at` lies beyond the
    /// end of the range.
    fn split_at(self, text: &str, at: Self::Position) -> Result<(Self, Self), RangeError>;

    /// Splits the given range into two parts at the specified position,
    /// and returns the left part.
    ///
    /// # Errors
    ///
    /// Returns the error of [`RangeExt::split_at`].
    fn split_off_left(self, text: &str, at: Self::Position) -> Result<Self, RangeError> {
        Ok(self.split_at(text, at)?.0)
    }

    /// Splits the given range into two parts at the specified position,
    /// and returns the right part.
    ///
    /// # Errors
    ///
    /// Returns the error of [`RangeExt::split_at`].
    fn split_off_right(self, text: &str, at: Self::Position) -> Result<Self, RangeError> {
        Ok(self.split_at(text, at)?.1)
    }

    /// Shrinks the same-line range by the given character count, on both the left and right.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::NotSingleLine`] if the range spans multiple lines.
    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError>;

    /// Returns a subrange of the range, starting at `from` and ending at `to`.
    ///
    /// Both positions are _relative_ to the start of the range, and the range
    /// itself must be an absolute range.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::PositionOutOfRange`] if `from` or `to` lie beyond
    /// the end of the range, or [`RangeError::StartAfterEnd`] if `from > to`.
    fn sub(self, text: &str, from: Self::Position, to: Self::Position) -> Result<Self, RangeError>;

    /// Splits the given range into two optional subranges, using the given delimiter.
    ///
    /// The range should be the exact range for the given text.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::DelimiterNotSingleByte`] for a multi-byte
    /// delimiter, and (for the byte and tree-sitter ranges)
    /// [`RangeError::TextRangeMismatch`] when the text is not the exact
    /// text of the range.
    fn sub_delimited(self, text: &str, delimiter: char) -> Result<(Option<Self>, Option<Self>), RangeError>;

    /// Splits the given range into _three_ optional subranges,
    /// using the two given delimiters, consecutively.
    ///
    /// The range should be the exact range corresponding to the given text.
    ///
    /// # Errors
    ///
    /// Returns [`RangeError::DelimiterNotSingleByte`] for a multi-byte
    /// delimiter, and (for the byte and tree-sitter ranges)
    /// [`RangeError::TextRangeMismatch`] when the text is not the exact
    /// text of the range.
    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> Result<(Option<Self>, Option<Self>, Option<Self>), RangeError>;
}
```

Update the trait-level doctest (mod.rs lines 27-38) to the fallible form:

```rust
/// use async_language_server::text_utils::RangeExt;
///
/// let (left, right) = (0..7).split_at("one/two", 3).expect("position is inside the range");
/// assert_eq!(left, 0..3);
/// assert_eq!(right, 3..7);
///
/// assert_eq!((0..7).shrink(1, 2).expect("single-line range"), 1..5);
/// assert_eq!((0..7).sub("one/two", 1, 5).expect("positions are inside the range"), 1..5);
```

Apply the same `.expect(...)` suffix to the `sub_delimited`/`sub_delimited_tri` doctest assertions (lines 104-151), wrapping call sites: `(0..7).sub_delimited("one/two", D)` becomes `(0..7).sub_delimited("one/two", D).expect("valid range")` inside each `assert_eq!`.

- [ ] **Step 2: Rewrite `bytes.rs`** (full new content):

```rust
use super::{RangeError as _ /* not needed; via super::check_* */, check_delimiter, check_text_length};
```

(Use plainly: `use super::{check_delimiter, check_text_length};` and reference `crate::error::RangeError` via `use crate::error::RangeError;`.)

```rust
use crate::error::RangeError;

use super::{check_delimiter, check_text_length};

type ByteRange = std::ops::Range<usize>;
type BytePosition = usize;

impl super::RangeExt for ByteRange {
    type Position = BytePosition;

    fn split_at(self, _text: &str, at: Self::Position) -> Result<(Self, Self), RangeError> {
        if at > self.end - self.start {
            return Err(RangeError::PositionOutOfRange);
        }
        Ok((self.start..(self.start + at), (self.start + at)..self.end))
    }

    fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError> {
        // Byte ranges have no line concept, so shrinking cannot fail.
        let new_start = self.start.saturating_add(amount_left).min(self.end);
        let new_end = self.end.saturating_sub(amount_right).max(self.start);
        Ok(new_start..new_end)
    }

    fn sub(self, _text: &str, from: Self::Position, to: Self::Position) -> Result<Self, RangeError> {
        let len = self.end - self.start;
        if from > len || to > len {
            return Err(RangeError::PositionOutOfRange);
        }
        if from > to {
            return Err(RangeError::StartAfterEnd);
        }
        Ok((self.start + from)..(self.start + to))
    }

    fn sub_delimited(self, text: &str, delim: char) -> Result<(Option<Self>, Option<Self>), RangeError> {
        check_text_length(text.len(), self.end - self.start)?;
        check_delimiter(delim)?;

        if let Some(offset) = text.find(delim) {
            Ok((
                if offset == 0 {
                    None // delimiter is the first character
                } else {
                    Some(self.clone().split_off_left(text, offset)?)
                },
                if offset + 1 >= text.len() {
                    None // delimiter is the last character
                } else {
                    Some(self.clone().split_off_right(text, offset + 1)?)
                },
            ))
        } else if !text.is_empty() {
            Ok((Some(self), None))
        } else {
            Ok((None, None))
        }
    }

    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> Result<(Option<Self>, Option<Self>, Option<Self>), RangeError> {
        check_delimiter(delim0)?;
        check_delimiter(delim1)?;

        if text.is_empty() {
            return Ok((None, None, None));
        }

        check_text_length(text.len(), self.end - self.start)?;

        let Some(delim0_offset) = text.find(delim0) else {
            return Ok((Some(self), None, None));
        };

        let (first, remainder) = self.sub_delimited(text, delim0)?;
        let Some(remainder) = remainder else {
            return Ok((first, None, None));
        };

        let remainder_start = remainder.start - self.start;
        let remainder_text = &text[remainder_start..];

        let (second, third) = remainder.sub_delimited(remainder_text, delim1)?;
        Ok((first, second, third))
    }
}
```

- [ ] **Step 3: Rewrite `lsp.rs`.** Same pattern. Exact conversions from the current file:
  - `split_at`: replace `assert!(at_absolute >= self.start && at_absolute <= self.end);` with an early `if !(at_absolute >= self.start && at_absolute <= self.end) { return Err(RangeError::PositionOutOfRange); }`; wrap the tuple in `Ok(...)`.
  - `shrink`: replace `assert_eq!(self.start.line, self.end.line, "shrink only supports single-line ranges");` with `if self.start.line != self.end.line { return Err(RangeError::NotSingleLine); }`; wrap in `Ok(...)`.
  - `sub`: `assert!(from <= to);` → `if from > to { return Err(RangeError::StartAfterEnd); }`; both sanity asserts → `if !(x >= self.start && x <= self.end) { return Err(RangeError::PositionOutOfRange); }`; wrap in `Ok(...)`.
  - `sub_delimited`: `assert_eq!(delim.len_utf8(), 1, ...)` → `check_delimiter(delim)?;`. The `unwrap_or(u32::MAX)` on `u32::try_from` stays (capping, not an invariant). Wrap returns in `Ok(...)`; internal `self.split_off_left(text, delim_pos)` / `split_off_right` calls get `?`.
  - `sub_delimited_tri`: replace the delimiter asserts with `check_delimiter(delim0)?; check_delimiter(delim1)?;`, keep the empty-text early return as `Ok((None, None, None))`, and restructure to the find-first shape so the existing `#[expect(clippy::expect_used, ...)]`/`expect("delim0 was found")` disappears entirely:

```rust
    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> Result<(Option<Self>, Option<Self>, Option<Self>), RangeError> {
        check_delimiter(delim0)?;
        check_delimiter(delim1)?;

        if text.is_empty() {
            return Ok((None, None, None));
        }

        let Some(delim0_offset) = text.find(delim0) else {
            return Ok((Some(self), None, None));
        };

        let (first, remainder) = self.sub_delimited(text, delim0)?;
        let Some(remainder) = remainder else {
            return Ok((first, None, None));
        };

        let remainder_text = &text[delim0_offset + 1..];
        let (second, third) = remainder.sub_delimited(remainder_text, delim1)?;
        Ok((first, second, third))
    }
```

  Signature line at the top of the file's impl and `use` block: add `use crate::error::RangeError;` and `use super::check_delimiter;`.

- [ ] **Step 4: Rewrite `tree_sitter.rs`.** Same conversions, preserving the current assert order per method:
  - `split_at`: leading `assert_eq!(text.len(), self.end_byte - self.start_byte, ...)` → `check_text_length(text.len(), self.end_byte - self.start_byte)?;`; `Ok((left, right))`.
  - `shrink`: `assert_eq!(self.start_point.row, self.end_point.row, ...)` → `if self.start_point.row != self.end_point.row { return Err(RangeError::NotSingleLine); }`; `Ok(...)`.
  - `sub`: `assert!(from <= to);` → StartAfterEnd check; the text-length assert → `check_text_length(...)?;`; `Ok(...)`.
  - `sub_delimited`: text-length assert → `check_text_length(...)?;`; delimiter assert → `check_delimiter(delim)?;`; wrap in `Ok(...)`.
  - `sub_delimited_tri`: two delimiter asserts → `check_delimiter(delim0)?; check_delimiter(delim1)?;`; keep empty-text early return as `Ok((None, None, None))`; the later text-length assert → `check_text_length(...)?;`; internal `self.sub_delimited` / `remainder.sub_delimited` calls get `?`; `Ok(...)`.
  - Header: `use crate::error::RangeError;` and `use super::{check_delimiter, check_text_length};`.

- [ ] **Step 5: Mechanical pass over the three test files.** In `bytes_tests.rs`, `lsp_tests.rs`, `tree_sitter_tests.rs`: every call to a `RangeExt` method gets `.expect("valid range")` appended before use. Two shapes cover everything:

```rust
// destructuring:
let (left, right) = r(0, 10).split_at(T, 5).expect("valid range");
// inside assert_eq!:
assert_eq!(
    r(0, 7).sub_delimited("one/two", D1).expect("valid range"),
    (Some(r(0, 3)), Some(r(4, 7))),
);
```

Apply to all call sites in all three files (do not change any expected values).

- [ ] **Step 6: Verify (all three feature configurations — the tree-sitter impl only compiles with the feature)**

Run:
```bash
cargo test --lib text_utils::
cargo test --no-default-features
cargo test --all-features
cargo fmt && cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```
Expected: all green (`expect` in tests is allowed via clippy.toml; the removed `#[expect(clippy::expect_used)]` in lsp.rs no longer exists so `unfulfilled_lint_expectations` cannot fire).

- [ ] **Step 7: Report for commit**

Suggested message: `feat!: RangeExt returns Result<_, RangeError> instead of panicking (breaking: part 2 of 2)`

---

### Task 5: Tighten `position_to_encoding` parameters

**Files:**
- Modify: `src/text_utils/conversions.rs:19-24`

**Interfaces:**
- Produces: `pub fn position_to_encoding<P>(contents: &Rope, position: P, encoding_source: Encoding, encoding_target: Encoding) -> P where P: Into<Position>, P: From<Position>` (was `impl Into<Encoding>` twice).

- [ ] **Step 1: Change the signature** at `conversions.rs:19-24` and delete the two `let … = ….into();` normalization lines (29-30), using the parameters directly. Keep the `#[error]`-free body otherwise unchanged. Add the one-line invariant comment at the `unreachable!()`:

```rust
        // Internal invariant: the same-encoding case returns early above,
        // so this arm is unreachable from any external input.
        _ => unreachable!(),
```

- [ ] **Step 2: Fix call sites.** Run `grep -rn "position_to_encoding(" src/` — every call site in `src/requests/conversion.rs` and `src/tree_sitter_utils.rs` (if any) now passes `Encoding` by value; where a call passes `&encoding`, change it to `*encoding` (`Encoding` is `Copy`). The public `From<&Encoding>` impls stay (public API, out of scope).

- [ ] **Step 3: Verify**

Run: `cargo build --all-targets && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: clean; doctest unchanged (it already passes `Encoding` values).

- [ ] **Step 4: Report for commit**

Suggested message: `refactor: position_to_encoding takes Encoding by value, not impl Into<Encoding>`

---

### Task 6: Amend the error-handling rule

**Files:**
- Modify: `.claude/rules/error-handling.md` (section "The typed error", first bullet)

**Interfaces:** none (documentation).

- [ ] **Step 1: Replace the bullet** that currently reads:

```markdown
- Keep the single crate-wide enum. Do not introduce per-module error types or
  conversion hierarchies; split only if the enum grows to where matching it
  gets noisy.
```

with:

```markdown
- Keep `ServerError` as the single enum of the server/protocol domain. Leaf
  utilities without protocol semantics (such as `RangeExt` carrying
  `RangeError`) get their own narrow error type, defined in the same
  `src/error.rs` file; do not fold unrelated situations into `ServerError`
  merely to standardize the name, and do not scatter error types across
  modules.
```

- [ ] **Step 2: Verify** the rule file has no other sentence contradicting the new text (read the section top to bottom).

- [ ] **Step 3: Report for commit**

Suggested message: `docs: error-handling rule acknowledges leaf-utility error types (RangeError)`

---

### Task 7: Public-signature audit checkpoint (owner gate)

**Files:** none modified (output is a report to the owner).

**Interfaces:** none.

- [ ] **Step 1: Sweep the public surface**: `src/lib.rs` exports (`server::*`, `oneshot`, `text_utils`, `tree_sitter_utils`, `lsp_types`). For every public `fn`/method parameter, apply the single criterion: can a representable invalid state be removed, or a confusable pair separated, by a type (enum/newtype)? Use LSP `workspaceSymbol`/`findReferences`, not grep alone.

- [ ] **Step 2: Compile the findings list** — candidate, signature today, proposed type, what invalid state it removes, breaking surface. Already decided in the spec (do not re-litigate): encoded-vs-UTF-8 position separation NO; `DocumentVersion` newtype NO; `Into<Encoding>` → `Encoding` DONE (Task 5).

- [ ] **Step 3: Present the list to the owner.** Default answer for anything doubtful is NO. Nothing is implemented without explicit per-item approval.

- [ ] **Step 4: Record the outcome** in the final task report (even "no further candidates"), so Task 21's steering doc reflects it.

**Outcome (2026-08-31, owner gate passed):** 35 public items swept; one candidate approved —
`Document::query`'s `Option` conflates "no grammar/tree" with "query failed to compile".
Implemented as Task 7b below. Everything else: no candidate (nine borderline items
documented in `.superpowers/sdd/task-7-report.md`).

### Task 7b: QueryError — split Document::query failure modes (owner-approved)

**Files:**
- Modify: `src/error.rs` (feature-gated enum, after `RangeError`)
- Modify: `src/documents/document.rs` (`query` signature + body)
- Modify: `src/tree_sitter_utils.rs` (re-export)
- Modify: `examples/tree_sitter.rs` (call site)
- Modify: `src/server/state/tests.rs` (call site)

**Interfaces:**
- Produces: `Document::query(&self, query: impl AsRef<str>) -> Result<Vec<DocumentQueryCapture>, QueryError>`; `QueryError` (feature-gated) re-exported as `async_language_server::tree_sitter_utils::QueryError`.

- [ ] **Step 1: Add the enum to `src/error.rs`** after `RangeError`:

```rust
/// Failures of [`Document::query`](crate::server::Document::query).
///
/// A leaf-utility error without protocol semantics, like [`RangeError`]:
/// it never crosses the wire itself and is mapped by the caller at their
/// own boundary.
#[cfg(feature = "tree-sitter")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QueryError {
    /// The document has no tree-sitter language or parsed tree attached.
    #[error("document has no tree-sitter language or parsed tree")]
    NoTree,
    /// The query string failed to compile.
    #[error("invalid tree-sitter query")]
    InvalidQuery {
        /// The underlying compilation error.
        #[source]
        error: tree_sitter::QueryError,
    },
}
```

- [ ] **Step 2: Convert `Document::query`** in `src/documents/document.rs`:

```rust
    /// Creates and runs a query for the given query string.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError::NoTree`] when the document has no tree-sitter
    /// language or parsed tree attached, and [`QueryError::InvalidQuery`]
    /// when the query string fails to compile.
    pub fn query(&self, query: impl AsRef<str>) -> Result<Vec<DocumentQueryCapture>, QueryError> {
        let lang = self
            .tree_sitter_lang
            .as_ref()
            .ok_or(QueryError::NoTree)?;
        let tree = self
            .tree_sitter_tree
            .as_ref()
            .ok_or(QueryError::NoTree)?;

        let query = Query::new(lang, query.as_ref())
            .map_err(|error| QueryError::InvalidQuery { error })?;
```

The body after that line is unchanged except the final `Some(items)` becomes `Ok(items)`, and the `Err`-arm `tracing::warn!` + `#[cfg(not(feature = "tracing"))] drop(error);` block disappears entirely — the compile error is now returned to the caller, not swallowed into a log. Import `QueryError` via `crate::error::QueryError`.

- [ ] **Step 3: Re-export** in `src/tree_sitter_utils.rs`: `pub use crate::error::QueryError;`

- [ ] **Step 4: Adapt the two call sites.** `examples/tree_sitter.rs` (the query call around line 61): handle the `Result` in whatever shape fits the example's didactic flow (e.g. `.expect("valid query")` is acceptable in example code). `src/server/state/tests.rs` (around line 290): adapt the assertion to the `Result` shape.

- [ ] **Step 5: Verify** (the tree-sitter gate means all-features is the load-bearing config):

```bash
cargo test --all-features
cargo test --no-default-features
cargo fmt && cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

- [ ] **Step 6: Report for commit**

Suggested message: `feat!: Document::query returns Result<_, QueryError> splitting NoTree from InvalidQuery (breaking)`

---

### Task 8: Extract the serve() stack helper

**Files:**
- Modify: `src/server/serve.rs:53-80`

**Interfaces:**
- Produces: `pub(crate) async fn run_over_streams<S, R, W>(server: S, reader: R, writer: W) -> ServerResult<()> where S: Server + Clone + Send + Sync + 'static, R: futures::AsyncRead, W: futures::AsyncWrite` (bounds matching `run_buffered(self, input: impl AsyncRead, output: impl AsyncWrite)` — async-lsp lib.rs:521).

- [ ] **Step 1: Refactor `serve.rs`.** Replace the body after `into_read_write` with a delegation, adding the helper below it:

```rust
pub async fn serve<S>(transport: Transport, server: S) -> ServerResult<()>
where
    S: Server + Clone,
    S: Send + Sync + 'static,
{
    let (reader, writer) = transport.into_read_write().await?;
    run_over_streams(server, reader, writer).await
}

/// Runs the real middleware stack (lifecycle, tracing, concurrency,
/// panic catching, client-process monitor) over arbitrary byte streams.
///
/// `serve()` delegates here; the wire-tier tests (`src/server/tests.rs`)
/// drive the same stack over `tokio::io::duplex`, so the tested stack can
/// never drift from the shipped one.
pub(crate) async fn run_over_streams<S, R, W>(server: S, reader: R, writer: W) -> ServerResult<()>
where
    S: Server + Clone + Send + Sync + 'static,
    R: futures::AsyncRead,
    W: futures::AsyncWrite,
{
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let builder = ServiceBuilder::new().layer(LifecycleLayer::default());

        #[cfg(feature = "tracing")]
        let builder = builder.layer(TracingLayer::default());

        builder
            .layer(ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS))
            .layer(CatchUnwindLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(Router::from_language_server(LanguageServerWithState::new(
                client,
                server.clone(),
            )))
    });

    server
        .run_buffered(reader, writer)
        .await
        .map_err(Into::into)
}
```

Preserve the existing `serve` doc comment unchanged.

- [ ] **Step 2: Verify behavior-preservation**

Run: `cargo build --all-targets && cargo test && cargo test --no-default-features && cargo test --all-features && cargo clippy --all-targets -- -D warnings`
Expected: clean; no behavior change (pure extraction).

- [ ] **Step 3: Report for commit**

Suggested message: `refactor: extract run_over_streams from serve() for stack-faithful testing`

---

### Task 9: W2 harness + negotiation test (catalog #1)

**Files:**
- Create: `src/server/tests.rs`
- Modify: `src/server/mod.rs` (add `#[cfg(test)] mod tests;` after `mod with_state;`)
- Modify: `Cargo.toml` (add `"time"` to the existing tokio dev-dependency features — futures-rs has no timeout; this is the spec §4.4 alternative)

**Interfaces:**
- Consumes: `run_over_streams` (Task 8).
- Produces (used by Tasks 10-13): `FuturesReadHalf`/`FuturesWriteHalf` adapters, `RawClient { stream, pending }` with `write_message`, `read_message`, `send_request`, `await_response`, `request`, `notify`, `initialize_client`, and `spawn_wire_server::<S>() -> (RawClient, JoinHandle<ServerResult<()>>)`; `const WIRE_TIMEOUT: Duration = Duration::from_secs(5)`; `EchoServer`.

- [ ] **Step 1: Create `src/server/tests.rs`** with the harness:

```rust
//! Wire-tier tests: a raw JSON-RPC client speaking real `Content-Length`
//! framing over `tokio::io::duplex`, driving the actual middleware stack
//! through `run_over_streams`. The client side deliberately uses no
//! async-lsp code: it sees the exact bytes and stays isolated from
//! async-lsp client-path bugs.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, MarkedString, Position, Range as LspRange,
    TextDocumentPositionParams,
};
use futures::FutureExt as _; // `now_or_never` only; futures has no timeout
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream, ReadBuf, split};
use tokio::time::timeout;

use crate::error::ServerResult;
use crate::server::{Server, serve::run_over_streams};

const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

async fn bounded<F: std::future::Future>(future: F) -> F::Output {
    timeout(WIRE_TIMEOUT, future)
        .await
        .expect("completes within the bounded wire timeout")
}

// --- tokio → futures adapters (server side only; modeled on transport.rs) ---

struct FuturesReadHalf(tokio::io::ReadHalf<DuplexStream>);

impl futures::AsyncRead for FuturesReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut self.get_mut().0).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        }
    }
}

struct FuturesWriteHalf(tokio::io::WriteHalf<DuplexStream>);

impl futures::AsyncWrite for FuturesWriteHalf {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

// --- raw JSON-RPC client ---

struct RawClient {
    stream: DuplexStream,
    /// Server-initiated messages seen while waiting for a response.
    pending: Vec<Value>,
}

impl RawClient {
    async fn write_message(&mut self, message: &Value) {
        timeout(WIRE_TIMEOUT, async {
            let body = serde_json::to_string(message).expect("message serializes");
            // …write `Content-Length: {len}\r\n\r\n` header, then body, then flush…
        })
        .await
        .expect("writes complete within the bounded wire timeout");
    }

    /// Reads one framed message; `None` on EOF (server closed the wire).
    async fn read_message(&mut self) -> Option<Value> {
        let mut content_length = None;
        let mut line = Vec::new();
        loop {
            line.clear();
            if self.stream.read_until(b'\n', &mut line).await.expect("header reads") == 0 {
                return None; // EOF
            }
            let trimmed = trim_crlf(&line);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
                content_length = Some(value.parse::<usize>().expect("length parses"));
            }
        }
        let len = content_length.expect("Content-Length header present");
        let mut body = vec![0u8; len];
        self.stream.read_exact(&mut body).await.expect("body reads");
        Some(serde_json::from_slice(&body).expect("body is JSON"))
    }

    fn send_request(&mut self, id: i64, method: &str, params: Value) -> impl Future<Output = ()> + '_ {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.write_message(&message)
    }

    async fn await_response(&mut self, id: i64) -> Value {
        loop {
            let message = self
                .read_message()
                .await
                .expect("connection stays open until the response arrives");
            if message.get("id").and_then(Value::as_i64) == Some(id)
                && (message.get("result").is_some() || message.get("error").is_some())
            {
                return message;
            }
            self.pending.push(message);
        }
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send_request(id, method, params).await;
        self.await_response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_message(&message).await;
    }

    /// Full initialize handshake; returns the `InitializeResult`.
    async fn initialize_client(&mut self, encodings: &[&str]) -> Value {
        let response = self
            .request(
                1,
                "initialize",
                json!({
                    "processId": null,
                    "capabilities": {
                        "general": { "positionEncodings": encodings }
                    }
                }),
            )
            .await;
        self.notify("initialized", json!({})).await;
        response["result"].clone()
    }
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

// --- test servers ---

#[derive(Clone)]
struct EchoServer;

fn echo_hover(position: Position) -> Option<Hover> {
    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String("echo".into())),
        range: Some(LspRange::new(position, position)),
    })
}

impl Server for EchoServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        let position = params.text_document_position_params.position;
        async move { Ok(echo_hover(position)) }
    }
}

// --- wiring ---

async fn spawn_wire_server<S>(server: S) -> (RawClient, tokio::task::JoinHandle<ServerResult<()>>)
where
    S: Server + Clone + Send + Sync + 'static,
{
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    let (server_read, server_write) = split(server_stream);
    let handle = tokio::spawn(run_over_streams(
        server,
        FuturesReadHalf(server_read),
        FuturesWriteHalf(server_write),
    ));
    (
        RawClient {
            stream: client_stream,
            pending: Vec::new(),
        },
        handle,
    )
}

// --- catalog #1 ---

#[tokio::test]
async fn initialize_negotiates_position_encoding_end_to_end() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    // The client prefers utf-16, but also offers utf-8: the server's
    // preference order must pick utf-8 through the real JSON round trip.
    let result = client.initialize_client(&["utf-16", "utf-8"]).await;

    assert_eq!(result["capabilities"]["positionEncoding"], "utf-8");

    drop(client);
    let _ = bounded(server).await;
}
```

**Harness hardening (review finding, fixed in-round):** the landed `RawClient` holds a buffered read half + write half (`read_until` needs `AsyncBufRead`), and `write_message`/`read_message` wrap their bodies in `timeout(WIRE_TIMEOUT, …)` — client I/O is bounded BY CONSTRUCTION, so every test in Tasks 10-13 inherits the guarantee; EOF is a normal completion inside the bound, only a silent hang panics.

- [ ] **Step 2: Wire the module** — in `src/server/mod.rs`, after `mod with_state;` add:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 3: Verify**

Run: `cargo test --lib server::tests`
Expected: 1 passed. If the test hangs, the bounded timeout fails it within 5 s — investigate the framing, never remove the timeout.
Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --no-default-features --lib server::tests`
Expected: clean in both configurations (harness uses no feature-gated API).

- [ ] **Step 4: Report for commit**

Suggested message: `test: W2 wire harness (raw JSON-RPC client over duplex) + end-to-end encoding negotiation`

---

### Task 10: W2 conversion & dispatch tests (catalog #2, #3, #4, #5, #6)

**Files:**
- Modify: `src/server/tests.rs` (append)

**Interfaces:**
- Consumes: the Task 9 harness (`RawClient`, `spawn_wire_server`, `EchoServer`, `bounded`, `WIRE_TIMEOUT`).
- Consumes: `TextDocumentItem`/`DidOpenTextDocumentParams` JSON shape for raw `textDocument/didOpen` notifications.

- [ ] **Step 1: Append the tests**

```rust
fn did_open(uri: &str, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": "test",
            "version": 1,
            "text": text
        }
    })
}

#[tokio::test]
async fn utf16_positions_round_trip_through_real_serialization() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // "a🙂b": the smiley is 1 UTF-16 unit pair (cols 1-2), so 'b' sits at
    // UTF-16 col 3 but UTF-8 byte 5. The handler must see UTF-8 and the
    // wire response must come back as UTF-16.
    client
        .notify("textDocument/didOpen", did_open("file:///tmp/wire.txt", "a🙂b"))
        .await;

    let response = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 3 }
            }),
        )
        .await;

    assert_eq!(response["result"]["range"]["start"]["character"], 3);
    assert_eq!(response["result"]["range"]["end"]["character"], 3);

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn requests_before_initialize_are_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    let response = client
        .request(
            1,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;

    assert_eq!(response["error"]["code"], -32002); // ServerNotInitialized

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn double_initialize_is_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    let response = client
        .request(
            2,
            "initialize",
            json!({"processId": null, "capabilities": {}}),
        )
        .await;

    assert_eq!(response["error"]["code"], -32600); // InvalidRequest

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn requests_after_shutdown_are_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;
    let shutdown = client.request(2, "shutdown", json!(null)).await;
    assert!(shutdown.get("result").is_some());

    let response = client
        .request(
            3,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;

    assert_eq!(response["error"]["code"], -32600); // InvalidRequest

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn unwired_methods_return_method_not_found() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // One parametrized test over the future surface: methods the crate
    // does not wire must answer -32601 no matter how many are added.
    for (id, method) in ["textDocument/documentSymbol", "workspace/symbol", "textDocument/inlayHint"]
        .into_iter()
        .enumerate()
    {
        let response = client
            .request(
                i64::try_from(id).expect("small id") + 10,
                method,
                json!({}),
            )
            .await;
        assert_eq!(response["error"]["code"], -32601, "method {method}");
    }

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn incremental_did_change_applies_over_the_wire() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    client
        .notify("textDocument/didOpen", did_open("file:///tmp/wire.txt", "a🙂b"))
        .await;
    // Insert "xy" at UTF-16 col 3 (before 'b'): text becomes "a🙂xyb".
    client
        .notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt", "version": 2 },
                "contentChanges": [
                    { "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } }, "text": "xy" }
                ]
            }),
        )
        .await;

    // Hover at the new end of line: UTF-16 col 6 must exist in the edited
    // document and round-trip back as col 6.
    let response = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 6 }
            }),
        )
        .await;

    assert_eq!(response["result"]["range"]["start"]["character"], 6);

    drop(client);
    let _ = bounded(server).await;
}
```

Note: `send_request` returns a future borrowing `self`; call and await it immediately (`client.request(...)` already does both). If `notify`/`request` signatures fight the borrow checker, adjust the harness internals, not the tests.

**Landed nuance (Task 10 review outcome):** the parametrized `-32601` test sends MINIMALLY VALID params per method — async-lsp's `Router::from_language_server` registers every omni method and deserializes params BEFORE dispatch, so empty `{}` answers `-32602` (InvalidParams) and never reaches our `METHOD_NOT_FOUND` default. Expected codes unchanged; recorded for the Task 21 steering doc.

- [ ] **Step 2: Verify**

Run: `cargo test --lib server::tests`
Expected: 7 passed (Task 9's + 6 new).

- [ ] **Step 3: Report for commit**

Suggested message: `test: W2 conversion round-trip, lifecycle gating, dispatch, incremental didChange`

---

### Task 11: W2 gated tests (catalog #7, #8, #9)

**Files:**
- Modify: `src/server/tests.rs` (append)

**Interfaces:**
- Consumes: the Task 9 harness.
- Produces: `GatedServer { entered: UnboundedSender<u64>, release: watch::Receiver<bool> }` and `PanickingServer`.

- [ ] **Step 1: Append the gated servers**

```rust
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

#[derive(Clone)]
struct GatedServer {
    entered: mpsc::UnboundedSender<u64>,
    release: watch::Receiver<bool>,
}

impl Server for GatedServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        let entered = self.entered.clone();
        let mut release = self.release.clone();
        async move {
            let _ = entered.send(1);
            while !*release.borrow_and_update() {
                release.changed().await.expect("release channel stays alive");
            }
            Ok(echo_hover(params.text_document_position_params.position))
        }
    }
}

#[derive(Clone)]
struct PanickingServer;

impl Server for PanickingServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        _params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        async move {
            // Trigger the panic through `expect` (allowed in tests) rather
            // than `panic!`, which `panic_in_result_fn` rejects in a
            // Result-returning function even under -D warnings.
            let nothing: Option<Hover> = None;
            Ok(nothing.expect("intentional test panic"))
        }
    }
}
```

- [ ] **Step 2: Append the tests**

```rust
fn hover_params(uri: &str, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": character }
    })
}

#[tokio::test]
async fn stale_document_answers_content_modified_then_succeeds_on_retry() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;
    client
        .notify("textDocument/didOpen", did_open("file:///tmp/wire.txt", "a🙂b"))
        .await;

    // Fire hover but do not await it yet: the handler is gated inside.
    client
        .send_request(2, "textDocument/hover", hover_params("file:///tmp/wire.txt", 1))
        .await;
    entered_rx.recv().now_or_never().expect("handler entered");

    // Mutate the document while the handler is in flight.
    client
        .notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt", "version": 2 },
                "contentChanges": [{ "text": "changed" }]
            }),
        )
        .await;
    release_tx.send(true).expect("release sends");

    let stale = client.await_response(2).await;
    assert_eq!(stale["error"]["code"], -32801); // ContentModified

    // The retry against the new version succeeds.
    let retried = client
        .request(3, "textDocument/hover", hover_params("file:///tmp/wire.txt", 1))
        .await;
    assert!(retried.get("result").is_some());

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn panicking_handler_returns_structured_error() {
    let (mut client, server) = spawn_wire_server(PanickingServer);
    client.initialize_client(&["utf-16"]).await;

    let response = client
        .request(2, "textDocument/hover", hover_params("file:///tmp/wire.txt", 0))
        .await;

    let error = response["error"].as_object().expect("error, not a hang");
    assert!(
        error["message"].as_str().expect("message is a string").contains("panicked"),
        "message was: {error:?}"
    );

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn at_most_eight_requests_run_concurrently() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;

    for id in 10..19 {
        client
            .send_request(id, "textDocument/hover", hover_params("file:///tmp/wire.txt", 0))
            .await;
    }

    for _ in 0..8 {
        timeout(WIRE_TIMEOUT, entered_rx.recv())
            .await
            .expect("eight handlers enter")
            .expect("signal received");
    }
    // The only bounded absence-check in the suite: nothing enters while all
    // eight permits are held.
    timeout(Duration::from_millis(250), entered_rx.recv())
        .await
        .expect_err("the ninth handler must wait for a permit");

    release_tx.send(true).expect("release sends");

    // TRIPWIRE (owner decision 2026-08-31): upstream deadlock — with
    // ConcurrencyLayer at capacity, async-lsp 0.2.4's MainLoop stops polling
    // in-flight tasks while waiting for poll_ready (the inner dispatch select
    // polls only poll_ready + flush), so the gated futures are never polled
    // and the permits never release. https://github.com/oxalica/async-lsp/pull/30
    // The ninth handler must STILL not enter after release.
    timeout(Duration::from_millis(250), entered_rx.recv())
        .await
        .expect_err("the ninth handler stays wedged (upstream async-lsp#30)");

    // The join handle can never complete: abort it. When this test starts
    // failing after an async-lsp upgrade, the fix landed — flip the second
    // absence-check to asserting the ninth handler enters and completes.
    server.abort();
}
```

(`timeout` is `tokio::time::timeout`, imported by the Task 9 harness; `now_or_never` — used in the staleness test — is a real `futures::FutureExt` method, hence that import stays.)

- [ ] **Step 3: Verify**

Run: `cargo test --lib server::tests`
Expected: 10 passed. If the staleness test reports the hover succeeding instead of `-32801`, investigate the version snapshot logic (`implement_method!` step 4a) — that is the code under test, not a test bug to route around.

**Upstream note (verified from vendored source + PR #30):** the concurrency test's second half is a TRIPWIRE, not a behavioral expectation — async-lsp 0.2.4's `MainLoop` deadlocks at `ConcurrencyLayer` capacity (never polls in-flight tasks during `poll_ready` wait; stops reading the wire). Cleanup aborts the wedged join handle. After the upstream fix merges and we upgrade, flip the second absence-check to assert recovery.

- [ ] **Step 4: Report for commit**

Suggested message: `test: W2 staleness retry, panic-to-error, concurrency bound`

---

### Task 12: W2 shutdown + server→client tests (catalog #10, #11)

**Files:**
- Modify: `src/server/tests.rs` (append)

**Interfaces:**
- Consumes: the Task 9 harness; `Server::server_options` override for the configurable-diagnostics server; `WorkspaceDiagnostics`, `ServerOptions` from `crate::server`.

- [ ] **Step 1: Append the configurable server** (sends `workspace/configuration` mid-request):

```rust
#[derive(Clone)]
struct ConfigurableServer;

impl Server for ConfigurableServer {
    fn server_options(&self) -> crate::server::ServerOptions {
        crate::server::ServerOptions::default().with_workspace_diagnostics(
            crate::server::WorkspaceDiagnostics::setting("wireTest"),
        )
    }
}
```

- [ ] **Step 2: Append the tests**

```rust
#[tokio::test]
async fn shutdown_exit_terminates_the_server_loop_cleanly() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    let shutdown = client.request(2, "shutdown", json!(null)).await;
    assert!(shutdown.get("result").is_some());
    client.notify("exit", json!(null)).await;

    bounded(server).await.expect("serve loop resolves Ok(())");

    // EOF is the expected termination, not a hang: read until close.
    // (First extend RawClient with a small `read_to_end` helper that drains
    // its buffered reader — however the Task 9 harness named that field.)
    let mut raw = Vec::new();
    timeout(WIRE_TIMEOUT, client.read_to_end(&mut raw))
        .await
        .expect("server closes the wire")
        .expect("read succeeds");
    assert!(raw.is_empty(), "no trailing bytes after exit");
}
```

```rust
#[tokio::test]
async fn workspace_configuration_request_is_served_mid_request() {
    let root = {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-wire-config-{millis}"));
        std::fs::create_dir_all(&root).expect("temp workspace can be created");
        root
    };

    let (mut client, server) = spawn_wire_server(ConfigurableServer);

    // initialize with configuration capability + a workspace folder
    let response = client
        .request(
            1,
            "initialize",
            json!({
                "processId": null,
                "capabilities": { "workspace": { "configuration": true } },
                "workspaceFolders": [{ "uri": format!("file://{}", root.display()), "name": "root" }]
            }),
        )
        .await;
    assert!(response.get("result").is_some(), "initialize succeeds: {response}");
    client.notify("initialized", json!({})).await;

    client
        .send_request(2, "workspace/diagnostic", json!({ "previousResultIds": [] }))
        .await;

    // The server asks for its setting mid-flight; answer from the raw side.
    let configuration_request = loop {
        let message = timeout(WIRE_TIMEOUT, client.read_message())
            .await
            .expect("configuration request arrives")
            .expect("wire stays open");
        if message.get("method").is_some_and(Value::is_string) {
            break message;
        }
        client.pending.push(message);
    };
    assert_eq!(configuration_request["method"], "workspace/configuration");
    let request_id = configuration_request["id"].clone();
    client
        .write_message(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": [true]
        }))
        .await;

    let report = client.await_response(2).await;
    assert!(report.get("result").is_some(), "diagnostics report returns: {report}");

    drop(client);
    let _ = bounded(server).await;
    std::fs::remove_dir_all(root).expect("temp workspace can be removed");
}
```

Scope note (verified at plan time): the crate's only server→client request is `workspace/configuration` (sent from `src/workspace/diagnostics.rs:279-331`); it never sends `client/registerCapability` (`grep -ri "registerCapability" src/` is empty), so that half of catalog #11 has no trigger without new production code and is intentionally not faked.

**Landed deviations (review-adjudicated):** (1) `ConfigurableServer` also overrides `server_capabilities` to advertise a diagnostic provider — without it the wrapper answers `-32601 "workspace diagnostics are disabled"` on the wire and the report assertion is unsatisfiable; (2) `await_response` checks `pending` first — wire ordering is report-THEN-configuration on the current runtime, so both orders are handled; (3) the shutdown test's `expect` is split so the inner `Ok(())` is actually asserted (`unused_must_use`); (4) fixture filesystem calls use `tokio::fs` with the `"fs"` tokio dev-dep feature (same pattern as `time`/`sync`) instead of arch-lint `no-sync-io` comment allows — root fix, no suppression.

- [ ] **Step 3: Verify**

Run: `cargo test --lib server::tests`
Expected: 12 passed.

- [ ] **Step 4: Report for commit**

Suggested message: `test: W2 clean shutdown/exit termination and mid-request workspace/configuration`

---

### Task 13: W2 auxiliary tests (catalog #14, #15)

**Files:**
- Modify: `src/server/tests.rs` (append)

**Interfaces:** Consumes the Task 9/11 harness (`GatedServer`).

- [ ] **Step 1: Append the tests**

```rust
#[tokio::test]
async fn cancel_request_answers_request_cancelled() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;

    client
        .send_request(2, "textDocument/hover", hover_params("file:///tmp/wire.txt", 0))
        .await;
    entered_rx.recv().now_or_never().expect("handler entered");

    // Cancel while the handler is still gated, then release.
    client
        .notify("$/cancelRequest", json!({ "id": 2 }))
        .await;
    release_tx.send(true).expect("release sends");

    let response = client.await_response(2).await;
    assert_eq!(response["error"]["code"], -32800); // RequestCancelled

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn malformed_header_closes_the_connection() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    client
        .stream
        .write_all(b"Content-Length: abc\r\n\r\n")
        .await
        .expect("garbage writes");

    // The loop fails on framing and closes: EOF within the bound.
    let closed = timeout(WIRE_TIMEOUT, client.read_message())
        .await
        .expect("server reacts within the bound");
    assert!(closed.is_none(), "expected EOF, got {closed:?}");

    let outcome = bounded(server).await;
    assert!(outcome.is_err(), "the loop must fail, not exit Ok: {outcome:?}");
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --lib server::tests`
Expected: 14 passed (all W2 tests; catalog #1-#11, #14, #15).

- [ ] **Step 3: Report for commit**

Suggested message: `test: W2 $/cancelRequest and framing-robustness smoke`

---

### Task 14: W3 black-box TCP tests (catalog #12, #13)

**Files:**
- Create: `tests/lsp_wire.rs`

**Interfaces:**
- Consumes: public `serve`, `Transport`, `Server`, `ServerError`, `ServerResult` from `async_language_server::server`.

- [ ] **Step 1: Create `tests/lsp_wire.rs`**

Integration tests cannot reach the lib's `#[cfg(test)]` modules, so this file carries its own minimal raw client (~40 lines, framing only) — accepted duplication, smaller than a shared test-support crate.

```rust
//! Black-box tests through the only general public entry point: `serve()`
//! over a real TCP socket (`Transport::Socket`).

use std::time::Duration;

use async_language_server::server::{Server, ServerError, ServerResult, Transport, serve};
use async_lsp::lsp_types::{Hover, HoverContents, HoverParams, MarkedString, Position, Range};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, TcpStream};
use tokio::time::timeout;

const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct EchoServer;

impl Server for EchoServer {
    fn hover(
        &self,
        _state: async_language_server::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
        let position = params.text_document_position_params.position;
        async move {
            Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String("echo".into())),
                range: Some(Range::new(position, position)),
            }))
        }
    }
}

struct RawClient {
    stream: TcpStream,
}

impl RawClient {
    async fn write_message(&mut self, message: &Value) {
        let body = serde_json::to_string(message).expect("serializes");
        self.stream
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .await
            .expect("header writes");
        self.stream
            .write_all(body.as_bytes())
            .await
            .expect("body writes");
        self.stream.flush().await.expect("flushes");
    }

    async fn read_message(&mut self) -> Option<Value> {
        let mut content_length = None;
        let mut line = Vec::new();
        loop {
            line.clear();
            if self.stream.read_until(b'\n', &mut line).await.expect("reads") == 0 {
                return None;
            }
            let trimmed = trim_crlf(&line);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = std::str::from_utf8(trimmed)
                .expect("header is ASCII")
                .strip_prefix("Content-Length: ")
            {
                content_length = Some(value.trim().parse::<usize>().expect("length parses"));
            }
        }
        let mut body = vec![0u8; content_length.expect("header present")];
        self.stream.read_exact(&mut body).await.expect("body reads");
        Some(serde_json::from_slice(&body).expect("json"))
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.write_message(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        loop {
            let message = timeout(WIRE_TIMEOUT, self.read_message())
                .await
                .expect("responds in time")
                .expect("connection open");
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
    }
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

#[tokio::test]
async fn socket_connect_failure_maps_to_tcp_connect_error() {
    // Bind, learn the port, drop the listener: nothing listens there.
    // (A rare ephemeral-port reuse race remains; if it flakes, re-run once
    // before investigating — it is not a code defect.)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let error = timeout(WIRE_TIMEOUT, serve(Transport::Socket(port), EchoServer))
        .await
        .expect("fails within the bound")
        .expect_err("connect fails");
    assert!(
        matches!(error, ServerError::TcpConnect { port: p, .. } if p == port),
        "was: {error:?}"
    );
}

#[tokio::test]
async fn serve_happy_path_over_tcp_resolves_ok() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let serve_handle = tokio::spawn(serve(Transport::Socket(port), EchoServer));
    let (stream, _) = listener.accept().await.expect("server connects");
    let mut client = RawClient { stream };

    let initialize = client
        .request(
            1,
            "initialize",
            json!({"processId": null, "capabilities": {}}),
        )
        .await;
    assert!(initialize.get("result").is_some(), "{initialize}");

    let hover = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/tcp.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;
    assert!(hover.get("result").is_some(), "{hover}");

    let shutdown = client.request(3, "shutdown", json!(null)).await;
    assert!(shutdown.get("result").is_some());
    client
        .write_message(&json!({"jsonrpc": "2.0", "method": "exit"}))
        .await;

    serve_handle
        .await
        .expect("task joins")
        .expect("serve() resolves Ok(())");
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --test lsp_wire`
Expected: 2 passed. Then the full battery in all three configurations.

- [ ] **Step 3: Report for commit**

Suggested message: `test: W3 black-box TCP tier — connect-failure mapping and serve() happy path`

---

### Task 15: UTF-32 conversion arms + preference order (gap 2)

**Files:**
- Modify: `src/text_utils/conversions.rs` (tests module)
- Modify: `src/server/with_state/tests.rs` (append two tests, reusing its `temp_workspace` and `initialize_params` helpers)

**Interfaces:** none (tests only).

- [ ] **Step 1: Append to `conversions.rs` tests**

```rust
    #[test]
    fn converts_utf8_columns_to_utf32() {
        let text = Rope::from_str("a🙂b");
        let position = Position { line: 0, col: 5 };

        let converted = position_to_encoding(&text, position, Encoding::UTF8, Encoding::UTF32);

        assert_eq!(converted, Position { line: 0, col: 2 });
    }

    #[test]
    fn converts_utf32_columns_to_utf8() {
        let text = Rope::from_str("a🙂b");
        let position = Position { line: 0, col: 2 };

        let converted = position_to_encoding(&text, position, Encoding::UTF32, Encoding::UTF8);

        assert_eq!(converted, Position { line: 0, col: 5 });
    }

    #[test]
    fn converts_utf16_columns_to_utf32() {
        let text = Rope::from_str("a🙂b");
        let position = Position { line: 0, col: 3 };

        let converted = position_to_encoding(&text, position, Encoding::UTF16, Encoding::UTF32);

        assert_eq!(converted, Position { line: 0, col: 2 });
    }

    #[test]
    fn converts_utf32_columns_to_utf16() {
        let text = Rope::from_str("a🙂b");
        let position = Position { line: 0, col: 2 };

        let converted = position_to_encoding(&text, position, Encoding::UTF32, Encoding::UTF16);

        assert_eq!(converted, Position { line: 0, col: 3 });
    }

    #[test]
    fn caps_lines_before_converting_utf32_columns() {
        let text = Rope::from_str("first\n🙂");
        // Col 4 is the boundary AFTER the 4-byte emoji (col 1 would be
        // mid-codepoint and ropey clamps it to 0 — the expected value must
        // use a whole-character boundary, mirroring the UTF-16 sibling).
        let position = Position { line: 99, col: 4 };

        let converted = position_to_encoding(&text, position, Encoding::UTF8, Encoding::UTF32);

        assert_eq!(converted, Position { line: 1, col: 1 });
    }
```

- [ ] **Step 2: Append preference-order tests to `src/server/with_state/tests.rs`**, modeled on the existing `initialize_ignores_unknown_client_encodings` (that test's shape is the template — same helpers, same `GeneralClientCapabilities` block):

```rust
#[test]
fn initialize_prefers_utf8_when_the_client_offers_it() {
    let root = temp_workspace("prefer-utf8");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![PositionEncodingKind::UTF16, PositionEncodingKind::UTF8]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF8)
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn initialize_prefers_utf32_over_utf16() {
    let root = temp_workspace("prefer-utf32");
    let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

    let mut params = initialize_params(&root);
    params.capabilities.general = Some(GeneralClientCapabilities {
        position_encodings: Some(vec![PositionEncodingKind::UTF16, PositionEncodingKind::UTF32]),
        ..Default::default()
    });

    let result =
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

    assert_eq!(
        result.capabilities.position_encoding,
        Some(PositionEncodingKind::UTF32)
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
```

(Match the file's existing import style — `PositionEncodingKind` and `GeneralClientCapabilities` are already imported there for the sibling test.)

- [ ] **Step 3: Verify**

Run: `cargo test --lib text_utils::conversions && cargo test --lib with_state`
Expected: 5 + 2 new tests pass.

- [ ] **Step 4: Report for commit**

Suggested message: `test: UTF-32 conversion arms and full preference-order coverage`

---

### Task 16: `From<ServerError>` arms (gap 3)

**Files:**
- Modify: `src/error.rs` (tests module)

**Interfaces:** none.

- [ ] **Step 1: Append to the tests module**

```rust
    #[test]
    fn lsp_errors_map_to_internal_error() {
        let response = ResponseError::from(ServerError::Lsp(async_lsp::Error::Eof));

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
    }

    #[test]
    fn invalid_file_path_maps_to_internal_error() {
        let response = ResponseError::from(ServerError::InvalidFilePath {
            path: std::path::PathBuf::from("/bad"),
        });

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "invalid file path '/bad'");
    }

    #[test]
    fn tcp_connect_maps_to_internal_error() {
        let response = ResponseError::from(ServerError::TcpConnect {
            port: 9999,
            error: std::io::Error::other("connection refused"),
        });

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "failed to connect to port 9999");
    }
```

- [ ] **Step 2: Verify**

Run: `cargo test --lib error::`
Expected: 8 passed (5 existing + 3 new). If `async_lsp::Error::Eof` is not a unit variant in 0.2.4, pick any constructible variant via `async_lsp::Error::` autocompletion/LSP hover — do not change the assertion.

- [ ] **Step 3: Report for commit**

Suggested message: `test: all From<ServerError> wire-mapping arms`

---

### Task 17: `handle_document_save` (gap 4)

**Files:**
- Modify: `src/server/state/tests.rs` (append; reuse its `temp_workspace` helper and import style)

**Interfaces:** none.

- [ ] **Step 1: Append the tests** (match the file's existing imports; `DidSaveTextDocumentParams`, `TextDocumentIdentifier`, `Url`, `fs` are used by siblings)

```rust
#[test]
fn document_save_replaces_text_from_params() {
    let root = temp_workspace("save-from-params");
    let uri = {
        let path = root.join("saved.txt");
        Url::from_file_path(path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: Some("after".into()),
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), "after");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_falls_back_to_disk_when_params_have_no_text() {
    let root = temp_workspace("save-from-disk");
    let path = root.join("on-disk.txt");
    fs::write(&path, "from disk").expect("file can be written");
    let uri = Url::from_file_path(&path).expect("path converts to a URL");
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), "from disk");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_removes_the_document_when_no_text_and_no_file() {
    let root = temp_workspace("save-removes");
    let uri = {
        let path = root.join("missing.txt");
        Url::from_file_path(path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    });

    assert!(state.document(&uri).is_none(), "document is removed on failure");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --lib state::tests`
Expected: pass (8 existing + 3 new). If `TestServer` is named differently in that file, use its local name.

- [ ] **Step 3: Report for commit**

Suggested message: `test: handle_document_save replacement, disk fallback, remove-on-failure`

---

### Task 18: `DocumentMatcher` semantics (gap 5)

**Files:**
- Modify: `src/documents/matcher.rs` (append `#[cfg(test)] mod tests`)

**Interfaces:** none.

- [ ] **Step 1: Append the test module** (uses a real temp dir because URL-glob matching goes through `Url::to_file_path`):

```rust
#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use async_lsp::lsp_types::Url;

    use super::{DocumentMatcher, DocumentMatchers};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-matcher-{name}-{millis}"));
        fs::create_dir_all(&root).expect("temp dir can be created");
        root
    }

    #[test]
    fn find_matches_language_strings_case_insensitively() {
        let matchers = DocumentMatchers::new([DocumentMatcher::new("json").with_lang_strings(["Json"])]);

        let found = matchers
            .find(&Url::parse("file:///tmp/any.txt").unwrap(), "JSON")
            .expect("matched by language");
        assert_eq!(found.name(), "json");
    }

    #[test]
    fn find_matches_url_globs_against_real_paths() {
        let root = temp_dir("url-glob");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        let matchers =
            DocumentMatchers::new([DocumentMatcher::new("json").with_url_globs(["**/*.json"])]);

        let found = matchers
            .find(&uri, "plaintext")
            .expect("matched by glob when the language is unknown");
        assert_eq!(found.name(), "json");

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[test]
    fn language_strings_win_over_url_globs() {
        let root = temp_dir("precedence");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        let matchers = DocumentMatchers::new([
            DocumentMatcher::new("by-lang").with_lang_strings(["json"]),
            DocumentMatcher::new("by-glob").with_url_globs(["**/*.json"]),
        ]);

        let found = matchers.find(&uri, "json").expect("matched");
        assert_eq!(found.name(), "by-lang");

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[test]
    fn invalid_globs_are_skipped_not_matched() {
        let root = temp_dir("invalid-glob");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        // "[" is not a valid glob: the matcher contributes nothing, and the
        // document simply stays unmatched — the return half of the warn path.
        let matchers =
            DocumentMatchers::new([DocumentMatcher::new("broken").with_url_globs(["["])]);

        assert!(matchers.find(&uri, "plaintext").is_none());

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn lang_grammar_rides_along_with_the_matcher() {
        let matcher = DocumentMatcher::new("json")
            .with_lang_grammar(tree_sitter_json::LANGUAGE.into());

        assert!(matcher.lang_grammar().is_some());
        assert_eq!(
            DocumentMatcher::new("bare").lang_grammar(),
            None,
            "the getter is pub(crate); default is no grammar"
        );
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --lib documents::matcher && cargo test --all-features --lib documents::matcher`
Expected: 4 passed without the feature, 5 with it.

- [ ] **Step 3: Report for commit**

Suggested message: `test: DocumentMatcher find semantics, precedence, invalid-glob skip, grammar`

---

### Task 19: `Document::query` failure (gap 6)

**Files:**
- Modify: `src/documents/document.rs` (append to its tests module)

**Interfaces:** Consumes the `JsonServer`-style state wiring pattern proven at `src/server/state/tests.rs:236-249` and `QueryError` from Task 7b (`crate::error::QueryError`).

- [ ] **Step 1: Append to the tests module in `document.rs`** — construct documents through `ServerState` with a grammar-carrying matcher (the only in-crate way to get a parsed `Document`):

```rust
    #[cfg(feature = "tree-sitter")]
    #[test]
    fn query_errors_on_invalid_query_and_grammarless_documents() {
        use std::time::{SystemTime, UNIX_EPOCH};

        use async_lsp::{
            ClientSocket,
            lsp_types::{DidOpenTextDocumentParams, TextDocumentItem, Url},
        };

        use crate::server::{DocumentMatcher, Server, ServerOptions, ServerState};

        struct JsonServer;

        impl Server for JsonServer {
            fn server_document_matchers() -> Vec<DocumentMatcher> {
                vec![DocumentMatcher::new("json").with_lang_strings(["json"])
                    .with_lang_grammar(tree_sitter_json::LANGUAGE.into())]
            }
        }

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-query-{millis}"));
        std::fs::create_dir_all(&root).expect("temp workspace can be created");
        let uri = Url::from_file_path(root.join("doc.json")).expect("path converts to a URL");

        let mut state = ServerState::with_options::<JsonServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );
        let _ = state.handle_document_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "json".into(), 1, r#"{"a": 1}"#.into()),
        });
        let document = state.document(&uri).expect("document is tracked");

        // Malformed query syntax: the typed compile failure, not a bare None.
        assert!(matches!(
            document.query("(node"),
            Err(QueryError::InvalidQuery { .. })
        ));

        // A document with no grammar/tree answers NoTree, distinctly:
        // same state, different URL, language string no matcher claims.
        let plain_uri = Url::from_file_path(root.join("plain.txt")).expect("path converts");
        let _ = state.handle_document_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(plain_uri.clone(), "plaintext".into(), 1, "x".into()),
        });
        let plain = state.document(&plain_uri).expect("document is tracked");
        assert!(matches!(plain.query("(node"), Err(QueryError::NoTree)));

        std::fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
```

- [ ] **Step 2: Verify**

Run: `cargo test --all-features --lib documents::document`
Expected: 2 passed (existing reader test + new). Also `cargo test --no-default-features` must compile clean (test is feature-gated).

- [ ] **Step 3: Report for commit**

Suggested message: `test: Document::query returns typed errors for invalid queries and grammar-less documents`

---

### Task 20: `split_off_*` boundaries + `RangeError` triggers (gaps 8-follow-up, 10)

**Files:**
- Modify: `src/text_utils/range_ext/bytes_tests.rs`, `lsp_tests.rs`, `tree_sitter_tests.rs`

**Interfaces:** Consumes `RangeError` (Task 3) and the fallible API (Task 4).

- [ ] **Step 1: Append to `bytes_tests.rs`** (add `use crate::server::RangeError;` — or `crate::text_utils::RangeError`, whichever the file's style favors):

```rust
#[test]
fn split_off_boundaries() {
    let range = r(5, 15);
    assert_eq!(range.split_off_left(T, 0).expect("valid range"), r(5, 5));
    assert_eq!(range.split_off_right(T, 10).expect("valid range"), r(15, 15));
}

#[test]
fn out_of_range_positions_return_position_out_of_range() {
    assert_eq!(
        r(0, 10).split_at(T, 11).unwrap_err(),
        RangeError::PositionOutOfRange
    );
    assert_eq!(
        r(0, 10).sub(T, 3, 11).unwrap_err(),
        RangeError::PositionOutOfRange
    );
}

#[test]
fn reversed_sub_positions_return_start_after_end() {
    assert_eq!(r(0, 10).sub(T, 7, 3).unwrap_err(), RangeError::StartAfterEnd);
}

#[test]
fn multi_byte_delimiters_return_delimiter_not_single_byte() {
    assert_eq!(
        r(0, 7).sub_delimited("one—two", '—').unwrap_err(),
        RangeError::DelimiterNotSingleByte { delimiter: '—' }
    );
}

#[test]
fn mismatched_text_length_returns_text_range_mismatch() {
    assert_eq!(
        r(0, 7).sub_delimited("short", '/').unwrap_err(),
        RangeError::TextRangeMismatch {
            text_len: 5,
            range_len: 7
        }
    );
}
```

- [ ] **Step 2: Append the type-specific twins** to `lsp_tests.rs` and `tree_sitter_tests.rs`: the same five test names with `—`-suffix-free names already used there (`_lsp` / `_ts` conventions per the file's existing naming), using each file's local `r()`/position fixtures and a multi-line text (`"a\nb"`) for the shrink case those two types have and bytes lack:

```rust
#[test]
fn shrink_requires_a_single_line_range() {
    let multiline = Range {
        start: p(0, 0),
        end: p(1, 0),
    };
    assert_eq!(
        multiline.shrink(1, 1).unwrap_err(),
        RangeError::NotSingleLine
    );
}
```

(Use each file's actual `p`/`r` helpers; for tree_sitter build the `TsRange`/`TsPosition` fixtures the file already uses. In `tree_sitter` the same test asserts `RangeError::NotSingleLine` via `start_point.row != end_point.row`.)

- [ ] **Step 3: Verify**

Run: `cargo test --lib text_utils:: && cargo test --all-features --lib text_utils::`
Expected: pass; each of the five `RangeError` variants is now triggered somewhere.

- [ ] **Step 4: Report for commit**

Suggested message: `test: split_off boundaries and every RangeError variant trigger`

---

### Task 21: Testing steering document

**Files:**
- Create: `.claude/rules/testing.md`

**Interfaces:** none (documentation; consume the Task 7 audit outcome).

- [ ] **Step 1: Write the steering rule**

```markdown
# Testing

This rule is normative for all test work in this crate. It documents the
pipeline built in the 2026-08 testing cycle (spec:
`docs/superpowers/specs/2026-08-31-testing-implementation-design.md`).

## Philosophy

Type first, test second. Before writing a test, ask whether a type can
remove the invalid state the test would pin (`RangeError` replaced eight
`# Panics` contracts). A test exists only for behavior no type can
express; there are no tests for quantity or coverage statistics. The
typing criterion: remove a representable invalid state or separate a
genuinely confusable pair — otherwise the type is ceremony.

## The three tiers

| tier | where | what it pins |
|---|---|---|
| W0 unit | inline `#[cfg(test)] mod tests` / sibling `tests.rs` | arithmetic, conversion math, state machines, `Request` conversion hooks |
| W2 wire white-box | `src/server/tests.rs` | framing + serde + the real middleware stack over `tokio::io::duplex`, driven by the raw JSON-RPC client |
| W3 wire black-box | `tests/lsp_wire.rs` | `serve()` + `Transport::Socket` over real TCP |

Choose the lowest tier that can express the assertion. Use W2/W3 only for
what unit tests cannot see: lifecycle gating, staleness retry, panic
mapping, concurrency bound, termination, wire encoding.

## Harness inventory

- `crate::requests::testing` — baseline server/state/fixtures for
  per-request conversion tests. The `"🙂abc"` document and UTF-16 encoding
  are load-bearing (byte 4 == UTF-16 2).
- `src/server/tests.rs` — `spawn_wire_server`, `RawClient`,
  `EchoServer`/`GatedServer`/`PanickingServer`, `bounded`.
- `tests/lsp_wire.rs` — its own minimal framing client (integration tests
  cannot reach lib test modules; the duplication is accepted).

## Conventions

- Tests live inline per module, or in a sibling `tests.rs` for larger
  modules — never a stray file.
- Real temp workspaces on disk, millisecond-unique names under
  `std::env::temp_dir()`.
- Determinism: channel gates, never sleeps; every cross-task await bounded
  by `tokio::time::timeout` (the `time` feature on the existing tokio
  dev-dependency; futures-rs has no time support).
  `processId: null` in test `initialize` keeps the client monitor inert.
- All three feature configurations must compile and pass; keep tests free
  of tree-sitter-gated API unless the test itself is `#[cfg(feature = "tree-sitter")]`.
- `expect`/`unwrap` are allowed in tests (clippy.toml); they are not
  allowed in `src` outside the two blessed invariants.

## Adding a test for a new `Server` method

The method already follows the three-place pattern (trait method, `Request`
impl, `implement_methods!` line). Add: one W0 conversion test next to its
`Request` impl (via `requests::testing`), and rely on the parametrized
unknown-method W2 test for dispatch — growth adds no wire tests.

Wire-note: unwired methods answer `-32601` only when their params
deserialize — invalid params fail earlier with `-32602` (the router
validates before dispatch); the parametrized test sends minimally valid
params per method.

Known ceiling of the echo round-trip tests (#2, #6): an echo server that
returns the position it received cannot distinguish "conversion works"
from "conversion was deleted" (both are fixpoints on the sent column).
They do fail under either single-direction regression; if a stronger pin
is ever needed, an asserting server that fails unless the handler sees
the UTF-8 byte column breaks the symmetry.
```

Adjust the wording of the audit-outcome line if Task 7 approved further typing candidates.

- [ ] **Step 2: Verify** the rule against reality: every path and helper named above exists (spot-check with the LSP tools).

- [ ] **Step 3: Run the complete battery one final time** (all commands from Global Constraints) and report phase 5 complete.

- [ ] **Step 4: Report for commit**

Suggested message: `docs: testing steering rule — tiers, harness inventory, conventions`

---

## Self-Review (done at plan time)

- **Spec coverage**: Phase 1 = Tasks 1-2; Phase 2 = Tasks 3-7; Phase 3 = Tasks 8-14 (catalog #1-#15: #1 T9, #2-#6 T10, #7-#9 T11, #10-#11 T12, #14-#15 T13, #12-#13 T14); Phase 4 = Tasks 15-20 (gaps 2,3,4,5,6,10 + RangeError triggers; gap 1 closed by T11 #7; gap 7 by T14; gap 8 dissolved by T4); Phase 5 = Task 21. No spec item lacks a task.
- **Placeholders**: none — every code block is the final form to write.
- **Type consistency**: `RangeError` path/variants identical across Tasks 3, 4, 20; `run_over_streams` bounds identical in Tasks 8, 9; `RawClient` method set consistent across Tasks 9-13.
