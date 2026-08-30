# Polish Cycle — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the six deferred polish items — one behavioral hardening (resolve snapshot taken once) and five mechanical cleanups — per spec `docs/superpowers/specs/2026-08-30-polish-cycle-design.md`.

**Architecture:** Four small TDD tasks over the committed tree (baseline `915d5b8`+): resolve converters take a captured `Option<&Document>`; test fixture and constructor cleanups; one extraction; two rule lines. No public API changes.

**Tech Stack:** Rust 2024, MSRV 1.88; clippy pedantic `-D warnings`; zero lint allows (keep it zero).

## Global Constraints

- Spec D1–D6 bind every task. **Zero `#[allow]` anywhere in `src/` and `examples/` — no new ones.**
- **No git operations in tasks.** The agent never runs git write commands; commits are entirely the user's. Tasks end at verified-green.
- English; rustfmt-clean; every task compiles under `--no-default-features` (and the tree-sitter corner config where tree code is touched).
- TDD for Task 1 (behavior change); oracle verification (tests stay green + stated greps) for mechanical tasks.

---

### Task 1: Resolve snapshot taken once (D1)

**Files:**
- Modify: `src/server_with_state.rs` (both hand-wired resolve methods; tests: new end-to-end pair; TestServer gains an echo impl)
- Modify: `src/requests.rs` (all four converters take `Option<&Document>`; the three helper-level tests update call sites)

**Interfaces:**
- Produces (`pub(crate)`, signature change): `convert_incoming_completion_resolve(state: &ServerState, document: Option<&Document>, response: &mut LspCompletionItem)`, `convert_completion_resolve(state: &ServerState, document: Option<&Document>, response: &mut LspCompletionItem)`, and the two `code_action` twins. The sole-document pick moves to the call sites.

- [ ] **Step 1: Update the converter tests** (they define the new signatures; RED = compile error)

In `src/requests.rs` tests, update the three resolve tests to pass the document explicitly — sole-doc tests:

```rust
        let document = state.document(&url("only.txt")).expect("sole document is tracked");
        super::convert_incoming_completion_resolve(&state, Some(&document), &mut item);
        // … assertions unchanged …
        super::convert_completion_resolve(&state, Some(&document), &mut item);
```

the pass-through test becomes the `None` case:

```rust
    #[test]
    fn resolve_edits_pass_through_without_a_document() {
        let (state, _, _) = state_with_documents();

        let mut item = CompletionItem { /* unchanged fixture */ };

        super::convert_completion_resolve(&state, None, &mut item);

        /* assert unchanged range */
    }
```

and the round-trip test captures one snapshot and uses it for both directions:

```rust
        let sole = state.document(&url("only.txt")).expect("sole document is tracked");
        super::convert_incoming_completion_resolve(&state, sole.as_ref(), &mut item);
        super::convert_completion_resolve(&state, sole.as_ref(), &mut item);
        // assert the edit is unchanged (0,2,2)
```

- [ ] **Step 2: Write the end-to-end pick test** (append to `src/server_with_state.rs` tests; TestServer needs an echo impl first — add to the existing `impl Server for TestServer`):

```rust
        fn completion_resolve(
            &self,
            _state: ServerState,
            item: CompletionItem,
        ) -> impl Future<Output = ServerResult<CompletionItem>> + Send {
            std::future::ready(Ok(item))
        }
```

```rust
    #[test]
    fn resolve_converts_with_sole_document_and_passes_through_with_two() {
        let root = temp_workspace("resolve-pick");
        let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);
        let mut params = initialize_params(&root);
        params.capabilities.general = Some(GeneralClientCapabilities {
            position_encodings: Some(vec![PositionEncodingKind::UTF16]),
            ..Default::default()
        });
        futures::executor::block_on(server.initialize(params)).expect("server can initialize");

        let first = Url::from_file_path(root.join("a.test")).expect("path can be converted to a URL");
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(first, "test".into(), 1, "🙂abc".into()),
        });

        let echo = CompletionItem {
            label: "x".into(),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                Range::new(Position::new(0, 2), Position::new(0, 2)),
                "y".into(),
            ))),
            ..Default::default()
        };

        // Sole document: the echo handler receives UTF-8 (0,4) and the wire
        // shows UTF-16 (0,2) — identity for the client. Missing either
        // converter breaks this arm (one-way conversion shifts the column).
        let resolved = futures::executor::block_on(server.completion_item_resolve(echo.clone()))
            .expect("resolve succeeds");
        let Some(CompletionTextEdit::Edit(edit)) = resolved.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range.start, Position::new(0, 2));

        // Second document: no sole document, both converters pass through.
        let second = Url::from_file_path(root.join("b.test")).expect("path can be converted to a URL");
        let _ = server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(second, "test".into(), 1, "🙂def".into()),
        });
        let resolved = futures::executor::block_on(server.completion_item_resolve(echo))
            .expect("resolve succeeds");
        let Some(CompletionTextEdit::Edit(edit)) = resolved.text_edit else {
            panic!("expected edit");
        };
        assert_eq!(edit.range.start, Position::new(0, 2));

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
```

(Extend the test module's `lsp_types` imports with `CompletionItem`, `CompletionTextEdit`, `TextEdit`, `TextDocumentItem`, `DidOpenTextDocumentParams` as needed — several are already imported. Note: identity in both arms cannot by itself detect BOTH converters no-op'ing — that case is covered by the per-converter `Some(&document)` tests in Step 1, which assert each direction shifts the column. Do not drop those when touching this file.)

