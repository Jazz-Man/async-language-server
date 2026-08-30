# Refactoring Cycle 2 — hygiene and docs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zero lint-suppression debt — `grep -rn 'allow(' src examples` returns nothing — plus a real README, the error rule reconciled with the wire layer, and I2/I8/M5/M6/M8/I9 closed. Per spec `docs/superpowers/specs/2026-08-29-refactoring-cycle2-design.md`.

**Architecture:** Nine small tasks over the existing crate: rule amendment, resolve-hook wiring, cast safety, dead-code/mechanics, rootUri removal, style wave, matcher privatization, README, tests+battery. Three public breaking changes (named in the closing report).

**Tech Stack:** Rust 2024, MSRV 1.88; clippy pedantic `-D warnings` gates CI; empirical hit-lists in this plan were captured with `--force-warn` over the current module allows on the post-Cycle-1 tree.

## Global Constraints

- Spec decisions D1–D8 bind every task. **No `#[allow]` may exist anywhere in `src/` or `examples/` at cycle end** — deletions only.
- `.claude/rules/error-handling.md` (as amended by Task 1) is normative for touched code; `.claude/rules/tech.md` battery gates the cycle.
- **No git operations in tasks.** The agent never runs git write commands; commits are entirely the user's. Tasks end at verified-green.
- All code/comments English; rustfmt-clean; tests inline per module; every task compiles under `--no-default-features` (and the tree-sitter corner config where tree code is touched).
- TDD where a behavior changes (Tasks 2, 3, 5, 6-rpc, 7, 9); lint-oracle verification (remove allow → clippy must stay silent in ALL configs) where the change is debt-shedding.

---

### Task 1: Rule reconciliation — wire-layer carve-out

**Files:**
- Modify: `.claude/rules/error-handling.md` ("One boundary, both directions" section)

**Interfaces:** none (documentation rule).

- [ ] **Step 1: Amend the rule**

In `.claude/rules/error-handling.md`, after the first bullet of "One boundary, both directions", insert:

```markdown
- The wire adapter — `LanguageServerWithState` and the workspace-diagnostics
  request layer — is itself the boundary and constructs protocol-native
  `ResponseError` values directly (the staleness `CONTENT_MODIFIED` reply,
  the workspace-diagnostics-disabled `METHOD_NOT_FOUND` reply). Domain code
  below it stays `ServerError`-only.
```

- [ ] **Step 2: Verify consistency**

Confirm the section now reads: `From<ServerError>` impl is the conversion for domain errors; the adapter's protocol-native replies are the edge itself; `oneshot` converts the other way. No contradictions with other sections.

- [ ] **Step 3: Report** (no code, no tests)

---

### Task 2: Wire resolve hooks (I2-minimal) + shed `requests.rs` allows

**Files:**
- Modify: `src/server_with_state.rs` (add two hand-wired methods after `workspace_diagnostic`, ~line 306; extend the `async_lsp::lsp_types` import list with `CodeAction`, `CompletionItem`)
- Modify: `src/requests.rs` (two new conversion helpers; underscore-prefix unused default params; remove the two trait-level allows)
- Modify: `src/server_trait.rs` (doc notes on `completion_resolve` / `code_action_resolve`)

**Interfaces:**
- Consumes: `Request` impls `CompletionResolve`/`CodeActionResolve` (`src/requests.rs`) with their existing `modify_response` bodies.
- Produces: `pub(crate) fn convert_completion_resolve(state: &ServerState, response: &mut LspCompletionItem)` and `pub(crate) fn convert_code_action_resolve(state: &ServerState, response: &mut LspCodeAction)` in `src/requests.rs`.

- [ ] **Step 1: Write the failing tests** (append to `src/requests.rs` tests):

```rust
    #[test]
    fn resolve_edits_convert_against_the_sole_tracked_document() {
        // Exactly one tracked document ("🙂abc"), UTF-16 negotiated.
        let mut state = ServerState::new::<TestServer>(ClientSocket::new_closed());
        state.set_position_encoding(Encoding::UTF16);
        open_document(&mut state, url("only.txt"), "🙂abc");

        let mut item = CompletionItem {
            label: "item".into(),
            text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
                r(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        super::convert_completion_resolve(&state, &mut item);

        let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, r(0, 2, 2));
    }

    #[test]
    fn resolve_edits_pass_through_without_a_sole_document() {
        // Two tracked documents: the sole-document rule does not apply.
        let (state, _, _) = state_with_documents();

        let mut item = CompletionItem {
            label: "item".into(),
            text_edit: Some(LspCompletionTextEdit::Edit(TextEdit::new(
                r(0, 4, 4),
                "x".into(),
            ))),
            ..Default::default()
        };

        super::convert_completion_resolve(&state, &mut item);

        let Some(LspCompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range, r(0, 4, 4));
    }
```

