# Documentation Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the crate's documentation to the rust-skills `doc-*` standard — `///` style everywhere, full coverage enforced by `missing_docs`, `# Errors`/`# Panics` contracts, runnable doctests, two `examples/`, Cargo metadata, and CI enforcement.

**Architecture:** Four staged phases on branch `feature/doc`, one commit each, every phase ending with a green build. Phase 1 is a pure mechanical `/** */` → `///` conversion (no text changes). Phase 2 turns on `missing_docs` and writes everything the lint demands. Phase 3 polishes content (first sentences, sections, links) and removes doc-related `#![allow]`s against the rust-skills `doc-*` rules. Phase 4 enables doctests, adds examples, Cargo metadata, and the CI matrix.

**Tech Stack:** rustdoc (`missing_docs`, `broken_intra_doc_links`, `-D warnings`), clippy (`missing_panics_doc`, pedantic under `-D warnings`), cargo doctests, `tree-sitter-json` (dev-dependency), tokio (dev-dependency for examples), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-24-docs-refactor-design.md`

## Global Constraints

- Branch: `feature/doc`. Do not touch `main`.
- All written artifacts (docs, code comments, commit messages) in English only.
- No behavior changes. The only allowed source-code exceptions: removal of doc-related `#![allow]` attributes, the `[[example]]`/`[dev-dependencies]` additions, and the `rust-version`/metadata keys in `Cargo.toml`.
- Do not extend the README. The only README change in this plan is the `#![doc = include_str!("../README.md")]` hookup.
- Every phase ends with the full verification battery (see below) green, then one commit.
- The agent never performs git write operations (`add`, `commit`, `reset`, `checkout`, branch changes) — a hook enforces read-only git access. Commits are made by the user: at each phase end the executor presents the exact commit command for the user to run (the user can type it with the `!` prefix in the session), then confirms — read-only — via `git log --oneline -1` and `git status --short` that the commit landed and the tree is clean before starting the next phase.
- Feature gates: anything touching `#[cfg(feature = "tree-sitter")]` or `#[cfg(feature = "tracing")]` code must also be checked with `cargo test --no-default-features` and `cargo test --all-features`.
- Doctests must not use tree-sitter-gated API (they run under `--no-default-features` in CI).
- Verification battery (run from the repo root, all must pass silently):

```bash
cargo build --all-targets
cargo test
cargo test --no-default-features
cargo test --all-features
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

- rust-skills is the acceptance-criteria source: invoke it (`/rust-skills`) whenever a step says "accept against `doc-*`". LSP (`documentSymbol`, `hover`, `findReferences`) is the verification tool wherever a step says so.

---

## Phase 1 — Mechanical conversion (one commit)

### Task 1: Convert `/** */` to `///`, move `#[cfg]` after docs

**Files:**
- Modify: all 14 files below (doc comments only)
- `src/document.rs` (18 blocks), `src/document_matcher.rs` (10), `src/oneshot/workspace_diagnostics.rs` (12), `src/serve.rs` (1), `src/server_options.rs` (11), `src/server_state.rs` (4), `src/server_trait.rs` (1), `src/server_with_state.rs` (1), `src/text_utils/conversions.rs` (1), `src/text_utils/encoding.rs` (4), `src/text_utils/position.rs` (1), `src/text_utils/range_ext/mod.rs` (8), `src/transport.rs` (4), `src/tree_sitter_utils.rs` (9) — 85 blocks total.

**Interfaces:**
- Consumes: nothing (first task).
- Produces: every doc comment in `///`/`//!` form with unchanged text. Phases 2–4 edit these blocks further; phase boundaries assume this conversion is complete and text-identical.

**Transformation rule (deterministic — no wording changes allowed in this task):**

1. A block `/**` … `*/` becomes a run of `///` lines. Drop the `/**` and `*/` lines.
2. Each inner line: strip exactly its 4-space indent, prefix `/// ` (an inner blank line becomes a bare `///`).
3. Do not re-wrap or reflow text. Line content stays byte-identical after de-indenting.
4. Where a block already has `# Errors`-style sections, keep them verbatim.

Single-line example — `src/document.rs:55` before:

```rust
    /**
        Returns the URL of the document.
    */
    #[must_use]
    pub fn url(&self) -> &Url {
```

after:

```rust
    /// Returns the URL of the document.
    #[must_use]
    pub fn url(&self) -> &Url {
```

Multi-line example — `src/document.rs:20` before:

```rust
/**
    A document tracked by the language server, containing
    the URL, text, version, and language of the document.

    May be cloned somewhat cheaply to take a snapshot
    of the current state of the document.
*/
```

after:

```rust
/// A document tracked by the language server, containing
/// the URL, text, version, and language of the document.
///
/// May be cloned somewhat cheaply to take a snapshot
/// of the current state of the document.
```

**`#[cfg]` after docs — exactly three sites, swap the attribute and doc comment order:**

`src/document.rs:284-289` before:

```rust
#[cfg(feature = "tree-sitter")]
/**
    A capture from a tree-sitter query on a document.

    Created by calling [`Document::query`].
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentQueryCapture {
```

after:

```rust
/// A capture from a tree-sitter query on a document.
///
/// Created by calling [`Document::query`].
#[cfg(feature = "tree-sitter")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentQueryCapture {
```

`src/document_matcher.rs:31-35` (field) before:

```rust
    #[cfg(feature = "tree-sitter")]
    /**
        The tree-sitter language grammar to associate with the matched document.
    */
    pub lang_grammar: Option<Language>,
```

after:

```rust
    /// The tree-sitter language grammar to associate with the matched document.
    #[cfg(feature = "tree-sitter")]
    pub lang_grammar: Option<Language>,
```

`src/document_matcher.rs:83-88` (method) before:

```rust
    #[cfg(feature = "tree-sitter")]
    /**
        Sets the tree-sitter language grammar
        to associate with the document matcher.
    */
    #[must_use]
    pub fn with_lang_grammar(mut self, lang_grammar: Language) -> Self {
```

after:

```rust
    /// Sets the tree-sitter language grammar
    /// to associate with the document matcher.
    #[cfg(feature = "tree-sitter")]
    #[must_use]
    pub fn with_lang_grammar(mut self, lang_grammar: Language) -> Self {
```

- [ ] **Step 1:** Apply the transformation rule to all 85 blocks in the 14 files listed above. Verify none remain:

```bash
grep -rn '/\*\*' src/
```

Expected: no output.

