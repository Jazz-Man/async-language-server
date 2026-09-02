# LSP Surface — Plan 3: Semantic Tokens, Resolve Trio, Notifications, Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the cycle's specials — the semantic-tokens trio with a per-document UTF-8 token cache, the resolve trio, the six notification handlers, all twelve sync `Server`-trait hooks, and the registered `active_signature_help` incoming fix — closing the full LSP client→server surface.

**Architecture:** All three token requests are GENERATED registry rows naming three outgoing helpers; the delta helper seeds edit conversion from a `ServerState` cache of the last UTF-8 tokens handed out (mirroring what the server computed against), splicing edits into the cache on the UTF-8 side. Notifications stay internal (sync, in `with_state` + state methods) with additive sync trait hooks called after the internal handler. Spec: sections "Semantic tokens", "Notifications", "Architecture 1.5" of `docs/superpowers/specs/2026-09-01-lsp-surface-completion-design.md`.

**Tech Stack:** Rust edition 2024, registry rows + `conversion.rs` helpers, `DashMap` cache in `ServerState`, `conversion_tests!`/hand-written W0, capture-server dispatch tests.

## Global Constraints

- Owner commits — every task ends at a review checkpoint with a file list; no git commands anywhere.
- Rows carry `$trait : $alsp @ $Request`; types as full paths; typed defaults (`WorkDoneProgressParams::default()` etc.) in test struct literals; test modules import stamped types via `crate::requests::X`.
- Normative token semantics (LSP 3.17, quoted in the spec): `deltaStart` and `length` are encoded in the NEGOTIATED encoding; edit `start`/`deleteCount` index the FLAT u32 array and edits are interpretation-free; the 5-tuple layout is `SemanticToken { delta_line, delta_start, length, token_type, token_modifiers_bitset }` (pinned lsp-types 0.95.1, semantic_tokens.rs:146-153); results are untagged enums (`SemanticTokensResult` Tokens|Partial, `SemanticTokensFullDeltaResult` Tokens|TokensDelta|PartialTokensDelta{edits} — inline struct variant, `SemanticTokensRangeResult` Tokens|Partial).
- Trait methods always speak UTF-8: token columns and lengths the handler produces are UTF-8; conversion happens only in the outgoing helpers. `start`/`delete_count` pass through untouched (array positions, not code units).
- Cache discipline (spec, Semantic tokens): written whenever a FULL or DELTA response passes through `modify_response` (range responses convert but never cache); keyed by the request document's URL; a `didChange` does NOT invalidate it (the server's edits are against its previous UTF-8 state, which is exactly what the cache holds); on a cache miss the delta edits pass through unconverted (trace under the `tracing` feature).
- Notification hooks: `fn $name(&self, state: &ServerState, params: &$Params) {}` — sync by protocol necessity (an async hook would need spawning and break LSP message ordering), called AFTER the internal handler so hooks observe post-internal state; default bodies empty (additive); no `#[allow]`; hooks must not panic.
- All three feature configurations compile and pass; unwrap/expect only under the test exemption; `cargo dupes check` exit 0 (dissolve-first discipline; reasoned entries only for genuinely mandated parallelism).
- No per-plan final review: ONE end-of-cycle whole-branch review after this plan (base `c5cae8d`).
- The `$/` trio (setTrace, cancelRequest, progress) stays untouched — async-lsp auto-ignores the prefix; the one deliberate exception, recorded in the spec.

---

### Task 1: Token converter + state cache + `semantic_tokens_full`

**Files:**
- Modify: `src/server/state/mod.rs` (+1 field, +2 methods, +1 struct), `src/requests/conversion.rs` (converter + outgoing helper), `src/requests/registry.rs` (1 row)
- Create: `src/requests/semantic_tokens_full.rs` (tests only)

**Interfaces:**
- Consumes: `convert_position`/`position_to_encoding` machinery; registry grammar.
- Produces: `struct CachedSemanticTokens { result_id: String, data: Vec<async_lsp::lsp_types::SemanticToken> }` (private to state, `Debug + Clone`); `ServerState::cached_semantic_tokens(&self, url: &Url) -> Option<CachedSemanticTokens>` and `ServerState::store_semantic_tokens(&self, url: &Url, cached: CachedSemanticTokens)` (both `pub(crate)`); `convert_semantic_tokens_data(state: &ServerState, document: &Document, data: &mut Vec<SemanticToken>, direction: Direction)` and `modify_outgoing_semantic_tokens_result(state: &ServerState, document: &Document, response: &mut Option<SemanticTokensResult>)` in `conversion.rs`.

- [ ] **Step 1: State cache** — in `src/server/state/mod.rs`: add `use async_lsp::lsp_types::{SemanticToken, Url}` extension to the existing import (Url already imported; add SemanticToken); add the field `semantic_tokens: Arc<DashMap<Url, CachedSemanticTokens>>` to `ServerState` (init `Arc::new(DashMap::new())` in `with_options`); add:

```rust
/// The last UTF-8 semantic tokens handed out for a document, with their
/// result id — the server-side previous state that delta edits are
/// computed against.
#[derive(Debug, Clone)]
pub(crate) struct CachedSemanticTokens {
    /// The result id the response carried.
    pub(crate) result_id: String,
    /// The tokens in UTF-8 columns, exactly as the handler produced them.
    pub(crate) data: Vec<SemanticToken>,
}
```

```rust
    /// Returns the last UTF-8 semantic tokens stored for `url`.
    #[must_use]
    pub(crate) fn cached_semantic_tokens(&self, url: &Url) -> Option<CachedSemanticTokens> {
        self.semantic_tokens.get(url).map(|entry| entry.clone())
    }

    /// Stores the last UTF-8 semantic tokens for `url`, replacing any
    /// previous entry. Written whenever a full or delta response passes
    /// through the outgoing conversion; `didChange` does not invalidate —
    /// the server's own edits are against this state.
    pub(crate) fn store_semantic_tokens(&self, url: &Url, cached: CachedSemanticTokens) {
        self.semantic_tokens.insert(url.clone(), cached);
    }
```

- [ ] **Step 2: Converter** — in `src/requests/conversion.rs`:

```rust
/// Converts a semantic-token stream's `delta_start` and `length` columns
/// between the negotiated encoding and UTF-8, in place. `delta_line`
/// values are encoding-independent line deltas and pass through.
///
/// Tokens are relative: `delta_start` counts from the previous token's
/// start on the same line, or from 0 on a new line. The walk reconstructs
/// each token's absolute source-encoding position, converts it (and the
/// position after the token's length) through the document rope, and
/// re-relativizes against the previous CONVERTED token.
pub(crate) fn convert_semantic_tokens_data(
    state: &ServerState,
    document: &Document,
    data: &mut [LspSemanticToken],
    direction: Direction,
) {
    let (source, target) = match direction {
        Direction::Incoming => (state.get_position_encoding(), Encoding::UTF8),
        Direction::Outgoing => (Encoding::UTF8, state.get_position_encoding()),
    };
    if source == target {
        return;
    }
    let mut previous_source = LspPosition { line: 0, character: 0 };
    let mut previous_target = LspPosition { line: 0, character: 0 };
    for token in data.iter_mut() {
        let absolute_source = LspPosition {
            line: previous_source.line + token.delta_line,
            character: if token.delta_line == 0 {
                previous_source.character + token.delta_start
            } else {
                token.delta_start
            },
        };
        let absolute_end_source = LspPosition {
            line: absolute_source.line,
            character: absolute_source.character + token.length,
        };
        let absolute_target = position_to_encoding(&document.text, absolute_source, source, target);
        let absolute_end_target =
            position_to_encoding(&document.text, absolute_end_source, source, target);

        token.delta_line = absolute_target.line - previous_target.line;
        token.delta_start = if absolute_target.line == previous_target.line {
            absolute_target.character - previous_target.character
        } else {
            absolute_target.character
        };
        token.length = absolute_end_target
            .character
            .saturating_sub(absolute_target.character);

        previous_source = absolute_source;
        previous_target = absolute_target;
    }
}
```

(`SemanticToken as LspSemanticToken` joins the alias import; `position_to_encoding` is already imported. Mid-character lengths floor to the containing character boundary per `position_to_encoding` — saturating subtraction keeps the invariant length ≥ 0.)

```rust
/// Converts a semanticTokens/full response to the client encoding and
/// caches the handler's UTF-8 tokens (with the result id) for later
/// delta requests.
pub(crate) fn modify_outgoing_semantic_tokens_result(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspSemanticTokensResult>,
) {
    let Some(result) = response else { return };
    let tokens = match result {
        LspSemanticTokensResult::Tokens(tokens) => tokens,
        LspSemanticTokensResult::Partial(partial) => {
            convert_semantic_tokens_data(state, document, &mut partial.data, Direction::Outgoing);
            return;
        }
    };
    convert_semantic_tokens_data(state, document, &mut tokens.data, Direction::Outgoing);
    if let Some(result_id) = tokens.result_id.clone() {
        state.store_semantic_tokens(
            document.url(),
            CachedSemanticTokens { result_id, data: tokens.data.clone() },
        );
    }
}
```

(`SemanticTokensResult as LspSemanticTokensResult` alias; `use crate::server::CachedSemanticTokens;` — export it from the `server` facade next to `Document`/`ServerState` the way `read_document_from_disk` was exported in T8.)

- [ ] **Step 3: Registry row**