(The tests module already imports `ServerState`, `ClientSocket`, `Encoding`, `CompletionItem`, `TextEdit`, and the `url`/`open_document`/`r` helpers — same shapes as the existing conversion tests.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib requests::tests::resolve`
Expected: compile error — `convert_completion_resolve` does not exist.

- [ ] **Step 3: Implement the helpers** (in `src/requests.rs`, after the `CodeActionResolve` impl):

```rust
/// Converts a resolve response's edits against the sole tracked document.
///
/// Resolve requests carry no document URL, so there is no request document:
/// when exactly one document is tracked (the normal completion-then-resolve
/// flow), its snapshot converts the edits; otherwise the response passes
/// through unchanged.
pub(crate) fn convert_completion_resolve(
    state: &ServerState,
    response: &mut LspCompletionItem,
) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let documents = state.documents();
    let [document] = documents.as_slice() else {
        return;
    };
    <CompletionResolve as Request>::modify_response(state, document, response);
}

pub(crate) fn convert_code_action_resolve(state: &ServerState, response: &mut LspCodeAction) {
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let documents = state.documents();
    let [document] = documents.as_slice() else {
        return;
    };
    <CodeActionResolve as Request>::modify_response(state, document, response);
}
```

- [ ] **Step 4: Hand-wire the two methods** (in `src/server_with_state.rs`, after `workspace_diagnostic`):

```rust
    fn completion_item_resolve(
        &mut self,
        params: CompletionItem,
    ) -> BoxFuture<'static, Result<CompletionItem, Self::Error>> {
        let server = Arc::clone(&self.server);
        let state = self.state.clone();
        Box::pin(async move {
            let mut result = server.completion_resolve(state.clone(), params).await?;
            crate::requests::convert_completion_resolve(&state, &mut result);
            Ok(result)
        })
    }

    fn code_action_resolve(
        &mut self,
        params: CodeAction,
    ) -> BoxFuture<'static, Result<CodeAction, Self::Error>> {
        let server = Arc::clone(&self.server);
        let state = self.state.clone();
        Box::pin(async move {
            let mut result = server.code_action_resolve(state.clone(), params).await?;
            crate::requests::convert_code_action_resolve(&state, &mut result);
            Ok(result)
        })
    }
```

Remove the corresponding two lines (`completion_item_resolve`, `code_action_resolve`) from the `implement_methods!` table.

- [ ] **Step 5: Shed `requests.rs` allows + underscore defaults**

Remove `#[allow(dead_code)]` and `#[allow(unused_variables)]` from the `Request` trait (`src/requests.rs:32-33`). In the trait's default signatures, underscore-prefix the unused parameters:

```rust
    fn extract_url(_params: &Self::Params) -> Option<Url> {
        None
    }

    fn modify_params(_state: &ServerState, _document: &Document, _params: &mut Self::Params) {}
    fn modify_response(_state: &ServerState, _document: &Document, _response: &mut Self::Response) {}
```

- [ ] **Step 6: Document the contract** on the trait (`src/server_trait.rs`): extend the `completion_resolve` and `code_action_resolve` doc comments with one sentence each: "Positions in returned edits are converted to the negotiated encoding against the sole tracked document, when exactly one is open; otherwise they are returned unchanged."

- [ ] **Step 7: Verify**

Run: `cargo test --lib requests && cargo clippy --all-targets -- -D warnings && cargo test --no-default-features`
Expected: tests pass (new pair green), clippy silent with both trait allows gone.

---

### Task 3: Cast safety — five sites

**Files:**
- Modify: `src/tree_sitter_utils.rs:20-26` (`ts_point_to_lsp_position`)
- Modify: `src/text_utils/position.rs:38-45` (`Position::into_lsp`)
- Modify: `src/text_utils/range_ext/lsp.rs:30-46` (`shrink`) and `:116-117` (`sub_delimited` helper)
- Modify: `examples/minimal.rs:39-66` (diagnostics loop)

**Interfaces:** none change (same signatures; `const` may drop from two fns if the fix is not const-callable — acceptable, no internal const-context callers).