- [ ] **Step 2:** Apply the three `#[cfg]` swaps shown above.

- [ ] **Step 3:** Run the full verification battery (Global Constraints). Expected: all pass. `cargo fmt --check` must pass without running `cargo fmt` as a fixer — if it complains, the conversion de-indented incorrectly; fix the indent by hand, do not let rustfmt rewrite doc text.

- [ ] **Step 4:** Review that the diff is pure formatting:

```bash
git diff -w --stat
```

Note: `git diff -w` does NOT collapse this conversion to near-zero — the `///` prefix is non-whitespace text, so converted lines still appear as changed. Use it only to spot-check that wording is untouched: `git diff -w` on `src/document.rs` and `src/server_trait.rs` should show doc lines differing solely by the comment markers and indentation, never by words or punctuation.

- [ ] **Step 5:** Hand the commit to the user. Present this command and wait until the user runs it (suggest the `!` prefix) — do not run it yourself:

```bash
git add -A && git commit -m "Convert doc comments to ///, move cfg after docs, fix two broken intra-doc links"
```

Then confirm read-only: `git log --oneline -1` shows the new commit, `git status --short` is empty.

---

## Phase 2 — Coverage (one commit)

### Task 2: Enable `missing_docs`, add module docs and crate root

**Files:**
- Modify: `Cargo.toml` (add `[lints.rust]`)
- Modify: `src/lib.rs:1` (crate doc attr), `src/lib.rs:25` (`server` module doc), `src/text_utils/mod.rs:1`, `src/oneshot/mod.rs:1`, `src/tree_sitter_utils.rs:1`

**Interfaces:**
- Consumes: `///`-converted docs from Task 1.
- Produces: `[lints.rust] missing_docs = "warn"` in `Cargo.toml`; `//!` docs on all four public modules; README as crate docs. Tasks 3–4 close the remaining warnings this lint reports.

- [ ] **Step 1:** Add to `Cargo.toml` after the `[lints.clippy]` section:

```toml
[lints.rust]
missing_docs = "warn"
```

- [ ] **Step 2:** Run the lint to capture the work list (this is the "failing test" for the phase):

```bash
cargo doc --no-deps 2>&1 | grep 'missing documentation' | sort | uniq -c | sort -rn
```

Expected: warnings for the items enumerated in Tasks 3–4 (fields, enum variants, converters, `Server` trait methods, `ServerResult` alias, `WorkspaceDiagnosticReport::documents`) plus the four public modules. Save the output to compare after Tasks 3–4.

- [ ] **Step 3:** Add the crate doc attribute as the first line of `src/lib.rs` (before `pub use async_lsp::lsp_types;`):

```rust
#![doc = include_str!("../README.md")]
```

The README is 35 lines, contains no fenced code blocks, so nothing runs as a doctest once doctests are enabled in Phase 4.

- [ ] **Step 4:** Add `//!` docs. First statement inside the `pub mod server { }` block in `src/lib.rs`:

```rust
pub mod server {
    //! High-level API for implementing language servers.
    //!
    //! Implement [`Server`] with only the methods you need, configure
    //! [`ServerOptions`] and [`DocumentMatcher`]s, then run it with [`serve`]
    //! over a [`Transport`]. Each request receives a [`ServerState`], which
    //! tracks open documents as [`Document`] snapshots.
    //!
    //! All [`Server`] methods work with UTF-8 positions regardless of the
    //! encoding negotiated with the client — conversions between UTF-8,
    //! UTF-16, and UTF-32 are handled internally.
```

First lines of `src/text_utils/mod.rs` (above `mod conversions;`):

```rust
//! Utilities for positions, ranges, and position encodings.
//!
//! Exposes the crate's [`Position`] type, [`Encoding`],
//! [`position_to_encoding`], and [`RangeExt`], used to convert between
//! UTF-8, UTF-16, and UTF-32 coordinates and to manipulate byte, LSP,
//! and tree-sitter ranges.
```

First lines of `src/oneshot/mod.rs` (above `mod server;`):

```rust
//! Run a [`Server`](crate::server::Server) over workspace files on disk,
//! without an LSP client or transport.
//!
//! [`workspace_diagnostics()`] drives the same stateful wrapper as the live
//! server path: it initializes a workspace, opens each matched document, and
//! requests diagnostics once — useful for CLI-style batch runs.
```

First lines of `src/tree_sitter_utils.rs` (above `use std::collections::VecDeque;`):

```rust
//! Helpers for working with tree-sitter syntax trees in an LSP context.
//!
//! All conversions between tree-sitter and LSP coordinates assume UTF-8
//! positions, matching the crate-wide invariant.
```

- [ ] **Step 5:** Run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Expected: fails only with the remaining `missing documentation` warnings from Step 2 (modules now clean). Run `cargo build --all-targets` and `cargo clippy --all-targets -- -D warnings` — expected pass.

### Task 3: Close coverage gaps in `text_utils`, `result`, `transport`, `oneshot` fields

**Files:**
- Modify: `src/text_utils/position.rs`, `src/text_utils/encoding.rs`, `src/result.rs`, `src/transport.rs`, `src/oneshot/workspace_diagnostics.rs`

**Interfaces:**
- Consumes: Task 2's lint list.
- Produces: documented public fields, variants, converters, and the `ServerResult` alias. Task 5 verifies the count reaches zero.

- [ ] **Step 1:** `src/text_utils/position.rs` — document the fields (line 11) and converters:

```rust
pub struct Position {
    /// Zero-based line index.
    pub line: usize,
    /// Column offset within the line, in the units of the encoding in use.
    pub col: usize,
}
```

```rust
    /// Creates a position from an LSP position.
    #[must_use]
    pub const fn from_lsp(position: LspPosition) -> Self {
```

```rust
    /// Converts the position into an LSP position.
    #[must_use]
    pub const fn into_lsp(self) -> LspPosition {
```

```rust
    /// Creates a position from a tree-sitter point.
    #[must_use]
    pub const fn from_ts(point: TsPoint) -> Self {
```

```rust
    /// Converts the position into a tree-sitter point.
    #[must_use]
    pub const fn into_ts(self) -> TsPoint {
```

- [ ] **Step 2:** `src/text_utils/encoding.rs` — document the associated functions (each doc goes directly above the existing `#[must_use]`):

```rust
    /// Returns the LSP default encoding, [`Encoding::UTF16`].
```
(on `default`, line 34)