```rust
semantic_tokens_full: semantic_tokens_full @ SemanticTokensFull {
    doc: "Handles `textDocument/semanticTokens/full` requests from the client.\n\nReturns the document's full semantic token stream, or `None`. Token columns and lengths are UTF-8 here and converted to the negotiated encoding on the wire. Requires a semantic tokens provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::SemanticTokensParams,
    response: Option<async_lsp::lsp_types::SemanticTokensResult>,
    document: text_document,
    outgoing: modify_outgoing_semantic_tokens_result,
}
```

- [ ] **Step 4: Tests** — `src/requests/semantic_tokens_full.rs`, hand-written (arrays are not Position rows):

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};

    use crate::requests::Request;
    use crate::testing::state_with_documents;

    use super::SemanticTokensFull;

    fn token(delta_line: u32, delta_start: u32, length: u32) -> SemanticToken {
        SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: 0,
            token_modifiers_bitset: 0,
        }
    }

    #[test]
    fn full_tokens_convert_columns_and_lengths_and_cache() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        // "🙂abc": UTF-8 bytes — token at byte 0 length 4 (the emoji),
        // token at byte 4 length 3 ("abc"). UTF-16: columns 0 and 4,
        // lengths 2 and 3.
        let mut response = Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some("r1".into()),
            data: vec![token(0, 0, 4), token(0, 4, 3)],
        }));

        <SemanticTokensFull as Request>::modify_response(&state, &document, &mut response);

        let Some(SemanticTokensResult::Tokens(tokens)) = response else {
            panic!("expected tokens");
        };
        assert_eq!(tokens.data[0].delta_start, 0);
        assert_eq!(tokens.data[0].length, 2);
        assert_eq!(tokens.data[1].delta_start, 2);
        assert_eq!(tokens.data[1].length, 3);
        // Cache keeps the handler's UTF-8 form for delta seeding.
        let cached = state.cached_semantic_tokens(&emoji).expect("cached");
        assert_eq!(cached.result_id, "r1");
        assert_eq!(cached.data[1].delta_start, 4);
        assert_eq!(cached.data[1].length, 3);
    }
}
```

(Plus one multi-line case — a token on line 1 after a token on line 0 — pinning `delta_line` pass-through and new-line `delta_start` semantics: build the emoji document's second line by extending the fixture? NO — the fixture is fixed; instead pin with the plain "abcdef" document under a UTF16 state by constructing the state manually? Simplest honest pin: a second test against the PLAIN document with `state_with_documents` asserting identity under ASCII (weaker) — SKIP the weaker test; the multi-line walk is covered structurally by the delta tests in Task 2 which use two-line content written to temp files. Record this choice.)

- [ ] **Step 5: Battery** (`cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo dupes check`; expected +1 lib test per config) and review checkpoint.

---

### Task 2: `semantic_tokens_range` + `semantic_tokens_full_delta`

**Files:**
- Modify: `src/requests/conversion.rs` (two helpers + edit-seeding walk), `src/requests/registry.rs` (2 rows)
- Create: `src/requests/semantic_tokens_range.rs`, `src/requests/semantic_tokens_full_delta.rs` (tests only)

**Interfaces:**
- Consumes: Task 1's converter + cache.
- Produces: `modify_outgoing_semantic_tokens_range_result(state, document, &mut Option<SemanticTokensRangeResult>)` (converts, NEVER caches); `modify_outgoing_semantic_tokens_delta_result(state, document, &mut Option<SemanticTokensFullDeltaResult>)` (Tokens branch converts+caches; TokensDelta/PartialTokensDelta convert each edit's inserted data seeded from the cache and splice the cache on the UTF-8 side).

- [ ] **Step 1: Range helper + row**:

```rust
/// Converts a semanticTokens/range response to the client encoding.
/// Range responses never seed the delta cache (only full and delta
/// responses do).
pub(crate) fn modify_outgoing_semantic_tokens_range_result(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspSemanticTokensRangeResult>,
) {
    let Some(result) = response else { return };
    match result {
        LspSemanticTokensRangeResult::Tokens(tokens) => {
            convert_semantic_tokens_data(state, document, &mut tokens.data, Direction::Outgoing);
        }
        LspSemanticTokensRangeResult::Partial(partial) => {
            convert_semantic_tokens_data(state, document, &mut partial.data, Direction::Outgoing);
        }
    }
}
```

(`SemanticTokensRangeResult as LspSemanticTokensRangeResult` alias.) Row:

```rust
semantic_tokens_range: semantic_tokens_range @ SemanticTokensRange {
    doc: "Handles `textDocument/semanticTokens/range` requests from the client.\n\nReturns the semantic token stream for the range in `params`, or `None`. Token columns and lengths are UTF-8 here and converted on the wire. Requires a semantic tokens provider with `range` support in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::SemanticTokensRangeParams,
    response: Option<async_lsp::lsp_types::SemanticTokensRangeResult>,
    document: text_document,
    incoming: range at range,
    outgoing: modify_outgoing_semantic_tokens_range_result,
}
```

- [ ] **Step 2: Delta helper**:

```rust
/// Converts a semanticTokens/full/delta response to the client encoding.
///
/// Edit `start`/`delete_count` index the flat number array and pass
/// through untouched. Each edit's inserted tokens are relative to the
/// token preceding the edit region in the SERVER's UTF-8 stream — the
/// cached previous result — so conversion seeds its walk from there, in
/// both the UTF-8 frame (source) and the client-encoding frame (target).
/// On a cache miss the edit passes through unconverted (traced under the
/// `tracing` feature). The cache is spliced on the UTF-8 side with the
/// ORIGINAL inserted values, keeping it equal to what the server's next
/// delta is computed against.
pub(crate) fn modify_outgoing_semantic_tokens_delta_result(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspSemanticTokensFullDeltaResult>,
) {
    let Some(result) = response else { return };
    let url = document.url();
    let cached = state.cached_semantic_tokens(&url);
    match result {
        LspSemanticTokensFullDeltaResult::Tokens(tokens) => {
            convert_semantic_tokens_data(state, document, &mut tokens.data, Direction::Outgoing);
            if let Some(result_id) = tokens.result_id.clone() {
                state.store_semantic_tokens(
                    &url,
                    CachedSemanticTokens { result_id, data: tokens.data.clone() },
                );
            }
        }
        LspSemanticTokensFullDeltaResult::TokensDelta(delta) => {
            convert_semantic_tokens_edits(state, document, cached.as_ref(), &mut delta.edits);
            if let Some(result_id) = delta.result_id.clone() {
                splice_semantic_tokens_cache(state, &url, cached.as_ref(), &delta.edits, result_id);
            }
        }
        LspSemanticTokensFullDeltaResult::PartialTokensDelta { edits } => {
            convert_semantic_tokens_edits(state, document, cached.as_ref(), edits);
        }
    }
}
```

```rust
/// Converts each edit's inserted tokens, seeded from the cached UTF-8
/// stream: the token preceding the edit region provides the relative
/// origin in both frames. Edits assume token-aligned `start` values (the
/// vscode-sample shape); a mid-token start seeds from the last fully
/// preceding token.
fn convert_semantic_tokens_edits(
    state: &ServerState,
    document: &Document,
    cached: Option<&CachedSemanticTokens>,
    edits: &mut [LspSemanticTokensEdit],
) {
    let Some(cached) = cached else {
        #[cfg(feature = "tracing")]
        tracing::debug!("semantic tokens delta without a cached previous result");
        return;
    };
    for edit in edits {
        let Some(inserted) = edit.data.as_mut() else { continue };
        // Flat-array index -> token index; the preceding token anchors
        // the inserted stream's relative columns.
        let preceding = (edit.start / 5).saturating_sub(1) as usize;
        let (seed_source, seed_target) = if preceding == 0 {
            (LspPosition { line: 0, character: 0 }, LspPosition { line: 0, character: 0 })
        } else {
            let seed_source = absolute_position(&cached.data[..preceding]);
            let seed_target =
                position_to_encoding(&document.text, seed_source, Encoding::UTF8, state.get_position_encoding());
            (seed_source, seed_target)
        };
        convert_seeded_token_stream(
            state,
            document,
            inserted,
            seed_source,
            seed_target,
        );
    }
}
```

Write `absolute_position(prefix: &[LspSemanticToken]) -> LspPosition` (fold the deltas), `convert_seeded_token_stream` (Task 1's walk factored to take initial `previous_source`/`previous_target` — REFACTOR Task 1's `convert_semantic_tokens_data` to delegate to it with zero-origins so the walk exists once), and:

```rust
/// Applies the edits to the cached UTF-8 stream with the ORIGINAL
/// (unconverted) inserted values, storing the result under `result_id`.
fn splice_semantic_tokens_cache(
    state: &ServerState,
    url: &Url,
    cached: Option<&CachedSemanticTokens>,
    edits: &[LspSemanticTokensEdit],
    result_id: String,
) {
    let Some(cached) = cached else { return };
    let mut flat: Vec<u32> = cached
        .data
        .iter()
        .flat_map(|token| {
            [token.delta_line, token.delta_start, token.length, token.token_type, token.token_modifiers_bitset]
        })
        .collect();
    // Edits are relative to the same state; apply back-to-front so
    // indices stay valid (the spec's client-side algorithm).
    let mut sorted: Vec<&LspSemanticTokensEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| edit.start);
    for edit in sorted.iter().rev() {
        let start = (edit.start as usize).min(flat.len());
        let end = (start + edit.delete_count as usize).min(flat.len());
        let inserted: Vec<u32> = edit
            .data
            .iter()
            .flat_map(|tokens| {
                tokens.iter().flat_map(|token| {
                    [token.delta_line, token.delta_start, token.length, token.token_type, token.token_modifiers_bitset]
                })
            })
            .collect();
        flat.splice(start..end, inserted);
    }
    let data = flat
        .chunks_exact(5)
        .map(|chunk| LspSemanticToken {
            delta_line: chunk[0],
            delta_start: chunk[1],
            length: chunk[2],
            token_type: chunk[3],
            token_modifiers_bitset: chunk[4],
        })
        .collect();
    state.store_semantic_tokens(url, CachedSemanticTokens { result_id, data });
}
```

CRITICAL ORDERING: `convert_semantic_tokens_edits` mutates `edit.data` in place BEFORE the splice reads it — the splice must use the ORIGINAL values. Constrain the order inside `modify_outgoing_semantic_tokens_delta_result` by cloning first: change the TokensDelta branch to snapshot `let original: Vec<_> = delta.edits.clone();` BEFORE `convert_semantic_tokens_edits`, and pass `&original` to the splice. (The snippet above shows the shape; the landing must follow this order — a test in Step 3 pins it.)

Row:

```rust
semantic_tokens_full_delta: semantic_tokens_full_delta @ SemanticTokensFullDelta {
    doc: "Handles `textDocument/semanticTokens/full/delta` requests from the client.\n\nReturns edits transforming the previous token stream (identified by `params.previous_result_id`) into the current one, or a full stream when a delta is not practical. Token columns and lengths are UTF-8 here; edits' inserted tokens are converted seeded against the cached previous UTF-8 stream, and flat-array indices pass through unchanged. Requires a semantic tokens provider with `full.delta` support in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::SemanticTokensDeltaParams,
    response: Option<async_lsp::lsp_types::SemanticTokensFullDeltaResult>,
    document: text_document,
    outgoing: modify_outgoing_semantic_tokens_delta_result,
}
```

- [ ] **Step 3: Tests** — `semantic_tokens_full_delta.rs`, hand-written, three tests. The conversion document is TWO-LINE and multibyte on both lines: open a document with text `"🙂abc\nx🙂z"` at a unique URL (line 0 = `🙂abc`, line 1 = `x🙂z` where 🙂 spans bytes 1–4, so byte 5 = `z`, UTF-16 units: x=1, 🙂=2, z at 3):
  1. `delta_edits_convert_seeded_from_cache`: seed the cache by running a full response through `modify_outgoing_semantic_tokens_result` (UTF-8 data `[token(0, 0, 4), token(0, 4, 3)]`, result "r1"); then a delta whose edit `{start: 5, delete_count: 5, data: Some(vec![token(1, 5, 1)])}` replaces the second token with one at line 1, UTF-8 byte 5, length 1 — assert the OUTGOING inserted token is `(delta_line: 1, delta_start: 3, length: 1)` (new line → own column; byte 5 → UTF-16 3).
  2. `delta_cache_miss_passes_through`: no seed; the edit comes back byte-identical.
  3. `delta_splice_keeps_original_utf8_values`: after the seeded conversion of test 1, `state.cached_semantic_tokens(url)` holds the SPLICED stream `[token(0, 0, 4), token(1, 5, 1)]` — the ORIGINAL UTF-8 inserted values — under the delta's new result id; this pins the clone-before-convert ordering.
  Plus `semantic_tokens_range.rs`: one incoming-range + outgoing-columns test mirroring Task 1's shape over the range result enum (both branches: Tokens with data; Partial with data-only).
- [ ] **Step 4: Battery + checkpoint** (expected +5 lib tests per config).

---

### Task 3: Resolve trio — codeLens/resolve, inlayHint/resolve, workspaceSymbol/resolve

**Files:**
- Modify: `src/requests/registry.rs` (3 rows appended to `resolve_methods!`)
- Create: `src/requests/code_lens_resolve.rs`, `inlay_hint_resolve.rs`, `workspace_symbol_resolve.rs` (struct + impl + tests; `pub(crate) use` re-exports)

**Interfaces:**
- Consumes: `implement_resolve_method!` (sole-document anchor + `convert_resolve_item` both directions — already stamped from the resolve table).
- Produces: `CodeLensResolve` (incoming range + outgoing range — thin `convert_range` both directions), `InlayHintResolve` (position + text_edits + label-part locations both directions — reuse the T7 inlay walk as a direction-parameterized `convert_inlay_hint(state, document, &mut InlayHint, Direction)` factored out of `modify_outgoing_inlay_hints`), `WorkspaceSymbolResolve` (incoming `OneOf` Left location → convert against THAT location's document with `read_document_from_disk` fallback; outgoing same — mirror `symbol.rs`'s per-URL resolution but for a single item).

- [ ] **Step 1: Rows** (docs follow the retrofit resolve rows' pattern — describe the default-unchanged resolution and the sole-document conversion):

```rust
code_lens_resolve: code_lens_resolve @ CodeLensResolve {
    doc: "Handles `codeLens/resolve` requests from the client.\n\nFills in the command of a lens previously returned by [`Server::code_lens`]. The default implementation resolves the lens unchanged. Requires a code lens provider with `resolve_provider` enabled. The lens's range is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise it passes through unchanged.",
    params: async_lsp::lsp_types::CodeLens,
    response: async_lsp::lsp_types::CodeLens,
}
inlay_hint_resolve: inlay_hint_resolve @ InlayHintResolve {
    doc: "Handles `inlayHint/resolve` requests from the client.\n\nFills in additional detail on a hint previously returned by [`Server::inlay_hint`]. The default implementation resolves the hint unchanged. Requires an inlay hint provider with `resolve_provider` enabled. The hint's position, edits, and label-part locations are converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
    params: async_lsp::lsp_types::InlayHint,
    response: async_lsp::lsp_types::InlayHint,
}
workspace_symbol_resolve: workspace_symbol_resolve @ WorkspaceSymbolResolve {
    doc: "Handles `workspaceSymbol/resolve` requests from the client.\n\nFills in the location range of a symbol previously returned by [`Server::symbol`] without one. The default implementation resolves the symbol unchanged. Requires a workspace symbol provider with `resolve_provider` enabled. The symbol's location is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — against the location's own document when tracked, reading from disk otherwise; a location without a range passes through unchanged.",
    params: async_lsp::lsp_types::WorkspaceSymbol,
    response: async_lsp::lsp_types::WorkspaceSymbol,
}
```

- [ ] **Step 2: Three custom files** — each mirrors the landed custom pattern (struct, `impl Request`, re-export, `request_extract_url!` NOT used — resolve requests carry no document; hooks are plain `modify_params`/`modify_response`):
  - `code_lens_resolve.rs`: both hooks are one-line `convert_range(..., &mut lens.range, ...)` calls with the respective Direction.
  - `inlay_hint_resolve.rs`: factor `convert_inlay_hint(state, document, hint, direction)` out of T7's `modify_outgoing_inlay_hints` (position + text_edits + label-part locations; the outgoing helper then loops it — refactor keeps the walk in one place); both resolve hooks call it.
  - `workspace_symbol_resolve.rs`: `convert_workspace_symbol_location(state, document, symbol, direction)` — on `OneOf::Left(location)`, resolve the conversion document per-URL (`state.document(&location.uri)` else `read_document_from_disk`), convert the range against it; `Right(_)` untouched. (Reuse pattern from `symbol.rs`; a shared private fn in conversion.rs is fine.)
- [ ] **Step 3: Tests** — capture-server dispatch tests after the `drive_link_resolve` pattern (with_state/tests.rs or the per-file tests — follow where link_resolve's dispatch tests live): each resolve drives one round trip asserting the sole-document conversion both directions; workspace_symbol_resolve additionally pins the Right-variant pass-through and the disk-fallback branch with a temp_workspace file.
- [ ] **Step 4: Battery + checkpoint** (expected +3..6 lib tests per config depending on test granularity — report actual).

---

### Task 4: Notification internal handlers

**Files:**
- Modify: `src/server/with_state/mod.rs` (six notification methods), `src/server/state/documents.rs` (three state methods) — read the file first; the handler methods live where `handle_document_save` does
- Test: `src/server/with_state/tests.rs` or `src/server/state/tests.rs` (wherever the existing document-state tests live)

**Interfaces:**
- Produces: `ServerState::handle_watched_files_change(&self, changes: Vec<FileEvent>) -> ControlFlow`-equivalent (sync, returns the handler's ControlFlow shape used by with_state — follow `handle_document_save`'s signature convention), `handle_files_renamed(&self, files: Vec<FileRename>)`, `handle_files_deleted(&self, files: Vec<FileDelete>)` — plus the with_state notification methods wiring them.

- [ ] **Step 1: State methods** (behavior per the spec's Notifications table):
  - `handle_watched_files_change`: for each `FileEvent { uri, typ }` — `Deleted` → remove the entry if it is Workspace-origin (Open-origin untouched); `Created`/`Changed` → if a Workspace-origin entry exists for the URI, re-read from disk and replace its snapshot (std::fs; unreadable → trace + keep the old snapshot). Untracked URIs ignored.
  - `handle_files_renamed`/`handle_files_deleted`: parse each String URI (`Url::parse`; unparseable → trace + skip), remove Workspace-origin entries matching the old URI / the URI; Open-origin untouched.
- [ ] **Step 2: with_state methods** — six notification handlers, each sync, `ControlFlow::Continue(())`, `#[cfg(feature = "tracing")] debug!` first:

```rust
    fn will_save(&mut self, params: WillSaveTextDocumentParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("will_save: {}", params.text_document.uri);
        ControlFlow::Continue(())
    }

    fn did_change_watched_files(
        &mut self,
        params: DidChangeWatchedFilesParams,
    ) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_change_watched_files: {} events", params.changes.len());
        self.state.handle_watched_files_change(params.changes)
    }

    fn did_create_files(&mut self, params: CreateFilesParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_create_files: {} files", params.files.len());
        ControlFlow::Continue(())
    }

    fn did_rename_files(&mut self, params: RenameFilesParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_rename_files: {} files", params.files.len());
        self.state.handle_files_renamed(params.files)
    }

    fn did_delete_files(&mut self, params: DeleteFilesParams) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("did_delete_files: {} files", params.files.len());
        self.state.handle_files_deleted(params.files)
    }

    fn work_done_progress_cancel(
        &mut self,
        params: WorkDoneProgressCancelParams,
    ) -> ControlFlow<Result<()>> {
        #[cfg(feature = "tracing")]
        debug!("work_done_progress_cancel: {:?}", params.token);
        ControlFlow::Continue(())
    }
```

(Imports extend: `DidChangeWatchedFilesParams, WillSaveTextDocumentParams, CreateFilesParams, RenameFilesParams, DeleteFilesParams, WorkDoneProgressCancelParams, FileEvent, FileRename, FileDelete`. The state methods return `ControlFlow<Result<()>>` — `Continue(())` on success; nothing in them fails hard, unreadable files are traced-and-skipped entries.)
- [ ] **Step 3: W0 state tests** — real temp workspaces: (a) watched-files `Changed` re-reads a mutated disk file into a Workspace-origin snapshot (mutate between snapshot and event; assert new text visible via `state.document`); (b) `Deleted` drops the Workspace entry; (c) rename/delete drop old-URI entries; (d) Open-origin immunity across all three. Workspace-origin snapshots are produced by the existing workspace-loading path — read how `refresh_workspace_documents`/diagnostics creates them and reuse that entry point in the test setup (do not construct private state by hand).
- [ ] **Step 4: Battery + checkpoint** (expected +4 lib tests per config).