- [ ] **Step 1: Replace truncating casts with checked conversion + saturating fallback**

Every `x as u32` over `usize` becomes `u32::try_from(x).unwrap_or(u32::MAX)` — a >4-billion row/column/line cannot occur in practice, and saturating is the crate's existing policy for out-of-range positions.

`src/tree_sitter_utils.rs` (also remove the `#[allow]` at :21):

```rust
pub const fn ts_point_to_lsp_position(pos: TsPoint) -> LspPosition {
    LspPosition {
        line: u32::try_from(pos.row).unwrap_or(u32::MAX),
        character: u32::try_from(pos.column).unwrap_or(u32::MAX),
    }
}
```

If `unwrap_or` is not const-callable on the pinned toolchain, drop `const` from this fn and `Position::into_lsp` and note it in the report.

`src/text_utils/position.rs` (also remove the `#[allow]` at :40):

```rust
    pub const fn into_lsp(self) -> LspPosition {
        LspPosition {
            line: u32::try_from(self.line).unwrap_or(u32::MAX),
            character: u32::try_from(self.col).unwrap_or(u32::MAX),
        }
    }
```

`src/text_utils/range_ext/lsp.rs` `shrink` (remove the `#[allow]` at :30):

```rust
        let start_char = self
            .start
            .character
            .saturating_add(u32::try_from(amount_left).unwrap_or(u32::MAX))
            .min(self.end.character);
        let end_char = self
            .end
            .character
            .saturating_sub(u32::try_from(amount_right).unwrap_or(u32::MAX))
            .max(self.start.character);
```

`src/text_utils/range_ext/lsp.rs:116` (remove the `#[allow]`):

```rust
            let character = u32::try_from(text[line_byte..offset].chars().count())
                .unwrap_or(u32::MAX);
```

`examples/minimal.rs` (remove the `#[allow]` at :39; inside the loop):

```rust
                items.push(Diagnostic {
                    range: Range::new(
                        Position::new(u32::try_from(line).unwrap_or(u32::MAX), 0),
                        Position::new(
                            u32::try_from(line).unwrap_or(u32::MAX),
                            u32::try_from(length).unwrap_or(u32::MAX),
                        ),
                    ),
```

(`line` comes from `.enumerate()` over `lines()`; `length` is `text.len()` — both `usize`.)

- [ ] **Step 2: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo test --no-default-features --features tree-sitter`
Expected: all green; `grep -n 'cast_possible_truncation' src examples` → 0.

---

### Task 4: Mechanics + dead code (incl. I8)

**Files:**
- Modify: `src/text_utils/range_ext/mod.rs:105-177` (remove two default method bodies + their allows)
- Modify: `src/server_with_state.rs:280` (drop vestigial allow)
- Modify: `src/document_matcher.rs:95,102` (drop vestigial allows)
- Modify: `src/server_state.rs:40,96` (drop vestigial allows), `:117` and `:540` (drop `<T: Server>` generics)

**Interfaces:**
- Produces (breaking): `RangeExt::sub_delimited` / `sub_delimited_tri` become required methods (no default bodies).
- `insert_document` and `handle_document_save` lose their `<T: Server>` parameter (both `pub(crate)`; call sites drop `::<T>`).

- [ ] **Step 1: Remove the `RangeExt` panicking defaults (I8)**

In `src/text_utils/range_ext/mod.rs`, replace each of the two default bodies (`unimplemented!(...)` + `#[allow(unused_variables)]`) with nothing — the method signature and doc comment stay, the `;`-terminated form makes them required:

```rust
    fn sub_delimited(self, text: &str, delimiter: char) -> (Option<Self>, Option<Self>);

    fn sub_delimited_tri(
        self,
        text: &str,
        delim0: char,
        delim1: char,
    ) -> (Option<Self>, Option<Self>, Option<Self>);
```

The crate's own impls (`bytes.rs`, `lsp.rs`, `tree_sitter.rs`) already implement both; the doc examples call them through concrete ranges and keep compiling.

- [ ] **Step 2: Drop vestigial allows**

Remove `#[allow(dead_code)]` at `src/document_matcher.rs:95,102` and `src/server_state.rs:40,96`, and `#[allow(unused_variables)]` at `src/server_with_state.rs:280`. Usage evidence: `DocumentMatchers::{new, find, find_url}` all have live call sites (`src/server_state.rs:129,145,256,340,567`, `src/oneshot/workspace_diagnostics.rs:267`); `did_close`'s param is already `_params`.