```rust
    /// Converts the encoding into its `lsp_types` counterpart.
```
(on `into_lsp`, line 39)

```rust
    /// Returns the wire representation of the encoding (`utf-8`, `utf-16`, or `utf-32`).
```
(on `as_str`, line 48)

```rust
    /// Creates an encoding from its `lsp_types` counterpart.
```
(on `from_lsp`, line 58 — the existing `#[allow(clippy::missing_panics_doc)]` stays until Phase 3)

- [ ] **Step 3:** `src/result.rs` — document the alias (line 10), the enum variants, and the constructors:

```rust
/// Convenience `Result` alias for operations that can fail with a [`ServerError`].
pub type ServerResult<T> = Result<T, ServerError>;
```

The `ServerError` enum itself (line 13) also needs a doc — it is part of this task's gap list:

```rust
/// An error that can occur while running a language server.
#[derive(Debug, Error)]
pub enum ServerError {
```

```rust
#[derive(Debug, Error)]
pub enum ServerError {
    /// Failed to connect to the given TCP port.
    #[error("Failed to connect to port {0}")]
    TcpConnect(u16),
    /// Error that does not fit any other variant.
    #[error("Uncategorized error: {0}")]
    Unknown(String),
    /// JSON-RPC error sent to or received from the client.
    #[error("JSON RPC error: {0}")]
    Rpc(ServerErrorCode, String),
    /// Error raised by the underlying async-lsp machinery.
    #[error(transparent)]
    Lsp(#[from] async_lsp::Error),
    /// I/O error raised by a transport or a file read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

```rust
    /// Wraps an arbitrary error as a [`ServerError::Unknown`].
    pub fn unknown(error: impl Into<BoxDynError>) -> Self {
```

```rust
    /// Creates a JSON-RPC error with the given code and message.
    pub fn rpc(code: ServerErrorCode, message: impl ToString) -> Self {
```

- [ ] **Step 4:** `src/transport.rs` — document the variants:

```rust
pub enum Transport {
    /// Connects to a TCP socket on the given port of `127.0.0.1`.
    Socket(u16),
    /// Uses the process standard input and output.
    #[default]
    Stdio,
}
```

```rust
pub enum LspTransportRead {
    /// Read half of a connected [`Transport::Socket`].
    Socket(OwnedReadHalf),
    /// Read half of [`Transport::Stdio`].
    Stdio(Stdin),
}
```

```rust
pub enum LspTransportWrite {
    /// Write half of a connected [`Transport::Socket`].
    Socket(OwnedWriteHalf),
    /// Write half of [`Transport::Stdio`].
    Stdio(Stdout),
}
```

- [ ] **Step 5:** `src/oneshot/workspace_diagnostics.rs` — document the public fields (lines 80, 98–100):

```rust
pub struct WorkspaceDiagnosticReport {
    /// Diagnostics for each matched document, one entry per document.
    pub documents: Vec<DocumentDiagnostics>,
}
```

```rust
pub struct DocumentDiagnostics {
    /// URI of the document the diagnostics belong to.
    pub uri: Url,
    /// Document version at the time the diagnostics were produced.
    pub version: i32,
    /// The diagnostic report returned by the server.
    pub report: DocumentDiagnosticReportResult,
}
```

- [ ] **Step 6:** Re-run the doc lint and confirm only the `Server` trait items remain. Note: warning summary lines carry no filename — map via the `--> path:line` lines instead:

```bash
cargo doc --no-deps 2>&1 | grep -A2 'missing documentation' | grep -- '-->' | grep -v server_trait
```

Expected: no output. Two easy-to-miss items belong to this task (found during execution 2026-08-24): the `ServerError` enum itself (`src/result.rs`) — `/// An error that can occur while running a language server.` — and the `RangeExt::Position` associated type (`src/text_utils/range_ext/mod.rs`) — `/// The position type used by this kind of range.` directly above `type Position;`.

### Task 4: Document every `Server` trait method

**Files:**
- Modify: `src/server_trait.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: individually documented trait methods; the four group `//` comments are removed.

- [ ] **Step 1:** Add a doc comment above each of the 19 trait methods. Docs for the four statics:

```rust
    /// Returns the server name and version reported to the client during initialization.
    fn server_info() -> Option<ServerInfo> {
```

```rust
    /// Returns the options configuring this crate's behavior, read once during initialization.
    fn server_options(&self) -> ServerOptions {
```

```rust
    /// Returns the capabilities to advertise to the client during initialization.
    ///
    /// Merge the capabilities required by the implemented [`Server`] methods
    /// into a [`ServerCapabilities`] value. Returning `None` advertises only
    /// the crate's defaults.
    fn server_capabilities(client_capabilities: ClientCapabilities) -> Option<ServerCapabilities> {
```

```rust
    /// Returns the matchers that associate documents with languages and grammars.
    ///
    /// See [`DocumentMatcher`] for how documents are matched.
    fn server_document_matchers() -> Vec<DocumentMatcher> {
```

Docs for the request handlers (each goes directly above the existing method signature):

```rust
    /// Handles `textDocument/hover` requests from the client.
    ///
    /// Returns hover contents for the position in `params`, or `None` when
    /// there is nothing to show. Positions and ranges are UTF-8. Requires a
    /// hover provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/completion` requests from the client.
    ///
    /// Returns completion items at the position in `params`, or `None`.
    /// Requires a completion provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `completionItem/resolve` requests from the client.
    ///
    /// Fills in additional detail on an item previously returned by
    /// [`Server::completion`]. The default implementation resolves the item
    /// unchanged; returning the item as-is is always valid. Requires a
    /// completion provider with `resolve_provider` enabled.
```

```rust
    /// Handles `textDocument/codeAction` requests from the client.
    ///
    /// Returns code actions available for the range in `params`, or `None`.
    /// Requires a code action provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `codeAction/resolve` requests from the client.
    ///
    /// Fills in additional detail on an action previously returned by
    /// [`Server::code_action`]. The default implementation resolves the
    /// action unchanged. Requires a code action provider with
    /// `resolve_provider` enabled.
```

```rust
    /// Handles `textDocument/documentLink` requests from the client.
    ///
    /// Returns links inside the document in `params`, or `None`. Requires a
    /// document link provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `documentLink/resolve` requests from the client.
    ///
    /// Fills in the target of a link previously returned by [`Server::link`].
    /// The default implementation resolves the link unchanged. Requires a
    /// document link provider with `resolve_provider` enabled.
```

```rust
    /// Handles `textDocument/declaration` requests from the client.
    ///
    /// Returns the declaration locations of the symbol at the position in
    /// `params`, or `None`. Requires a declaration provider in
    /// [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/definition` requests from the client.
    ///
    /// Returns the definition locations of the symbol at the position in
    /// `params`, or `None`. Requires a definition provider in
    /// [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/references` requests from the client.
    ///
    /// Returns the locations that reference the symbol at the position in
    /// `params`, or `None`. Requires a references provider in
    /// [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/rename` requests from the client.
    ///
    /// Returns a workspace edit renaming the symbol at the position in
    /// `params` to `params.new_name`, or `None` when renaming is not
    /// possible. Requires a rename provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/prepareRename` requests from the client.
    ///
    /// Returns the range of the symbol at the position in `params` that a
    /// rename would apply to, or `None` when renaming is not possible.
    /// Requires a rename provider with `prepare_provider` enabled.
```

```rust
    /// Handles `textDocument/formatting` requests from the client.
    ///
    /// Returns edits formatting the whole document in `params`, or `None`.
    /// Requires a document formatting provider in
    /// [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/rangeFormatting` requests from the client.
    ///
    /// Returns edits formatting the range in `params`, or `None`. Requires a
    /// document range formatting provider in [`Server::server_capabilities`].
```

```rust
    /// Handles `textDocument/diagnostic` requests from the client.
    ///
    /// Returns the diagnostics for the document in `params`. The document's
    /// current snapshot is available through
    /// `state.document(&params.text_document.uri)`. Requires a diagnostic
    /// provider in [`Server::server_capabilities`].
```

Note: `use async_lsp::lsp_types::ServerCapabilities` is already imported at the top of the file (line 14) — the `server_capabilities` doc link resolves without new imports.

- [ ] **Step 2:** Delete the four now-redundant group comments (`// Hover, Completion, Code Action, Document Link`, `// Declaration, Definition, References, Rename`, `// Formatting`, `// Diagnostics`).

- [ ] **Step 3:** `cargo doc --no-deps 2>&1 | grep 'missing documentation'` — expected: no output.

### Task 5: LSP cross-check and Phase 2 commit

**Files:** none (verification only)

- [ ] **Step 1:** LSP control pass (per spec §5): run `documentSymbol` over `src/server_trait.rs`, `src/document.rs`, `src/document_matcher.rs`, `src/server_options.rs`, `src/server_state.rs`, `src/transport.rs`, `src/result.rs`, `src/text_utils/*.rs`, `src/oneshot/*.rs`, `src/tree_sitter_utils.rs`. For every public symbol in the outline, confirm a doc exists (expand/`hover` the symbol). The two lists — LSP symbol inventory and the Step 2 lint output from Task 2 — must account for exactly the same items.
- [ ] **Step 2:** `hover` on `Server`, `Document`, `ServerState`, `Transport`, `Encoding` — confirm the rendered docs start with the intended summary sentence.
- [ ] **Step 3:** Run the full verification battery. Expected: all pass, including `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` (zero `missing_docs` warnings).
- [ ] **Step 4:** Hand the commit to the user. Present this command and wait until the user runs it (suggest the `!` prefix) — do not run it yourself:

```bash
git add -A && git commit -m "Add module docs and close missing documentation gaps"
```

Then confirm read-only: `git log --oneline -1` shows the new commit, `git status --short` is empty.

---

## Phase 3 — Content quality (one commit)

### Task 6: First sentences, canonical sections, link unification

**Files:**
- Modify: `src/document.rs`, `src/document_matcher.rs`, `src/text_utils/position.rs`, `src/text_utils/encoding.rs`, `src/text_utils/range_ext/mod.rs`, `src/tree_sitter_utils.rs`

**Interfaces:**
- Consumes: docs from Phases 1–2.
- Produces: every public doc with a standalone ≤ ~15-word summary sentence, details prose after a blank line, full-path intra-doc links. Canonical section order after Phase 3/4: summary → details → `# Examples` → `# Errors` → `# Panics` → `# Safety` (only the applicable ones). The two `# Example Usage` blocks in `range_ext/mod.rs` are intentionally left for Task 10 to replace wholesale with runnable `# Examples`.

- [ ] **Step 1:** Rewrite the failing first sentences (current → new). The summary line(s) move into one short opening sentence; the rest of the old text stays in the details paragraph unchanged where still accurate:

- `src/document.rs` `Document`: "A document tracked by the language server, containing the URL, text, version, and language of the document." → "A snapshot of a text document tracked by the language server." (keep the cloning/read-only prose as details)
- `src/document_matcher.rs` `DocumentMatcher`: "Options for matching documents based on their URLs and language identifiers, and associating them with an optional tree-sitter language grammar when tree-sitter feature is enabled." → "Associates documents with a name by URL glob and/or language id." (keep the tree-sitter sentence as details)
- `src/text_utils/position.rs` `Position`: → "A zero-based line and column position." (keep the cheap-copy/conversion prose as details)
- `src/text_utils/encoding.rs` `Encoding`: "A position encoding supported by this library." → "A position encoding supported by this crate."
- `src/text_utils/range_ext/mod.rs` `RangeExt`: "Extension trait for different kinds of ranges:" → "Extensions for splitting, shrinking, and delimiting ranges." (the numbered list and the methods list stay as details)
- `src/tree_sitter_utils.rs` — all nine summaries lack sentence-ending periods; add them (e.g. "Converts a tree sitter `Point` to an LSP `Position`.").
- `src/document.rs` `Document::text_bytes` (line ~100): copy-paste bug — its doc duplicates `text_contents`. Change the summary to "Returns the full text of the document, as bytes." and keep the `text_reader` preference sentence.
- `src/document_matcher.rs` `DocumentMatcher::new`: reconcile the contradiction with the `name` field doc ("only used for debugging purposes" vs "unique identifier"). New details text: "The name is exposed on matched documents through [`Document::matched_name`]; it does not need to be unique."

- [ ] **Step 2:** Unify intra-doc links to full paths. The two baseline-broken links were already fixed right after Task 1 (user decision, 2026-08-24, carried by the phase-1 commit): `src/document.rs` `node_at_position_named` now links [`Document::node_at_position`], and `src/document_matcher.rs` `name` now links [`crate::server::Document::matched_name`]. Sweep for any other bare method links:

```bash
grep -rnE '\[`[a-z_]+`\]' src/ | grep -v '#[' | grep -v tests
```

Every hit must be a link to a free function, type, or module (fine — they have no owner) or gain its owner prefix (`Type::method` for methods and fields). `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` must stay green (it catches broken links but not bare ones — the grep is the check).

- [ ] **Step 3:** Verify section order on the items that have sections today (`serve`, `Transport::into_read_write`, `oneshot::workspace_diagnostics`, the two `range_ext` blocks): summary → details → examples/errors/panics, no section before the details prose.

### Task 7: `# Panics` / `# Errors` contracts, remove the `#![allow]`s

**Files:**
- Modify: `src/document.rs`, `src/text_utils/range_ext/mod.rs`, `src/text_utils/encoding.rs`, `src/transport.rs`, `src/serve.rs`, `src/oneshot/workspace_diagnostics.rs` (prose only)

**Interfaces:**
- Consumes: Task 6 docs.
- Produces: `# Panics` sections wherever a panic path exists, `# Errors` on every public fallible function, `Option`-returning functions describing their `None` conditions, and no doc-related `#![allow]` left in the crate.

- [ ] **Step 1:** `src/document.rs` `Document::node_text` — replace the prose sentence "Panics if the node is not valid for the document." with a section:

```rust
    /// Returns the UTF-8 text of a [`Node`].
    ///
    /// # Panics
    ///
    /// Panics if the node's byte range is not within this document.
```

- [ ] **Step 2:** `src/text_utils/range_ext/mod.rs` — add `# Panics` sections to the `RangeExt` methods whose implementations assert (byte-range impl in `bytes.rs` asserts `at <= end - start`, `from <= to <= end - start`):

```rust
    /// # Panics
    ///
    /// Panics if `at` lies beyond the end of the range.
```
(added to `split_at` — `split_off_left`/`split_off_right` inherit the contract through `split_at`, no separate section)

```rust
    /// # Panics
    ///
    /// Panics if the range spans multiple lines.
```
(`shrink` — its existing prose sentence "Panics if the range spans across multiple lines." moves into this section)

```rust
    /// # Panics
    ///
    /// Panics if `from` or `to` lie beyond the end of the range, or if `from > to`.
```
(`sub`)

- [ ] **Step 3:** `src/text_utils/encoding.rs` `Encoding::from_lsp` — delete the `#[allow(clippy::missing_panics_doc)]` attribute (line 56) and add:

```rust
    /// # Panics
    ///
    /// Panics if the encoding kind is not one of UTF-8, UTF-16, or UTF-32.
```

- [ ] **Step 4:** `src/transport.rs` — delete line 1 (`#![allow(clippy::missing_panics_doc)]`). Add to `Transport::into_read_write` (after its existing `# Errors`):

```rust
    /// # Panics
    ///
    /// Panics on a `Transport` variant other than [`Transport::Socket`] and
    /// [`Transport::Stdio`]. There is no such variant today; the branch
    /// exists so a future variant fails loudly instead of silently.
```

- [ ] **Step 5:** `src/serve.rs` — delete line 1 (`#![allow(clippy::missing_panics_doc)]`), then run `cargo clippy --all-targets -- -D warnings`. The spec's audit says `serve` has no reachable panic (`NonZeroUsize::new(8).unwrap()` at `serve.rs:50` is statically infallible). Branch:
  - If clippy passes: done, no section needed.
  - If clippy reports `missing_panics_doc` on `serve`: the lint disagrees with the audit. Do not re-add the crate-level allow. Instead hoist the literal out of the function body, directly above `pub async fn serve`:

```rust
const MAX_CONCURRENT_REQUESTS: NonZeroUsize = match NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};
```

    and use `.layer(ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS))` in the body. `NonZeroUsize::new` is `const fn`, so the constant evaluates at compile time; behavior is identical (still 8). This moves the panic-capable expression out of the documented function, satisfying the lint. Record in the commit message which branch was taken.

- [ ] **Step 6:** `# Errors` audit — every public function returning `Result` must have an `# Errors` section. Inventory: `serve` (has one), `Transport::into_read_write` (has one), `oneshot::workspace_diagnostics` (has one). Confirm by inspection:

```bash
grep -rn 'pub.*fn.*-> .*Result' src/ | grep -v 'pub(crate)' | grep -v tests
```

Expected: only those three plus `Server` trait methods returning `ServerResult` futures — for the trait methods, the error behavior is uniform (`ServerError`, reported when the method is not implemented or the handler fails), which is documented on the trait itself; add one sentence to the trait doc (after the existing second paragraph): "Handlers report failures by returning `Err(ServerError)`; the wrapper converts them to LSP error responses."

- [ ] **Step 7:** `src/document.rs` `Document::query` — replace "Returns `Some(captures)` if the query was successful, otherwise `None`." with explicit `None` conditions matching the implementation: "Returns `None` when the document has no tree-sitter language or parsed tree assigned, or when the query string fails to compile."

### Task 8: rust-skills acceptance and Phase 3 commit

**Files:** none (verification only)

- [ ] **Step 1:** Invoke rust-skills (`/rust-skills`), filter the rule list to every `doc-*` rule (the spec references 15; known ones include `doc-all-public`, `doc-module-inner`, `doc-first-sentence`, `doc-canonical-sections`, `doc-hidden-setup`, `doc-question-mark`). Check the crate against each. The two doc-example rules (`doc-hidden-setup`, `doc-question-mark`) apply from Phase 4 onward — their Phase 3 status is "no runnable examples yet, no violation". Fix any violations found in this step directly.
- [ ] **Step 2:** Run the full verification battery. Expected: all pass.
- [ ] **Step 3:** Hand the commit to the user. Present this command and wait until the user runs it (suggest the `!` prefix) — do not run it yourself:

```bash
git add -A && git commit -m "Polish documentation content and contracts"
```

Then confirm read-only: `git log --oneline -1` shows the new commit, `git status --short` is empty.

---

## Phase 4 — Doctests, examples, metadata, CI (one commit)

### Task 9: Cargo metadata and MSRV

**Files:**
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: nothing.
- Produces: complete `[package]` metadata; `cargo_common_metadata` allow removed; verified `rust-version`. Task 14's CI assumes `readme = "README.md"` is set here.

- [ ] **Step 1:** Edit the `[package]` section:

```toml
[package]
name = "async-language-server"
version = "0.0.0"
edition = "2024"
license = "MIT"
publish = false
description = "A higher-level abstraction on top of async-lsp for building language servers with less boilerplate"
repository = "https://github.com/Jazz-Man/async-language-server"
readme = "README.md"
rust-version = "1.88"
```

(`repository` is the `origin` remote URL converted to https.)

- [ ] **Step 2:** Delete this line from `[lints.clippy]`:

```toml
cargo_common_metadata = { level = "allow", priority = 1 }
```

- [ ] **Step 3:** Verify the MSRV empirically (the code uses let-chains, stable since 1.88; the spec requires an actual build check):

```bash
rustup toolchain list | grep -q 1.88 || rustup toolchain install 1.88 --profile minimal
cargo +1.88 check --all-targets
```

Expected: success. If dependencies (not this crate's code) fail to build under 1.88, run `cargo +1.88 update` (with `rust-version` set and edition 2024's resolver this re-resolves to MSRV-compatible versions), re-check, and re-run the battery under the stable toolchain. If the workspace still cannot build under 1.88, raise `rust-version` to the smallest version that builds (try 1.89, 1.90, …) and record the final value in the commit message.

- [ ] **Step 4:** `cargo clippy --all-targets -- -D warnings` — expected pass (confirms removing the `cargo_common_metadata` allow is clean now that metadata is present).

### Task 10: Enable doctests; core API examples

**Files:**
- Modify: `Cargo.toml` (remove `doctest = false`; add dev-dependencies), `src/text_utils/position.rs`, `src/text_utils/encoding.rs`, `src/text_utils/conversions.rs`, `src/text_utils/range_ext/mod.rs`, `src/document_matcher.rs`, `src/server_options.rs`, `src/transport.rs`, `src/result.rs`

**Interfaces:**
- Consumes: Phase 3 docs.
- Produces: `cargo test` runs doctests; six `# Examples` sections. Task 11 adds the two large ones.

- [ ] **Step 1:** In `Cargo.toml`, delete the `doctest = false` line from `[lib]`, and add:

```toml
[dev-dependencies]
tokio = { version = "1.53.1", features = ["rt", "rt-multi-thread", "macros"] }
```

(the lib's own tokio features are unchanged; examples and doctests get `#[tokio::main]`).

- [ ] **Step 2:** `src/text_utils/position.rs` — append to the `Position` struct doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::Position;
/// use async_lsp::lsp_types::Position as LspPosition;
///
/// let position = Position { line: 3, col: 7 };
/// let lsp = position.into_lsp();
/// assert_eq!(lsp, LspPosition { line: 3, character: 7 });
/// assert_eq!(Position::from_lsp(lsp), position);
/// ```
```

- [ ] **Step 3:** `src/text_utils/encoding.rs` — append to the `Encoding` struct doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::Encoding;
///
/// // The LSP default when the client does not negotiate an encoding.
/// assert_eq!(Encoding::default(), Encoding::UTF16);
/// assert_eq!(Encoding::UTF8.as_str(), "utf-8");
/// ```
```

- [ ] **Step 4:** `src/text_utils/conversions.rs` — append to the `position_to_encoding` doc (values mirror the existing unit tests):

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::{Encoding, Position, position_to_encoding};
///
/// let text = ropey::Rope::from_str("a\u{1f642}b");
///
/// // The smiley is 4 UTF-8 bytes but 2 UTF-16 units.
/// let position = Position { line: 0, col: 5 };
/// let converted = position_to_encoding(&text, position, Encoding::UTF8, Encoding::UTF16);
/// assert_eq!(converted, Position { line: 0, col: 3 });
/// ```
```

(`ropey` is a regular dependency and therefore visible to doctests; no dev-dependency needed.)

- [ ] **Step 5:** `src/text_utils/range_ext/mod.rs` — append to the `RangeExt` trait doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::RangeExt;
///
/// let (left, right) = (0..7).split_at("one/two", 3);
/// assert_eq!(left, 0..3);
/// assert_eq!(right, 3..7);
///
/// assert_eq!((0..7).shrink(1, 2), 1..5);
/// assert_eq!((0..7).sub("one/two", 1, 5), 1..5);
/// ```
```

And replace the two `# Example Usage` sections (`sub_delimited`, `sub_delimited_tri`) with runnable `# Examples` sections — the assertions are the exact values from the old illustrative comments, verified against the byte-range implementation:

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::RangeExt;
///
/// const D: char = '/';
///
/// assert_eq!((0..7).sub_delimited("one/two", D), (Some(0..3), Some(4..7)));
/// assert_eq!((0..4).sub_delimited("/two", D), (None, Some(1..4)));
/// assert_eq!((0..4).sub_delimited("one/", D), (Some(0..3), None));
/// assert_eq!((0..3).sub_delimited("one", D), (Some(0..3), None));
/// assert_eq!((0..0).sub_delimited("", D), (None, None));
/// ```
```

```rust
/// # Examples
///
/// ```
/// use async_language_server::text_utils::RangeExt;
///
/// const D0: char = '/';
/// const D1: char = '@';
///
/// assert_eq!(
///     (0..13).sub_delimited_tri("one/two@three", D0, D1),
///     (Some(0..3), Some(4..7), Some(8..13)),
/// );
/// assert_eq!(
///     (0..7).sub_delimited_tri("one/two", D0, D1),
///     (Some(0..3), Some(4..7), None),
/// );
/// assert_eq!(
///     (0..3).sub_delimited_tri("one", D0, D1),
///     (Some(0..3), None, None),
/// );
/// assert_eq!((0..0).sub_delimited_tri("", D0, D1), (None, None, None));
/// ```
```

- [ ] **Step 6:** `src/document_matcher.rs` — append to the `DocumentMatcher` struct doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::server::DocumentMatcher;
///
/// let matcher = DocumentMatcher::new("json")
///     .with_url_globs(["**/*.json", "*.jsonc"])
///     .with_lang_strings(["json", "jsonc"]);
///
/// assert_eq!(matcher.name, "json");
/// assert_eq!(matcher.url_globs, ["**/*.json", "*.jsonc"]);
/// ```
```

- [ ] **Step 7:** `src/server_options.rs` — append to `ServerOptions::with_workspace_diagnostics`:

```rust
/// # Examples
///
/// ```
/// use async_language_server::server::{ServerOptions, WorkspaceDiagnostics};
///
/// let options = ServerOptions::default()
///     .with_workspace_diagnostics(WorkspaceDiagnostics::disabled());
/// ```
```

- [ ] **Step 8:** `src/transport.rs` — append to the `Transport` struct doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::server::Transport;
///
/// assert_eq!(Transport::Stdio.to_string(), "Stdio");
/// assert_eq!(Transport::Socket(9999).to_string(), "Socket(9999)");
/// ```
```

- [ ] **Step 9:** `src/result.rs` — append to the `ServerError` enum doc:

```rust
/// # Examples
///
/// ```
/// use async_language_server::server::ServerError;
///
/// let error = ServerError::TcpConnect(9999);
/// assert_eq!(error.to_string(), "Failed to connect to port 9999");
///
/// let error = ServerError::from("boom");
/// assert_eq!(error.to_string(), "Uncategorized error: boom");
/// ```
```

- [ ] **Step 10:** Run `cargo test` — expected: the new doctests appear and pass (`test src/lib.rs - ...` lines). Run `cargo test --no-default-features` — all doctests must still pass (none use tree-sitter API).

### Task 11: The two large doctests (`oneshot`, `serve`)

**Files:**
- Modify: `src/oneshot/workspace_diagnostics.rs`, `src/serve.rs`

**Interfaces:**
- Consumes: tokio dev-dependency from Task 10.
- Produces: a full `oneshot::workspace_diagnostics` run inside a doctest and a compiling `no_run` example for `serve`.

- [ ] **Step 1:** Append to the `workspace_diagnostics` function doc (setup hidden per `doc-hidden-setup`, errors via `?` per `doc-question-mark`; the report shape mirrors the module's own tests — `DocumentDiagnosticReport::Full` wraps `RelatedFullDocumentDiagnosticReport` in this `lsp_types` version):

```rust
/// # Examples
///
/// Run a `Server` over a directory without an LSP client:
///
/// ```
/// use async_lsp::lsp_types::{
///     Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport,
///     DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, Position, Range,
///     RelatedFullDocumentDiagnosticReport,
/// };
/// use async_language_server::oneshot::WorkspaceDiagnosticConfig;
/// use async_language_server::server::{
///     DocumentMatcher, Server, ServerResult, ServerState,
/// };
///
/// struct LongLineServer;
///
/// impl Server for LongLineServer {
///     fn server_document_matchers() -> Vec<DocumentMatcher> {
///         vec![DocumentMatcher::new("demo").with_url_globs(["**/*.demo", "*.demo"])]
///     }
///
///     async fn document_diagnostics(
///         &self,
///         state: ServerState,
///         params: DocumentDiagnosticParams,
///     ) -> ServerResult<DocumentDiagnosticReportResult> {
///         let document = state
///             .document(&params.text_document.uri)
///             .expect("document is open");
///         let mut items = Vec::new();
///         for (line, text) in document.text_contents().lines().enumerate() {
///             if text.len() > 20 {
///                 items.push(Diagnostic {
///                     range: Range {
///                         start: Position { line: line as u32, character: 0 },
///                         end: Position { line: line as u32, character: text.len() as u32 },
///                     },
///                     message: format!("line is {} bytes long", text.len()),
///                     ..Diagnostic::default()
///                 });
///             }
///         }
///         Ok(DocumentDiagnosticReportResult::Report(
///             DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
///                 related_documents: None,
///                 full_document_diagnostic_report: FullDocumentDiagnosticReport {
///                     result_id: None,
///                     items,
///                 },
///             }),
///         ))
///     }
/// }
///
/// # fn main() -> Result<(), async_language_server::server::ServerError> {
/// # use async_language_server::oneshot::workspace_diagnostics;
/// # let root = std::env::temp_dir().join("async-language-server-oneshot-doctest");
/// # let _ = std::fs::remove_dir_all(&root);
/// # std::fs::create_dir_all(&root)?;
/// std::fs::write(root.join("sample.demo"), "short\nthis line is much too long\n")?;
///
/// let report =
///     futures::executor::block_on(workspace_diagnostics(LongLineServer, WorkspaceDiagnosticConfig::new(&root)))?;
///
/// assert_eq!(report.documents.len(), 1);
/// assert!(report.documents[0].uri.path().ends_with("sample.demo"));
/// assert_eq!(report.documents[0].diagnostics().len(), 1);
///
/// std::fs::remove_dir_all(root)?;
/// Ok(())
/// # }
/// ```
```

Notes for the implementer: matchers in the oneshot path are matched by URL globs only (`DocumentMatchers::find_url`), so `with_url_globs` is required — `with_lang_strings` alone would find nothing. `futures::executor::block_on` avoids an async main; `futures` is a regular dependency.

- [ ] **Step 2:** Append to the `serve` function doc:

```rust
/// # Examples
///
/// A stdio server cannot run inside a doctest, so this example only compiles:
///
/// ```no_run
/// use async_language_server::server::{Transport, serve};
/// # #[derive(Clone)]
/// # struct MyServer;
/// # impl async_language_server::server::Server for MyServer {}
/// # #[tokio::main]
/// # async fn main() -> async_language_server::server::ServerResult<()> {
/// serve(Transport::Stdio, MyServer).await
/// # }
/// ```
```

- [ ] **Step 3:** `cargo test` — expected: both new doctests pass (the `no_run` one shows as compiled-only). `cargo test --no-default-features` — expected pass.

### Task 12: `examples/minimal.rs`

**Files:**
- Create: `examples/minimal.rs`

**Interfaces:**
- Consumes: tokio dev-dependency (Task 10).
- Produces: a compiling, clippy-clean example demonstrating `Server` + `Transport::Stdio` + `serve`.

- [ ] **Step 1:** Create `examples/minimal.rs`:

```rust
//! A minimal language server that reports over-long lines as diagnostics.
//!
//! Run from an LSP client that launches it over stdio:
//!
//! ```text
//! cargo run --example minimal
//! ```

use async_language_server::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticServerCapabilities, DocumentDiagnosticOptions,
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Position, Range, RelatedFullDocumentDiagnosticReport,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};
use async_language_server::server::{Server, ServerResult, ServerState, Transport, serve};

/// Lines longer than this many bytes are reported.
const MAX_LINE_BYTES: usize = 80;

#[derive(Clone)]
struct LongLineServer;

impl Server for LongLineServer {
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DocumentDiagnosticOptions {
                    identifier: Some("long-lines".into()),
                    inter_file_dependencies: false,
                    workspace_diagnostics: false,
                },
            )),
            ..ServerCapabilities::default()
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    async fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> ServerResult<DocumentDiagnosticReportResult> {
        let Some(document) = state.document(&params.text_document.uri) else {
            return Ok(full_report(Vec::new()));
        };

        let mut items = Vec::new();
        for (line, text) in document.text_contents().lines().enumerate() {
            let length = text.len();
            if length > MAX_LINE_BYTES {
                items.push(Diagnostic {
                    range: Range::new(
                        Position::new(line as u32, 0),
                        Position::new(line as u32, length as u32),
                    ),
                    message: format!(
                        "line is {length} bytes long, over the {MAX_LINE_BYTES}-byte limit"
                    ),
                    ..Diagnostic::default()
                });
            }
        }

        Ok(full_report(items))
    }
}