---

### Task 5: The twelve sync `Server`-trait hooks

**Files:**
- Modify: `src/server/server_trait.rs` (12 hook methods below the registry stamp), `src/server/with_state/mod.rs` (call sites)
- Test: `src/server/with_state/tests.rs` (table-driven wiring test)

**Interfaces:**
- Produces: hooks `will_save`, `did_change_watched_files`, `did_create_files`, `did_rename_files`, `did_delete_files`, `work_done_progress_cancel`, `did_open`, `did_change`, `did_close`, `did_save`, `did_change_configuration`, `did_change_workspace_folders` — all `fn $name(&self, _state: &ServerState, _params: &$Params) {}`, doc comment stating: called after the internal handler, synchronous by protocol necessity, default no-op.

- [ ] **Step 1: Hook methods** — one representative shape:

```rust
    /// Called after the internal handler processes a
    /// `textDocument/didOpen` notification.
    ///
    /// Synchronous by protocol necessity — an async hook would require
    /// spawning and break LSP message ordering — so hooks may not await
    /// and must not panic. The default implementation does nothing.
    fn did_open(&self, _state: &ServerState, _params: &DidOpenTextDocumentParams) {}
```

(Twelve of these; params types: `WillSaveTextDocumentParams`, `DidChangeWatchedFilesParams`, `CreateFilesParams`, `RenameFilesParams`, `DeleteFilesParams`, `WorkDoneProgressCancelParams`, `DidOpenTextDocumentParams`, `DidChangeTextDocumentParams`, `DidCloseTextDocumentParams`, `DidSaveTextDocumentParams`, `DidChangeConfigurationParams`, `DidChangeWorkspaceFoldersParams` — extend the import list.)
- [ ] **Step 2: Call sites** — in each with_state notification handler, after the internal call, add `self.server.$hook(&self.state, &params);` (borrow note: `self.server` is `Arc<T>` — `self.server.did_open(&self.state, &params)` works on the Arc's deref; params borrowed before any move).
- [ ] **Step 3: Table-driven wiring test**:

```rust
/// Records every hook fired, in order.
struct HookRecordingServer(Arc<Mutex<Vec<&'static str>>>);

impl Server for HookRecordingServer {
    fn did_open(&self, _state: &ServerState, _params: &DidOpenTextDocumentParams) {
        self.record("did_open");
    }
    // ... eleven more, same shape; `record` pushes into the mutex.
}
```

One test drives all twelve notifications through `LanguageServerWithState` (reuse the existing builders: `open_document` for the document quartet; minimal literals elsewhere per the verification report's notification-params section) and asserts the recorded slice equals the twelve names in dispatch order. For `did_change_watched_files`, the hook asserts the AFTER-internal contract: the hook reads `state.document(&url)` and observes the already-refreshed text (drive it against a temp-workspace file mutated between snapshot and event, as in Task 4's state test).
- [ ] **Step 4: Battery + checkpoint** (expected +1 lib test per config).

---

### Task 6: `active_signature_help` incoming fix (registered Minor)

**Files:**
- Modify: `src/requests/registry.rs` (signature_help row moves from `generated_methods!` to `custom_methods!` — drop the hook fields, doc gains the context sentence), `src/requests/conversion.rs` (factor the label-offset walk into `convert_signature_help_label_offsets(state, document, help, direction)`), `src/requests/mod.rs` (mod + re-export)
- Create: `src/requests/signature_help.rs` (struct + impl + tests move/extend)

**Interfaces:**
- Produces: `SignatureHelp` custom impl whose `modify_params` converts the flattened position AND walks `params.context.active_signature_help`'s label offsets client→UTF-8 (reversed direction of the outgoing walk); `modify_response` calls the existing outgoing helper.

- [ ] **Step 1: Factor the walk** — `convert_signature_help_label_offsets(state, document, help: &mut SignatureHelp, direction)` containing exactly today's `modify_outgoing_signature_help` body's walk; the outgoing helper delegates with `Direction::Outgoing`.
- [ ] **Step 2: Custom impl** — position conversion, then the context walk:

```rust
    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_position(
            state,
            document,
            &mut params.text_document_position_params.position,
            Direction::Incoming,
        );
        if let Some(help) = params
            .context
            .as_mut()
            .and_then(|context| context.active_signature_help.as_mut())
        {
            convert_signature_help_label_offsets(state, document, help, Direction::Incoming);
        }
    }
```
- [ ] **Step 3: Row move + doc sentence** — the custom row's doc replaces "Parameter label offsets are recounted..." with "The position AND the label offsets of an echoed `context.active_signature_help` are converted to UTF-8 before the handler runs; label offsets are recounted against the label string itself."
- [ ] **Step 4: Mirror test** — incoming twin of T7's outgoing label test: params with client-unit offsets `[2,3]` on label "🙂f(a)" + position (0,2); assert byte `[4,5]` and position (0,4) after `modify_params`. Existing outgoing test moves with the file unchanged.
- [ ] **Step 5: Battery + checkpoint** (expected +1 lib test per config).

---

### Task 7: Resolve standalone fallback + final sweep

Extended by the owner's design decision (2026-09-02, registered at T3's review): the resolve engine's sole-document gate leaves `workspaceSymbol/resolve` unconverted in multi-document sessions — the mirror of the T8 gate the standalone hook fixed for `workspace/symbol`.