- [ ] **Step 3: Implement the snapshot**

In `src/server_with_state.rs`, both hand-wired methods take `mut params` and gain, before the handler call:

```rust
        let sole = {
            let documents = state.documents();
            (documents.len() == 1).then(|| documents[0].clone())
        };
```

then `crate::requests::convert_incoming_completion_resolve(&state, sole.as_ref(), &mut params);` before, `crate::requests::convert_completion_resolve(&state, sole.as_ref(), &mut result);` after (same shape for `code_action_resolve`).

- [ ] **Step 4: Retarget the converters**

All four in `src/requests.rs`: new `document: Option<&Document>` parameter; the internal `let documents = state.documents(); let [document] = … else { return; };` pair becomes:

```rust
    let Some(document) = document else {
        return;
    };
```

(the UTF-8 early return on `state` stays first).

- [ ] **Step 5: Verify**

Run: `cargo test --lib requests server_with_state && cargo clippy --all-targets -- -D warnings && cargo test --no-default-features`
Expected: all green; the new end-to-end test passes in both arms.

---

### Task 2: Mechanics — fixture, `Encoding` shadow, test constructor (D2–D4)

**Files:**
- Modify: `src/text_utils/encoding.rs` (delete the inherent `const fn default`; `impl Default` body)
- Modify: `src/server_state.rs` (tests: inline `with_options` at the `ServerState::new::<…>` sites; delete the `#[cfg(test)]` constructor; ensure `ServerOptions` import)
- Modify: `src/requests.rs` (tests: same inlining at its three sites)

**Interfaces:** none public (`Default::default()` remains; const-callability of `Encoding::default()` is dropped, no const callers exist — spec D3).

- [ ] **Step 1: `Encoding` — remove the shadow**

Delete from `src/text_utils/encoding.rs`:

```rust
    /// Returns the LSP default encoding, [`Encoding::UTF16`].
    #[must_use]
    pub const fn default() -> Self {
        Self::UTF16
    }
```

and make the trait impl direct:

```rust
impl Default for Encoding {
    fn default() -> Self {
        Self::UTF16
    }
}
```

The doctest's `assert_eq!(Encoding::default(), Encoding::UTF16);` keeps compiling through the trait.

- [ ] **Step 2: Inline the test constructor**

Replace every `ServerState::new::<TestServer>(ClientSocket::new_closed())` / `::new::<JsonServer>(…)` (ten sites: `src/server_state.rs` ×7, `src/requests.rs` ×3) with:

```rust
ServerState::with_options::<TestServer>(ClientSocket::new_closed(), &ServerOptions::default())
```

(matching each site's server type), then delete the `#[cfg(test)]`-gated `new` constructor. Add the `ServerOptions` import to the tests modules if missing.

- [ ] **Step 3: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: green; `grep -n 'pub const fn default' src/text_utils/encoding.rs` → 0; `grep -n 'cfg(test)' src/server_state.rs` shows only the tests module gate.

---

### Task 3: Headroom — extract the tree-sitter edit block (D5)

**Files:**
- Modify: `src/server_state.rs` (`handle_document_change`)

**Interfaces:** none (private helper).

- [ ] **Step 1: Extract**

Move the `#[cfg(feature = "tree-sitter")]` `InputEdit`-construction block out of `handle_document_change`'s per-change loop into a private helper placed next to `change_char_range`:

```rust
    /// Builds the tree-sitter incremental edit for one content change,
    /// returning `None` when the changed range cannot be resolved.
    #[cfg(feature = "tree-sitter")]
    fn tree_sitter_edit(
        doc: &Document,
        change: &TextDocumentContentChangeEvent,
        encoding: Encoding,
    ) -> Option<tree_sitter::InputEdit> {
        // … the moved block, `None` where the loop currently skips …
    }
```

(The exact parameter set follows the block's actual reads — adjust to what it uses; keep the call order identical so the Cycle-1 regression tests still exercise the moved code. Both functions must land well under 100 counted lines.)

- [ ] **Step 2: Verify**

Run: `cargo test && cargo test --no-default-features --features tree-sitter && cargo clippy --all-targets -- -D warnings`
Expected: green in both configs; parent function comfortably under 80 counted lines.

---

### Task 4: Rule wording (D6)

**Files:**
- Modify: `.claude/rules/error-handling.md` (two lines)

**Interfaces:** none.

- [ ] **Step 1: Edit the two lines**

Line ~52: `All upstream code — trait impls, state, walkers — returns` → `All domain code — trait impls, state, walkers — returns`.
Line ~56: `(the staleness `CONTENT_MODIFIED` reply,` → `(the staleness `CONTENT_MODIFIED` replies,`.

- [ ] **Step 2: Cycle acceptance run**

```bash
cargo build --all-targets && cargo test && cargo test --no-default-features \
  && cargo test --all-features && cargo test --no-default-features --features tree-sitter \
  && cargo fmt --check && cargo clippy --all-targets -- -D warnings \
  && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
grep -rn 'allow(' src examples   # expected: 0 lines
```

Any failure → `no-workarounds` + `superpowers:systematic-debugging`; never suppress.

- [ ] **Step 3: Report**

Battery results, the grep, rule-compliance pass. No breaking changes to name this time — commits are the user's.