fn full_report(items: Vec<Diagnostic>) -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items,
            },
        },
    ))
}

#[tokio::main]
async fn main() -> ServerResult<()> {
    serve(Transport::Stdio, LongLineServer).await
}
```

- [ ] **Step 2:** `cargo build --all-targets && cargo clippy --all-targets -- -D warnings` — expected pass (the `#[allow(clippy::cast_possible_truncation)]` keeps pedantic's cast lint quiet under `-D warnings`). If `DiagnosticServerCapabilities`/`DocumentDiagnosticOptions` field names differ in this `lsp_types` version, adjust to what compiles — do not change the example's structure.

### Task 13: `examples/tree_sitter.rs` and dev-dependency

**Files:**
- Modify: `Cargo.toml` (dev-dependency + `[[example]]` gate)
- Create: `examples/tree_sitter.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: the tree-sitter example, gated on the feature so `cargo test --no-default-features` skips it.

- [ ] **Step 1:** Add the grammar dev-dependency (picks the version compatible with `tree-sitter = 0.26`):

```bash
cargo add --dev tree-sitter-json
```

- [ ] **Step 2:** Gate the example so it is not built when the feature is off — add to `Cargo.toml`:

```toml
[[example]]
name = "tree_sitter"
required-features = ["tree-sitter"]
```

- [ ] **Step 3:** Create `examples/tree_sitter.rs`:

```rust
//! A language server that parses JSON with tree-sitter and reports syntax errors.
//!
//! Run from an LSP client that launches it over stdio:
//!
//! ```text
//! cargo run --example tree_sitter
//! ```