- [ ] **Step 3: Drop unused type parameters**

`src/server_state.rs:117` → `pub(crate) fn insert_document(&self, url: Url, text: String, version: i32, language: String, origin: DocumentOrigin)`; `src/server_state.rs:540` → `pub(crate) fn handle_document_save(&self, params: DidSaveTextDocumentParams) -> ControlFlow<Result<()>>` (remove `#[allow(clippy::extra_unused_type_parameters)]` at both). Update all `::<T>` call sites to plain calls.

- [ ] **Step 4: Verify (all configs — the dead-code oracle)**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --no-default-features && cargo test --all-features`
Expected: all green with the allows gone; if `dead_code` fires in ANY config, fix the flagged member (wire or delete) rather than restoring the allow.

---

### Task 5: Remove the `rootUri` fallback

**Files:**
- Modify: `src/server_with_state.rs:100-122` (`workspace_folders`)

**Interfaces:** none (private fn; behavior change: clients sending no `workspaceFolders` get no roots).

- [ ] **Step 1: Write the failing test** (append to `src/server_with_state.rs` tests; it pins the new behavior — no folders ⇒ no workspace items, where today the `root_uri` fallback would still produce items):

```rust
    #[test]
    fn initialize_without_workspace_folders_reports_no_items() {
        let root = temp_workspace("no-folders");
        fs::write(root.join("a.test"), "disk").expect("test file can be written");

        let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
        let mut params = initialize_params(&root);
        params.workspace_folders = None; // client sends neither folders nor rootUri
        futures::executor::block_on(server.initialize(params))
            .expect("server can initialize");

        let report =
            futures::executor::block_on(server.workspace_diagnostic(workspace_diagnostic_params()))
                .expect("workspace diagnostics can be fetched");

        let WorkspaceDiagnosticReportResult::Report(report) = report else {
            panic!("expected full workspace diagnostic report");
        };
        assert!(report.items.is_empty());

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
```

Note: the test must FAIL before Step 2 — today the `root_uri` fallback derives a folder from `params.root_uri`… `initialize_params` sets no `root_uri`, so with folders nulled the fallback ALSO yields nothing today. To make the RED real, set `params.root_uri = Some(workspace_folder(&root).uri);` in the pre-Step-2 run (assert non-empty then), then remove that line in the same edit as Step 2 so the final code has zero `root_uri` mentions. Simpler equivalent: write the test WITHOUT the root_uri line, verify it passes AFTER Step 2, and capture the RED by temporarily setting `params.root_uri` in a scratch run (documented in the report) — either way the committed test contains no `root_uri`.

- [ ] **Step 2: Simplify the function**

```rust
fn workspace_folders(params: &InitializeParams) -> Vec<WorkspaceFolder> {
    params.workspace_folders.clone().unwrap_or_default()
}
```

The `#[allow(deprecated)]`, the `root_uri` read, and the name-derivation closure all go. Remove `root_uri`-only mentions from the import list if it becomes unused (`InitializeParams` stays).

- [ ] **Step 3: Verify**

Run: `cargo test --lib server_with_state && cargo clippy --all-targets -- -D warnings`
Expected: green; `grep -rn 'allow(deprecated)' src` → 0 and `grep -rn 'root_uri' src` → 0 (the committed test contains no `root_uri` mention, per the Step 1 note).

---

### Task 6: Style wave — module allows off, `rpc` signature, one split

**Files:**
- Modify: `src/server_trait.rs:1-3` (drop three module allows; remove unused `Diagnostic`,`Url` imports at :9; add `#[must_use]` ×3)
- Modify: `src/server_state.rs:1-2` (drop two module allows; `with_options` takes `&ServerOptions`; split `handle_document_change`)
- Modify: `src/error.rs:1` (drop module allow; `rpc` takes `String`)

**Interfaces (breaking, public):** `ServerError::rpc(code, message: String)`; `pub(crate)` `with_options(client: ClientSocket, options: &ServerOptions)`.

- [ ] **Step 1: `server_trait.rs`**

Delete lines 1-3 (`#![allow(unused_imports)]`, `#![allow(unused_variables)]`, `#![allow(clippy::must_use_candidate)]`). Remove `Diagnostic` and `Url` from the `async_lsp::lsp_types` import (line 9; they are unused — verified via `--force-warn unused_imports`). Add `#[must_use]` above `server_info` (:38), `server_capabilities` (:52), and `server_document_matchers` (:59) — exactly the three `must_use_candidate` hits.

- [ ] **Step 2: `error.rs` — `rpc` owns its message**

```rust
    /// Creates a JSON-RPC error with the given code and message.
    #[must_use]
    pub fn rpc(code: ServerErrorCode, message: String) -> Self {
        ServerError::Rpc { code, message }
    }
```

Drop `#![allow(clippy::needless_pass_by_value)]` (line 1). Call sites: `src/oneshot/server.rs:120` already passes an owned `String` (`error.message`) — unchanged; `src/server_trait.rs` `method_not_implemented` passes `format!(...)` — unchanged. Update the test `rpc_errors_map_to_their_own_code` to `ServerError::rpc(ErrorCode::METHOD_NOT_FOUND, "nope".into())`.

- [ ] **Step 3: `server_state.rs` — `with_options` by reference**

```rust
    pub(crate) fn with_options<T: Server>(client: ClientSocket, options: &ServerOptions) -> Self {
```

Drop `#![allow(clippy::needless_pass_by_value)]` and `#![allow(clippy::too_many_lines)]` (lines 1-2). Update the two call sites: `src/server_with_state.rs:138` → `ServerState::with_options::<T>(client, &options)`; the test at `src/server_state.rs:770` → pass `&…` of its options value.

- [ ] **Step 4: Split `handle_document_change` (the one `too_many_lines` hit, 130/100)**

Extract the incremental-fallback tail (the `if incremental_update_failed { … }` block through the end, including the keep-last-known branch and the tree re-parse fix from Cycle 1) into a private method:

```rust
    /// Recovers a document whose incremental update failed: reload from
    /// disk when possible, otherwise keep the last-known text (and re-parse
    /// its tree under the tree-sitter feature).
    fn recover_failed_incremental_update(&mut self, uri: Url, version: i32, language: String)
```

Move the block's body there (adjusting `doc`-local reads to the parameters), call it from `handle_document_change`. Target: both functions under 100 lines.

- [ ] **Step 5: Verify**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --no-default-features`
Expected: green with all six module allows gone; `grep -c '^#!\[allow' src` → 0.

---

### Task 7: M6 — privatize `DocumentMatcher` fields

**Files:**
- Modify: `src/document_matcher.rs` (fields → `pub(crate)`; add `pub fn name()`; rewrite the doctest)
- Modify: internal field readers if the compiler points at any outside the module (expected: `src/document.rs` `matched_name`)

**Interfaces (breaking, public):** fields no longer public; construction via `new`/`with_*` only; `pub fn name(&self) -> &str` added.

- [ ] **Step 1: Rewrite the struct**

```rust
#[derive(Debug, Default, Clone)]
pub struct DocumentMatcher {
    /// The name of the document matcher.
    name: String,
    /// Optional globs to match documents based on their URLs.
    url_globs: Vec<String>,
    /// Strings to match documents based on their language identifiers.
    lang_strings: Vec<String>,
    /// The tree-sitter language grammar to associate with the matched document.
    #[cfg(feature = "tree-sitter")]
    lang_grammar: Option<Language>,
}

impl DocumentMatcher {
    /// Returns the matcher's name.
    ///
    /// The name is exposed on matched documents through
    /// [`crate::server::Document::matched_name`]; it does not
    /// need to be unique.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    // … new / with_url_globs / with_lang_strings / with_lang_grammar unchanged …
}
```

- [ ] **Step 2: Fix internal readers and the doctest**

Field readers inside `document_matcher.rs` (`DocumentMatchers::new`) keep working (same module). `src/document.rs` `matched_name` (reads `matcher.name` directly) switches to `matcher.name()`. The module doctest (`assert_eq!(matcher.name, "json")` …) rewrites builder-only:

```rust
/// let matcher = DocumentMatcher::new("json")
///     .with_url_globs(["**/*.json", "*.jsonc"])
///     .with_lang_strings(["json", "jsonc"]);
///
/// assert_eq!(matcher.name(), "json");
```

- [ ] **Step 3: Verify**

Run: `cargo test && cargo doc --no-deps` (doctest + docs check) `&& cargo clippy --all-targets -- -D warnings`
Expected: green; no public fields remain (`cargo doc` output lists only the methods).

---

### Task 8: README rewrite + M8

**Files:**
- Modify: `README.md` (full rewrite; it is the rendered crate documentation via `include_str!`)
- Modify: `src/transport.rs:43-46` (`# Errors` wording)

**Interfaces:** none.

- [ ] **Step 1: Rewrite `README.md`** with exactly this content:

```markdown
# async-language-server

A higher-level abstraction over [async-lsp] for writing language servers with
less boilerplate: tokio stdio/TCP transports, ropey-based incremental document
sync, automatic position-encoding negotiation (UTF-8/16/32), optional
[tree-sitter] integration, and workspace-wide diagnostics.

## Quick start

Implement the `Server` trait with only the methods you need and run it:

```rust,no_run
use std::future::Future;

use async_language_server::lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use async_language_server::server::{
    DocumentMatcher, Server, ServerResult, ServerState, Transport, serve,
};

#[derive(Clone)]
struct MyServer;

impl Server for MyServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("my-lang").with_lang_strings(["mylang"])]
    }

    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send {
        // Your analysis: `state.document(&params.text_document.uri)` returns
        // the document snapshot; produce diagnostics for it.
        std::future::ready(Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            }),
        )))
    }
}