**Files:**
- Modify: `src/requests/mod.rs` (`modify_params_standalone` hook), `src/server/with_state/mod.rs` (`implement_resolve_method!` branch), `src/requests/workspace_symbol_resolve.rs` (both hooks move), `.claude/rules/structure.md` (one sentence), `.claude/rules/testing.md` (`token` fixture line)
- Test: `src/server/with_state/tests.rs` (multi-doc resolve dispatch test)

- [ ] **Step 1: The params-side standalone hook** in `Request` (below `modify_response_standalone`):

```rust
    /// Params conversion for resolve requests with no document anchor.
    ///
    /// The resolve engine calls this instead of [`Request::modify_params`]
    /// when no sole tracked document resolves. Default no-op; override for
    /// state-driven conversions that resolve their own documents (the
    /// workspace-symbol-resolve shape).
    fn modify_params_standalone(_state: &ServerState, _params: &mut Self::Params) {}
```

- [ ] **Step 2: The resolve engine branch** — in `implement_resolve_method!`, replace the unconditional convert/handler/convert flow's anchor handling: when the sole document resolves, behavior is EXACTLY today's; when it does not, call `modify_params_standalone` before the handler and `modify_response_standalone` after it (the T8.5 hook already exists).

- [ ] **Step 3: workspace_symbol_resolve moves both hooks** to the standalone pair (its converters are document-free — the per-URL resolution takes state only). With this, its row doc's unconditional conversion description becomes TRUE as written (no gating clause to add). A multi-doc dispatch test: TWO tracked documents, resolve driven through dispatch, per-URL conversion asserted for a location in each.

- [ ] **Step 4: Sweep items** — 45-row registry-vs-spec cross-check (42 + 3 resolve); dupes comment-out probe per entry with fate table, reason-text refresh for drifted live entries; `token` fixture line in testing.md's harness inventory; the signature_help row doc's outgoing leg (trivia docs sweep, fold if trivial); cross-line folding fixture only if a one-line change.
- [ ] **Step 5: Full battery + review checkpoint; owner commits. Then the END-OF-CYCLE whole-branch review (base `c5cae8d`) with the ledger's carry-forward list.**