use async_language_server::lsp_types::{
    ClientCapabilities, Diagnostic, DiagnosticServerCapabilities, DiagnosticSeverity,
    DocumentDiagnosticOptions, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};
use async_language_server::server::{
    DocumentMatcher, Server, ServerResult, ServerState, Transport, serve,
};

#[derive(Clone)]
struct JsonServer;

impl Server for JsonServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("json")
            .with_url_globs(["**/*.json"])
            .with_lang_grammar(tree_sitter_json::LANGUAGE)]
    }

    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DocumentDiagnosticOptions {
                    identifier: Some("json".into()),
                    inter_file_dependencies: false,
                    workspace_diagnostics: false,
                },
            )),
            ..ServerCapabilities::default()
        })
    }

    async fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> ServerResult<DocumentDiagnosticReportResult> {
        let Some(document) = state.document(&params.text_document.uri) else {
            return Ok(full_report(Vec::new()));
        };

        let mut items = Vec::new();
        if document.has_syntax_tree() {
            // The tree is parsed and incrementally updated by the crate;
            // query it for parser ERROR nodes.
            for capture in document.query("(ERROR) @error").into_iter().flatten() {
                items.push(Diagnostic {
                    range: capture.range,
                    message: "syntax error".to_owned(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    ..Diagnostic::default()
                });
            }
        }

        Ok(full_report(items))
    }
}