#[tokio::main]
async fn main() -> ServerResult<()> {
    serve(Transport::Stdio, MyServer).await
}
```

Every `Server` method receives and produces **UTF-8** positions regardless of
the encoding negotiated with the client — conversions are handled internally.

## Tour

- **`Server` trait** (`server::Server`) — optional async handlers: hover,
  completion, definition, references, rename, formatting, diagnostics, …
  Unimplemented methods answer `METHOD_NOT_FOUND`.
- **`DocumentMatcher`** — associates documents with a language by URL globs
  and/or language-id strings, optionally carrying a tree-sitter grammar
  (language-per-document).
- **`serve()` + `Transport`** — wires your server into async-lsp behind a
  tower middleware stack (tracing, concurrency limit, panic catching,
  client-process monitor) over stdio or a TCP socket.
- **Workspace diagnostics** — `workspace/diagnostic` with walker-based
  scanning; exposure configured through `ServerOptions`.
- **`oneshot`** — run a `Server` over files on disk with no LSP client:
  CLI-style batch diagnostics.
- **`text_utils`** — `Encoding`, `Position`, and range helpers behind the
  transparent encoding conversion.

## Feature flags

Both default on: `tracing` (middleware + handler logging) and `tree-sitter`
(per-document grammars, `tree_sitter_utils`).

## Stability

This crate is the owner's fork of an upstream framework: version 0.0.0, not
published to crates.io, and consumed by pinning a revision or tag. Breaking
changes are named in commit messages; there is no semver safety net.

[async-lsp]: https://crates.io/crates/async-lsp
[tree-sitter]: https://tree-sitter.github.io/tree-sitter/
```

