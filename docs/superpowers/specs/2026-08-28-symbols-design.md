# `documentSymbol` & `workspace/symbol` support — design

**Date:** 2026-08-28
**Status:** approved design, pre-implementation
**Input:** requirements handoff from lsp-poc (`lsp-poc/docs/superpowers/specs/2026-08-28-als-symbols-requirements-handoff.md`), verified against this repository at `d7795c4` (= HEAD at design time)

## Goal

Downstream servers implementing the `Server` trait can handle `textDocument/documentSymbol` and `workspace/symbol`: the trait has the two methods, `LanguageServerWithState` routes both requests, symbol responses reach the client with correctly encoded ranges, and providers advertised in `server_capabilities()` pass through to the client.

Additionally (owner's decision, this cycle): fix the discovered encoding gap in `workspace/diagnostic`, whose reports currently go out in UTF-8 regardless of the negotiated encoding.

## Decisions

- **D1 — Signatures mirror lsp-types 0.95.1.** `document_symbol` returns `Option<DocumentSymbolResponse>` (`Flat(Vec<SymbolInformation>) | Nested(Vec<DocumentSymbol>)`); `workspace_symbol` returns `Option<WorkspaceSymbolResponse>` (`Flat(Vec<SymbolInformation>) | Nested(Vec<WorkspaceSymbol>)`). The handoff sketched `Option<Vec<SymbolInformation>>` for workspace symbols but explicitly delegated the choice; the response enum matches what async-lsp's `symbol` expects and follows the trait's existing pattern (`definition` → `GotoDefinitionResponse`, `completion` → `CompletionResponse`).
- **D2 — `documentSymbol` rides the standard macro.** `DocumentSymbolParams` carries a document URL and no positions, so `implement_method!` provides staleness detection and response conversion for free.
- **D3 — `workspace/symbol` is hand-wired.** `WorkspaceSymbolParams` carries only a query; with `extract_url` returning `None` the macro never calls `modify_response` (`src/server_with_state.rs:68-84`), so `symbol` gets a manual impl next to `workspace_diagnostic` (`src/server_with_state.rs:297-306`).
- **D4 — URL-less conversion is store-first, disk-fallback (approach B).** Positions in responses of requests that carry no document URL are converted against the document store first; untracked `file://` URLs are read from disk into a transient `ropey::Rope` for conversion; symbols whose document cannot be resolved are returned unchanged. Rejected alternatives: store-only (leaves the common untracked-file case mis-encoded under UTF-16 clients) and a public store-loading API (widens the public surface against the handoff's "nothing else changes" constraint).
- **D5 — All conversion lives in `src/requests.rs`**, per the crate's centralization rule; `workspace_diagnostics.rs` calls into it for the gap fix.

## Design

### 1. Trait surface (`src/server_trait.rs`)

Two methods in the existing handler shape, defaulting to `method_not_implemented` (`METHOD_NOT_FOUND`), with canonical doc comments:

```rust
/// Handles `textDocument/documentSymbol` requests from the client.
///
/// Returns the symbols contained in the document in `params`, or `None`.
/// Both response shapes are supported: `DocumentSymbolResponse::Flat`
/// and `Nested`. Positions and ranges are UTF-8. Requires a document
/// symbol provider in [`Server::server_capabilities`].
fn document_symbol(
    &self,
    state: ServerState,
    params: DocumentSymbolParams,
) -> impl Future<Output = ServerResult<Option<DocumentSymbolResponse>>> + Send {
    method_not_implemented("document_symbol")
}

/// Handles `workspace/symbol` requests from the client.
///
/// Returns the workspace symbols matching `params.query`, or `None`.
/// Positions are UTF-8 and are converted to the negotiated encoding
/// against the document each symbol refers to: tracked documents use
/// their in-memory snapshot, other `file://` documents are read from
/// disk for conversion, and symbols whose document cannot be read are
/// returned unchanged. Requires a workspace symbol provider in
/// [`Server::server_capabilities`].
fn workspace_symbol(
    &self,
    state: ServerState,
    params: WorkspaceSymbolParams,
) -> impl Future<Output = ServerResult<Option<WorkspaceSymbolResponse>>> + Send {
    method_not_implemented("workspace_symbol")
}
```

### 2. `documentSymbol` path

`Request` impl in `src/requests.rs`:

- `extract_url` → `params.text_document.uri.clone()`
- `modify_params` — not overridden (params carry no positions)
- `modify_response` — converts every range in the response:

```rust
fn modify_outgoing_document_symbol(
    state: &ServerState,
    document: &Document,
    symbol: &mut DocumentSymbol,
) {
    modify_outgoing_range(state, document, &mut symbol.range);
    modify_outgoing_range(state, document, &mut symbol.selection_range);
    if let Some(children) = symbol.children.as_mut() {
        for child in children {
            modify_outgoing_document_symbol(state, document, child);
        }
    }
}
```

`Flat` symbols convert through the existing `modify_outgoing_location`; `Nested` through the recursive helper. (The new `Request` struct named `DocumentSymbol` coexists with the lsp type of the same name via the module's existing `Lsp*` import aliasing.) Dispatch table gains one line in `src/server_with_state.rs`:

```
document_symbol => document_symbol @ crate::requests::DocumentSymbol,
```

(async-lsp's omni trait names the methods `document_symbol` and `symbol` — verified in `async-lsp-0.2.4/src/omni_trait_generated.rs:41-44`.)

Staleness semantics are unchanged from the macro: snapshot the document version before the handler, return `CONTENT_MODIFIED` if it changed by response time.

### 3. `workspaceSymbol` path

Hand-wired `symbol` in `LanguageServerWithState`, mirroring `workspace_diagnostic`:

```rust
fn symbol(
    &mut self,
    params: WorkspaceSymbolParams,
) -> BoxFuture<'static, Result<Option<WorkspaceSymbolResponse>, Self::Error>> {
    let server = Arc::clone(&self.server);
    let state = self.state.clone();
    Box::pin(async move {
        let mut result = server.workspace_symbol(state.clone(), params).await?;
        crate::requests::convert_workspace_symbols(&state, &mut result);
        Ok(result)
    })
}
```

`crate::requests::convert_workspace_symbols(&mut Option<WorkspaceSymbolResponse>)` walks the response:

- `Flat` — each `SymbolInformation.location`
- `Nested` — each `WorkspaceSymbol.location`; `OneOf::Left(Location)` is converted, `OneOf::Right(WorkspaceLocation)` (URI-only, resolve flow) carries no positions and is left untouched

No staleness check: there is no single request document (same as `workspace_diagnostic`).

### 4. URL-less conversion machinery (`src/requests.rs`, `pub(crate)`)

A private per-request document cache drives both URL-less entry points:

```rust
/// Resolves documents for responses of requests that carry no document
/// URL: the document store first, the file system second.
struct UrlLessDocumentCache<'a> {
    state: &'a ServerState,
    entries: HashMap<Url, Option<Rope>>,
}
```

- `resolve(url)`: store hit (`state.document(url)`) → clone its Rope; miss with a `file://` URL → `std::fs::read_to_string` into a transient Rope (synchronous read in request context, same precedent as `workspace_diagnostics.rs`); miss with a non-file or unreadable URL → `None` (cached). Nothing is inserted into the document store; open documents keep winning over disk state.
- Range conversion uses the existing `position_to_encoding(&rope, .., Encoding::UTF8, state.get_position_encoding())` path.
- Both entry points return immediately when the negotiated encoding is UTF-8 (identity — no cache, no I/O).
- Each unique URL is read at most once per request via the cache.

