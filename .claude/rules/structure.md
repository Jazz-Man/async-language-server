# Structure

Two layers wrap `async-lsp`:

1. **User layer** — the `Server` trait (`src/server/server_trait.rs`).
   Implementors override async methods (`hover`, `completion`, `definition`,
   `document_diagnostics`, ...). Every method is optional; unimplemented ones
   return `METHOD_NOT_FOUND` through `method_not_implemented`. The
   `*_resolve` methods are the exception: they default to resolving the item
   unchanged.
2. **Plumbing** — `LanguageServerWithState`
   (`src/server/with_state/mod.rs`, the `initialize` flow in
   `src/server/with_state/initialize.rs`), which implements async-lsp's
   `LanguageServer`: `initialize` (position encoding negotiation, capability
   merging, workspace folders) and all document notifications, then forwards
   requests to the `Server` trait.

`serve()` (`src/server/serve.rs`) wires the implementor into async-lsp's `MainLoop`
behind a tower `ServiceBuilder` stack — `LifecycleLayer`, `TracingLayer`,
`ConcurrencyLayer(8)`, `CatchUnwindLayer`, `ClientProcessMonitorLayer` — over
the process standard input and output.

## The UTF-8 invariant

`Server` trait methods always receive and produce **UTF-8** positions, no
matter which encoding was negotiated with the client (preference order in
`POSITION_ENCODING_PREFERRED_ORDER` in `src/server/with_state/mod.rs`:
UTF-8 > UTF-32 > UTF-16). All translation lives in `src/requests/`;
handlers never convert encodings themselves.

## Adding an LSP method touches three places

1. The trait method in `src/server/server_trait.rs`, defaulting to
   `method_not_implemented`, with a `///` doc naming the capability it
   requires.
2. A `Request` impl in a dedicated file under `src/requests/`, re-exported
   from `src/requests/mod.rs`.
3. One line in the `implement_methods!` table in `src/server/with_state/mod.rs`
   (`async_lsp_method => server_trait_method @ RequestType`).

Export any new public types through the `server` module in `src/lib.rs`.

## The `Request` pattern (`src/requests/`)

The `Request` trait lives in `src/requests/mod.rs`; each LSP request has
its own file with a `Request` impl providing three hooks:

- `extract_url` — pulls the document URL out of the params so the wrapper can
  snapshot its version.
- `modify_params` — client encoding → UTF-8, before the handler runs.
- `modify_response` — UTF-8 → client encoding, after.

State-driven conversions that resolve each position against their own
document — no single anchor (the workspace-symbol shape) — override the
standalone pair instead; the engines call `modify_params_standalone` (the
resolve family, when no sole tracked document resolves) and
`modify_response_standalone` (every family, when no conversion document
resolves) directly, and `modify_params`/`modify_response`'s defaults
delegate to them so the override runs in every dispatch state.

Implement these with the existing `modify_incoming_*` / `modify_outgoing_*`
helpers in `src/requests/conversion.rs` (positions, ranges, locations,
diagnostics, text edits) rather than calling `position_to_encoding`
directly — that is reuse of existing machinery, not new abstraction.
Conversely, do not build new abstraction layers for one-off conversions:
if no helper fits, add one next to the others in
`src/requests/conversion.rs`. Conversions stay centralized there. Convert
positions in responses against the document the position refers to, falling
back to the request's own document when that URL isn't tracked.

The `implement_method!` macro glues everything together and adds staleness
detection: it snapshots the document version before the handler and returns
`CONTENT_MODIFIED` if the version changed by response time (clients retry).
Do not duplicate that logic in handlers.

## State and documents

`ServerState` (`src/server/state/mod.rs`) is a cheaply-clonable
interior-mutable handle passed to every handler: a `DashMap` of documents,
workspace roots, the negotiated encoding, and matchers. `Document`
(`src/documents/document.rs`) is a snapshot clone wrapping a `ropey::Rope`,
plus an optional `Language`/`Tree` under the `tree-sitter` feature.

Documents carry an origin: `Open` (from the editor) or `Workspace` (loaded
from disk). Open documents win over disk state; closing an open document
keeps a disk snapshot only when workspace diagnostics are enabled for it.

`didChange` applies incremental edits to the Rope and, with tree-sitter,
`tree.edit()` + incremental reparse. If incremental application fails, it
reloads the whole file from disk — notification handlers must stay
synchronous per the LSP spec and async-lsp, hence the `std::fs` reads there.

## Matching and workspace scanning

`DocumentMatcher` (`src/documents/matcher.rs`) associates documents with a
named matcher via URL globs and/or language-id strings, optionally carrying
a tree-sitter grammar — one language per document, not per server.
`WorkspaceWalker` (`src/workspace/walker.rs`) scans roots with the `ignore`
crate: `.gitignore` respected by default, hidden files skipped.

## Diagnostics surfaces

- `src/workspace/diagnostics.rs` implements `workspace/diagnostic`: walks roots,
  loads matching files as `Workspace` documents, runs per-document
  diagnostics through the same `Server` method, and merges related-document
  reports. Exposure is set via `ServerOptions::with_workspace_diagnostics`:
  `Disabled` / `Enabled` / `Configurable(setting)`, where the setting is read
  from client configuration, each mechanism gated on client capabilities.
- `oneshot::workspace_diagnostics()` runs a `Server` over files on disk with
  no LSP client or transport — it drives `LanguageServerWithState` directly
  with a closed `ClientSocket`. CLI-style batch diagnostics.

Handlers report failures by returning `Err(ServerError)` (`src/error.rs`);
the wrapper converts them to LSP error responses.

## Support modules

- `text_utils` — `Encoding`, `position_to_encoding`, `Position`, and
  `RangeExt` (split/expand/shrink over byte, LSP, and tree-sitter ranges):
  the machinery behind the transparent encoding conversion.
- `tree_sitter_utils` (feature-gated) — parsing helpers over matched
  grammars.

---
_Every change respects the layer split and the UTF-8 invariant: new LSP
surface goes through the three-place pattern, encoding stays centralized in
`src/requests/`._