- [ ] **Step 2: Fix `# Errors` in `src/transport.rs`**

```rust
    /// # Errors
    ///
    /// - If the `Socket` transport is used and connecting to
    ///   `127.0.0.1:{port}` fails.
```

- [ ] **Step 3: Verify**

Run: `cargo test` (README fences are doctests — the quick start must compile) `&& cargo doc --no-deps`
Expected: green; the rendered crate page now shows the full API tour.

---

### Task 9: Pinning test + cycle verification

**Files:**
- Modify: `src/server_with_state.rs` (extend the encoding test)

**Interfaces:** none.

- [ ] **Step 1: Extend `initialize_ignores_unknown_client_encodings`**

Add a second scenario in the same test (new `server`/`params`): client advertises only `PositionEncodingKind::new("utf-7")` → assert `result.capabilities.position_encoding == Some(PositionEncodingKind::UTF16)` (pins `Encoding::default()`).

- [ ] **Step 2: Cycle acceptance run**

```bash
cargo build --all-targets
cargo test
cargo test --no-default-features
cargo test --all-features
cargo test --no-default-features --features tree-sitter
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
grep -rn 'allow(' src examples            # expected: 0 lines
grep -rn 'root_uri' src                   # expected: 0 lines
```

Any failure → `no-workarounds` + `superpowers:systematic-debugging` (invoke both skills, root-cause); never re-add an allow.

- [ ] **Step 3: Rule-compliance pass + report**

Check every touched file against `.claude/rules/error-handling.md` (as amended). Report battery results, the zero-allow grep outputs, and name the three breaking changes for the user's commit message: `ServerError::rpc(code, String)`, `RangeExt` required methods, `DocumentMatcher` private fields.