Entry points (both in `src/requests.rs`, called from outside the module):

- `pub(crate) fn convert_workspace_symbols(state: &ServerState, response: &mut Option<WorkspaceSymbolResponse>)`
- `pub(crate) fn convert_workspace_diagnostic_report(state: &ServerState, report: &mut WorkspaceDiagnosticReportResult)`

### 5. `workspace/diagnostic` gap fix

After `workspace_diagnostics.rs` assembles the report, it runs `convert_workspace_diagnostic_report`: every diagnostic inside each `WorkspaceDocumentDiagnosticReport::Full` item converts its ranges against that item's `uri` through the shared cache. `Unchanged` items carry no positions. Related-document reports merged into the item list are covered by the same loop.

Behavior change: under a UTF-16/UTF-32 client, workspace diagnostic ranges convert for the first time. Existing tests do not set a non-UTF-8 encoding and stay green.

### 6. Capabilities

`initialize` overwrites only `position_encoding`, `text_document_sync`, and the diagnostics/workspace fields (`src/server_with_state.rs:194-205`); `document_symbol_provider` and `workspace_symbol_provider` set by `server_capabilities()` reach the client unchanged. No code change needed — the design relies on this verified pass-through and adds a regression test.

### 7. Example (`examples/tree_sitter.rs`)