fn full_report(items: Vec<Diagnostic>) -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items,
            },
        },
    ))
}

#[tokio::main]
async fn main() -> ServerResult<()> {
    serve(Transport::Stdio, JsonServer).await
}
```

If the installed `tree-sitter-json` exposes `language()` instead of a `LANGUAGE` constant, use `tree_sitter_json::language()` — keep everything else identical.

- [ ] **Step 4:** `cargo build --all-targets && cargo clippy --all-targets -- -D warnings && cargo test --no-default-features` — expected pass (the example is skipped without the feature).

### Task 14: CI workflow and final verification

**Files:**
- Modify: `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: everything above.
- Produces: CI enforcing the whole battery across three feature configurations.

- [ ] **Step 1:** Replace the steps block of `.github/workflows/rust.yml` so the full file reads:

```yaml
name: Rust

on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  build:

    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v4
    - name: Build
      run: cargo build --all-targets --verbose
    - name: Test (default features)
      run: cargo test --verbose
    - name: Test (no default features)
      run: cargo test --no-default-features --verbose
    - name: Test (all features)
      run: cargo test --all-features --verbose
    - name: Format
      run: cargo fmt --check
    - name: Clippy
      run: cargo clippy --all-targets -- -D warnings
    - name: Docs
      run: cargo doc --no-deps --verbose
      env:
        RUSTDOCFLAGS: "-D warnings"
```