- `server_capabilities` adds `document_symbol_provider: Some(OneOf::Left(true))` and `workspace_symbol_provider: Some(OneOf::Left(true))`.
- `document_symbol` returns `Nested` symbols from the JSON tree: object keys as symbols, recursively for nested objects.
- `workspace_symbol` returns flat `SymbolInformation` for tracked documents, derived from the same key-walking as `document_symbol`, filtered to names containing `params.query` (case-insensitive) — no workspace walk, keeps the example small.
- `examples/minimal.rs` is untouched.

## Testing

Inline `#[cfg(test)] mod tests`, following the existing temp-workspace conventions:

- `requests.rs`
  - nested `DocumentSymbol` children convert recursively (UTF-16 state, multibyte content)
  - `Flat` `SymbolInformation.location` converts
  - workspace symbols: untracked `file://` document on disk converts via the disk read; open document converts via the store (unsaved content wins); non-file or missing URL passes through unchanged
  - identity: with UTF-8 negotiated, nothing changes
- `server_with_state.rs`
  - `document_symbol` end-to-end: handler returns UTF-8, client negotiates UTF-16, response arrives converted
  - unimplemented methods still answer `METHOD_NOT_FOUND`
  - gap fix: initialize with a UTF-16-only client → `workspace_diagnostic` report ranges arrive converted
  - capabilities: a `server_capabilities()` advertising both providers reaches the `InitializeResult` unchanged
- Example builds under the CI battery (`--all-targets`, default features).

## Error handling

- Handlers keep returning `Err(ServerError)`; the wrapper maps to RPC errors as before.
- The hand-wired `symbol` propagates errors with `?`; conversion itself cannot fail.
- An unresolvable document during URL-less conversion is not an error — positions pass through, as documented on the trait.
- No new panics; recursion depth is bounded by the symbol tree the downstream handler itself produced.

## Constraints honored

- **R5 bounds:** no new bounds on `S`; `ConcurrencyLayer(8)` untouched; both methods are `impl Future + Send` like the rest of the trait.
- **Handoff constraint:** document store, matcher API, and existing trait methods unchanged; no new public API besides the two trait methods (additive, defaulted).
- Verification battery per `.claude/rules/tech.md` gates completion.

## Out of scope

- `workspaceSymbol/resolve`
- Public API for workspace iteration/roots (downstream handlers keep their own bookkeeping)
- Exposing the conversion helpers publicly
- Versioning/tagging — the owner's call
- README changes — it does not enumerate trait methods (verified at design time)

## Acceptance

- A downstream server implementing both methods and advertising both providers receives `textDocument/documentSymbol` and `workspace/symbol` requests, and its symbol responses arrive with correctly encoded ranges in a real editor session under any negotiated encoding.
- `workspace/diagnostic` reports arrive with correctly encoded ranges under any negotiated encoding.
- Full CI battery green: `cargo build --all-targets`, `cargo test` in three feature configurations, `fmt --check`, `clippy -D warnings`, `doc -D warnings`.

## Provenance

Handoff citations re-verified at `d7795c4` (= HEAD): trait shape and defaults (`src/server_trait.rs`), dispatch macro and its `modify_response` gating (`src/server_with_state.rs:33-90`), `workspace_diagnostic` hand-wire precedent (`src/server_with_state.rs:297-306`), capabilities pass-through (`src/server_with_state.rs:147-247`), conversion helpers (`src/requests.rs`), missing conversion in `workspace_diagnostics.rs` (no `position_to_encoding`/`modify_outgoing*` calls in the file). lsp-types shapes from 0.95.1 sources; async-lsp method names from 0.2.4 sources.