- [ ] **Step 2:** Run the full verification battery locally one final time (all seven commands from Global Constraints). Expected: all pass, and `cargo test` output shows 12 doctests (`position_to_encoding`, `Position`, `Encoding`, `RangeExt`, `sub_delimited`, `sub_delimited_tri`, `DocumentMatcher`, `with_workspace_diagnostics`, `Transport`, `ServerError`, `workspace_diagnostics`, `serve`) plus the two examples compiling.

- [ ] **Step 3:** Final rust-skills pass — `doc-hidden-setup` and `doc-question-mark` against every runnable example from Tasks 10–11 (hidden setup lines all start with `# `, doctest error handling uses `?` with a `Result`-returning `main`).

- [ ] **Step 4:** Hand the commit to the user. Present this command and wait until the user runs it (suggest the `!` prefix) — do not run it yourself:

```bash
git add -A && git commit -m "Enforce full verification battery in CI"
```

Then confirm read-only: `git log --oneline -1` shows the new commit, `git status --short` is empty.

---

## Verification Summary

After Phase 4 the crate satisfies, end to end:

- rust-skills `doc-*` rules (Phase 3 acceptance + Phase 4 example rules).
- `lint-rustfmt-check`, `lint-static-verification`, `lint-missing-docs` (CI job).
- Two independent coverage checks that agree: LSP `documentSymbol` inventory (Task 5) and the `missing_docs` report driven to zero (Task 5, enforced by CI thereafter).
