# LSP Surface Completion — Design

Date: 2026-09-01
Status: approved design, pending owner spec review
Inputs: frozen v2 prompt (session), upstream verification pass (33/33 methods,
pinned lsp-types 0.95.1 + async-lsp 0.2.4 sources), LSP 3.17 specification
(normative quotes below).

## Goal

Wire every client→server method that async-lsp 0.2.4's `LanguageServer` trait
defines and this crate does not yet dispatch: **33 requests** and **6
notifications**. Before the methods: two pieces of shared machinery that make
growth cheap — a **method registry** (one row stamps trait method + dispatch +
regular Request hooks) and a **table-driven W0 conversion test harness**
(one row stamps a full conversion test). This closes the crate's LSP surface:
after the cycle, no client→server request falls through to `-32601` by
omission, and no notification from the upstream list can produce an
`Unhandled notification` routing error.

## Owner decisions (settled 2026-09-01)

1. **Symbols in**: `document_symbol`, `symbol`, `workspace_symbol_resolve`
   enter this cycle as ordinary plumbing. The parked 2026-08-28 symbols
   spec is dead; this design re-anchors from scratch per the re-open
   protocol (the parked cycle's approved URL-less conversion shape is
   recovered below).
2. **Semantic tokens: full trio** (full, full/delta, range).
3. **Notifications: full package + sync trait hooks everywhere** — all six
   unwired notifications get internal handlers, and EVERY client→server
   notification the crate dispatches (twelve) exposes a synchronous
   `Server`-trait hook (revised twice during spec review; see
   Notifications).
4. **Harness**: local `macro_rules!` in `src/testing.rs`, stamps one
   `#[test]` per row; a dedicated retrofit task migrates existing
   conversion tests into rows.
5. **Method registry**: full form — one table, multiple consumers;
   retrofit of the 16 wired methods first (Plan 2 Task 0).

## Current state (verified)

- Wired requests (16): `implement_methods!` — hover, completion, code_action,
  document_link, declaration, definition, references, rename, prepare_rename,
  formatting, range_formatting, document_diagnostic;
  `implement_resolve_method!` — completion_item_resolve, code_action_resolve,
  document_link_resolve (`src/server/with_state/mod.rs:226-250`).
- Wired notifications: initialized, did_change_configuration,
  did_change_workspace_folders, did_open, did_close, did_change, did_save
  (same file, `:164-213`); `exit` auto-continues via async-lsp's fallback.
- `workspace/diagnostic` lives in `src/workspace/diagnostics.rs`.
- Unhandled today → latent routing errors if a client sends them:
  `did_change_watched_files`, `work_done_progress_cancel` (non-`$/`
  fallback is `ControlFlow::Break(Routing error)` per
  async-lsp `omni_trait.rs:21-35`). The file-ops notifications and
  `will_save` are capability-gated off by default, so latent only if an
  implementor advertises them.

## Architecture 1 — Method registry

One registry module (`src/requests/registry.rs`) is the single source of
truth for (trait method name, async-lsp method name, Request type,
params/response types, doc, hook shape), split into three tables —
`generated_methods!` (rows fully determine a `Request` impl),
`custom_methods!` (names/types only; hooks live in the per-method file),
and `resolve_methods!` (the resolve trio). Multiple consumers stamp from
the tables via macro passthrough; the pattern is the same one upstream uses
(`omni_trait_generated.rs` consumed by `define!`).

```rust
// src/requests/registry.rs — the table
method_registry! {
    /// Goto implementation locations for the symbol at the position.
    implementation: "textDocument/implementation" @ Implementation {
        params: GotoImplementationParams,
        result: Option<GotoImplementationResponse>,
        hooks: position at text_document_position_params,
    },
    /// ...
    selection_range: "textDocument/selectionRange" @ SelectionRange {
        params: SelectionRangeParams,
        result: Option<Vec<SelectionRange>>,
        custom,   // multiple incoming positions; logic in src/requests/selection_range.rs
    },
}
```

Consumers (each expands the same rows through its own stamper, via the
macro-passthrough `method_registry!($consumer)` technique):

1. `server_trait.rs` — stamps `Server` trait methods: default body
   `method_not_implemented("<name>")`, `///` doc from the row, `#[must_use]`.
   Keeps the hand-written config methods (`server_info`, `server_options`,
   `server_capabilities`, `server_document_matchers`) above the stamp.
2. `with_state/mod.rs` — stamps the `implement_methods!` table body
   (replaces the hand-maintained list) and a second table for
   `implement_resolve_method!` rows (rows carry a `resolve` flag).
3. `src/requests/mod.rs` — for fully generated rows, stamps
   `pub struct X;` + `impl Request for X` (method string from the row,
   `extract_url`/`modify_params` composed from the existing
   `request_extract_url!` / `request_modify_params_position!` and a new
   `request_modify_params_range!` for range-at-path rows;
   `modify_response` delegated to the row's named outgoing helper).
   Rows marked `custom` keep a per-method file that owns the struct and impl.

A row is fully generated when both sides fit the grammar — incoming:
`position at <path>`, `range at <path>`, or `none` (extract_url still
derives from `text_document` when present; `none` also covers no-document
rows like execute_command); outgoing: `<helper>` (a function in
`conversion.rs`) or `none`. Anything else is `custom`.

Adding a method becomes: one row, plus the outgoing helper in
`conversion.rs` when the response carries positions (+ one thin hooks file
when the shape is irregular). Rows are compile-time checked — a wrong type
or path fails the build. `request_modify_params_range!` earns its place
with three generated-row users (color_presentation, inlay_hint,
semantic_tokens_range), so it is not a single-use abstraction.

**Retrofit** (Plan 2 Task 0): migrate the 16 wired methods into rows.
Methods with response conversion (most) keep their files and become
`hooks: custom`; the hand-maintained table in `with_state` and the trait
method list in `server_trait` are deleted in favor of stamped output.
Battery green after the retrofit proves the stamping is faithful.

## Architecture 2 — W0 conversion test harness

`conversion_tests!` in `src/testing.rs` (exported `pub(crate) use`, edition
2024), invoked once per request file inside its `#[cfg(test)] mod tests`.
A row carries only what varies:

- test name;
- `Request` type;
- `params` closure (fixture state → params, positions expressed in the
  CLIENT encoding);
- expected UTF-8 assertion for the incoming position;
- optional outgoing pair: a response built at a UTF-8 position and the
  expected client-encoding position.

Expansion per row is the standard W0 test: `state_with_documents` fixture
("🙂abc", UTF-16 negotiated; byte 4 == UTF-16 unit 2 is load-bearing) →
`modify_params` → assert → `modify_response` → assert. One `#[test]` per
row, named in the runner. Rows with a wrong-typed params closure fail to
compile.

Coverage boundary: single incoming position (+ optional single outgoing
position pair). Outside: multi-position, u32-field shapes, nested/linkedList
responses, URL-less responses, resolve family — hand-written as today.
One hand-written test exercises the macro's expansion on a canonical case
(the "test of the attribute" in the owner's PHP analogy).

Retrofit (Plan 1 Task 2): the regular-shape conversion tests migrate in
place into rows — definition and hover first (canonical pair, Plan 1 Task
1), then declaration, references, document_link, document_format,
document_range_format, rename_prepare, and completion's incoming side if
it fits the single-position shape. The irregular tests (code_action,
rename, document_diagnostics) and the resolve-family tests stay
hand-written — the coverage boundary excludes them. Expected
dupes effect: row lists are not near-duplicate AST bodies; if expansions
trip `cargo dupes check` (it analyzes source, not expansions), one reasoned
`.dupes-ignore.toml` entry for the macro itself — never per row.

## Roadmap — 33 requests

Verified against pinned sources. Anchor shorthand: `LT` =
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lsp-types-0.95.1/src`,
`AL` = same root + `async-lsp-0.2.4/src`. Trait-method names from
`AL:omni_trait_generated.rs`.

### Class A — fully generated registry rows (20 methods)

One row each; the named outgoing helper lands in `conversion.rs` as part of
the same task. Existing helpers cover the goto and TextEdit shapes.

| method | request type (LT:request.rs) | incoming | outgoing helper |
|---|---|---|---|
| implementation | GotoImplementation (:378) | position | `modify_outgoing_locations` — exists |
| type_definition | GotoTypeDefinition (:363) | position | same, exists |
| document_highlight | DocumentHighlightRequest (:397) | position | new `_document_highlights` |
| moniker | MonikerRequest (:824) | position | none (response carries no positions) |
| linked_editing_range | LinkedEditingRange (:602) | position | new `_linked_editing_ranges` |
| on_type_formatting | OnTypeFormatting (:588) | position | `modify_outgoing_text_edits` — exists |
| will_save_wait_until | WillSaveWaitUntil (:456) | none (staleness via text_document) | `modify_outgoing_text_edits` |
| execute_command | ExecuteCommand (:442) | none (no document at all, no staleness) | none — nothing to convert, nothing to pin |
| prepare_call_hierarchy | CallHierarchyPrepare (:714) | position | new `_call_hierarchy_items` |
| prepare_type_hierarchy | TypeHierarchyPrepare (:938) | position | new `_type_hierarchy_items` |
| code_lens | CodeLensRequest (:525) | none | new `_code_lenses` |
| folding_range | FoldingRangeRequest (:644) | none | new `_folding_ranges` |
| document_color | DocumentColor (:623) | none | new `_color_informations` |
| color_presentation | ColorPresentationRequest (:634) | range | new `_color_presentations` |
| inlay_hint | InlayHintRequest (:834) | range | new `_inlay_hints` |
| signature_help | SignatureHelpRequest (:316) | position | new `_signature_help` (label-string offsets — see Verification notes) |
| document_symbol | DocumentSymbolRequest (:408) | none | new `_document_symbols` (recursive Flat/Nested tree) |
| will_create_files | WillCreateFiles (:789) | none | new `_workspace_edit` |
| will_rename_files | WillRenameFiles (:798) | none | same |
| will_delete_files | WillDeleteFiles (:807) | none | same |

Verified response shapes these helpers must cover: `GotoDefinitionResponse`
untagged `Scalar(Location) | Array(Vec<Location>) | Link(Vec<LocationLink>)`
(LT:lib.rs:2613-2617); `FoldingRange` with non-Option `start_line/end_line`
and `Option<u32>` characters; `document_color`/`color_presentation` results
are bare Vecs, not Option; `DocumentSymbolResponse` untagged
`Flat(Vec<SymbolInformation>) | Nested(Vec<DocumentSymbol>)` with recursive
`children` — `Flat` is declared first; `WorkspaceEdit` in three forms
(`changes` map, `document_changes` Edits|Operations, `change_annotations`).

### Class A2 — custom hook files (7 methods, hand-written tests)

| method | request type | why the row cannot express it |
|---|---|---|
| selection_range | SelectionRangeRequest (:706) | `positions: Vec<Position>` — multiple incoming positions; response is a linked list `range + parent: Option<Box<_>>`, walked by `_selection_ranges` |
| inline_value | InlineValueRequest (:870) | two incoming ranges (`range` + `context.stopped_location`); response is a single `Option<InlineValue>` enum — all three variants carry a Range (`_inline_value`) |
| incoming_calls | CallHierarchyIncomingCalls (:722) | params carry `item: CallHierarchyItem` — incoming conversion of uri + two ranges; response nests items plus `from_ranges` |
| outgoing_calls | CallHierarchyOutgoingCalls (:730) | same shape, `to` item |
| supertypes | TypeHierarchySupertypes (:951) | params carry `item: TypeHierarchyItem`; response reuses `_type_hierarchy_items` |
| subtypes | TypeHierarchySubtypes (:963) | same |
| symbol (workspace/symbol) | WorkspaceSymbolRequest (:419) | **URL-less** response, `WorkspaceSymbol.location: OneOf<Location, WorkspaceLocation{uri}>` — see below |

Helpers live in `conversion.rs` under the `modify_outgoing_*` naming
convention (the Class A table abbreviates). Class A2 adds:
`_selection_ranges` (walks the parent chain), `_inline_value` (match on
variants), `_call_hierarchy_incoming_calls` / `_call_hierarchy_outgoing_calls`,
`_workspace_symbols` (the OneOf location), plus incoming counterparts where
params carry positions, ranges, or items. Shared semantics: hierarchy and
symbol items convert their ranges against the item's own document, falling
back to the request document (the existing convention).
`TypeHierarchyItem.tags: Option<SymbolTag>` is single — a verified quirk
with no conversion impact.

**String URIs** in file-ops params: parse to `Url` fallibly; unparseable
entries are traced and skipped (stream-of-fallible-entries discipline), not
panicked on (error-handling.md: no panics on external input).

**workspace/symbol URL-less conversion** (recovered from the parked
cycle's approved shape): store-first — location URI tracked → convert
against the tracked document; disk-fallback — read the file (cache per
request so N symbols in one response cost one read); pass-through —
unreadable or untracked-and-unreadable stays unconverted. The
`Right(WorkspaceLocation)` variant has no range, nothing to convert.

### Class B — resolve family (`implement_resolve_method!`, sole-document heuristic)

| method | request type | params in | result out |
|---|---|---|---|
| code_lens_resolve | CodeLensResolve (:536) | `CodeLens` (incoming range) | `CodeLens` |
| inlay_hint_resolve | InlayHintResolveRequest (:846) | `InlayHint` (incoming position/edits/label locations — richer than codeLens) | `InlayHint` |
| workspace_symbol_resolve | WorkspaceSymbolResolve (:430) | `WorkspaceSymbol` (incoming OneOf location) | `WorkspaceSymbol` |

Tests: capture-server dispatch tests after the `link_resolve` pattern.

### Class C — semantic tokens

Full design below.

## Semantic tokens

**Trait contract:** the three `Server` methods produce tokens whose
`delta_start` and `length` are counted in **UTF-8**, always. The wrapper
converts to the negotiated encoding on the way out (and the /range request's
incoming `Range` from negotiated to UTF-8).

**Normative anchors (LSP 3.17 spec):**
- Encoding: "The `deltaStart` and the `length` values must be encoded using
  the encoding the client and server agrees on during the `initialize`
  request."
- Edits: the worked example `{ start: 0, deleteCount: 1, data: [3] }`
  "replace the first number in the array" — `start`/`delete_count` index
  the **flattened u32 array**, and edits "are expressed on these number
  arrays without any form of interpretation what these numbers mean",
  unsorted, applied back-to-front.
- 5-tuple layout is enforced by lsp-types' custom serde
  (`LT:semantic_tokens.rs:146-228`); results are enums:
  `SemanticTokensResult = Tokens | Partial`,
  `SemanticTokensFullDeltaResult = Tokens | TokensDelta | PartialTokensDelta{edits}`
  (inline struct variant), `SemanticTokensRangeResult = Tokens | Partial`.

**Converter** (`src/requests/conversion.rs`):
`convert_semantic_tokens(document, tokens: &mut Vec<SemanticToken>, direction)`
— walks 5-tuples, reconstructs absolute (line, char_utf8) from deltas,
converts `delta_start` and `length` columns through the rope, re-deltas.
Line deltas are encoding-independent.

**The delta-request cache.** Converting a `SemanticTokensEdit`'s inserted
`data` requires the absolute position of the token preceding the edit
region — which lives in the **previous** token array, not in the request.
The wrapper is the only party that speaks both encodings, so
`ServerState` gains a per-document cache of the last **UTF-8** tokens it
handed out (keyed by URL: `(result_id, Vec<SemanticToken>)`), written
whenever a full or delta response passes through `modify_response`. Delta
conversion then: reconstruct the preceding token's absolute UTF-8 position
from the cached array, walk the edit's data accumulating absolute
positions, convert via rope, re-delta. `start`/`delete_count` pass through
untouched — they index array positions, which conversion never changes
(values change, the 5-per-token layout does not). A `didChange` to the
document does not invalidate the cache: the server's own edits are against
its previous UTF-8 state, which is exactly what the cache holds.

Three Request impls: `semantic_tokens_full` (extract_url staleness,
outgoing `Tokens | Partial` both converted), `semantic_tokens_range`
(incoming range via the new `request_modify_params_range!`, same outgoing),
`semantic_tokens_full_delta` (outgoing enum, all three variants; edits per
the cache design above). Tests: hand-written — multi-line token stream,
"🙂" at a token boundary (length 4 bytes = 2 UTF-16 units), UTF-16 and
UTF-32 negotiation, the edit branch with a seeded cache, and the
spec-example array round-trip.

## Notifications

Every unwired notification gets an internal handler on
`LanguageServerWithState` plus a sync state method after the
`handle_document_save` pattern. Handlers stay synchronous (LSP + async-lsp
constraint: notifications are processed inline, in order).

**Sync trait hooks** (owner decision, revised during spec review): the
five state-relevant notifications also expose a hook on the `Server` trait
so implementors can react without touching plumbing:

```rust
/// Called after the internal handler processes the notification.
/// Synchronous by protocol constraint; default no-op.
fn will_save(&self, _state: &ServerState, _params: &WillSaveTextDocumentParams) {}
```

- Hook set: **every client→server notification the crate dispatches** —
  the six new ones (`will_save`, `did_change_watched_files`,
  `did_create_files`, `did_rename_files`, `did_delete_files`,
  `work_done_progress_cancel`) plus the six already-internal ones
  (`did_open`, `did_change`, `did_close`, `did_save`,
  `did_change_configuration`, `did_change_workspace_folders`). Owner's
  rationale: retrofitting hooks later costs a full re-derivation cycle;
  build the uniform surface now. The `$/` trio (setTrace, cancelRequest,
  progress) is the one deliberate exception — async-lsp auto-ignores the
  `$/` prefix, and a `cancelRequest` hook without request-cancellation
  machinery would be noise; revisit only if cancellation support ever
  lands.
- Shape: `&self`, `&ServerState`, `&params`, returns `()`. Sync by
  necessity — an async hook would require spawning a task and would break
  LSP message ordering; hooks may not await and must not panic (the same
  contract the internal handlers already carry).
- Ordering: internal handler first, hook second — hooks observe
  post-internal state (the `did_change_watched_files` hook sees already
  refreshed documents).
- Default bodies are empty — additive; no implementor breaks.
- Hooks are hand-written trait methods, not registry rows (the registry is
  request-shaped).

| notification | internal behavior | trait hook |
|---|---|---|
| did_change_watched_files | for each `FileEvent` (`uri: Url`, `typ`: Created/Changed/Deleted) whose URI is a tracked **Workspace**-origin document: re-read from disk (sync `std::fs`, same discipline as the didChange fallback); Deleted → drop the snapshot; Open-origin documents untouched (the editor owns them). Unreadable → trace + continue | `did_change_watched_files` |
| did_rename_files / did_delete_files | drop tracked Workspace snapshots matching the old URIs (String URIs parsed fallibly; unparseable traced and skipped); next workspace scan re-adds | `did_rename_files` / `did_delete_files` |
| did_create_files | no-op + `debug!` (tracing feature) | `did_create_files` |
| will_save | `debug!` only — the hook is the point | `will_save` |
| work_done_progress_cancel | no-op + `debug!` with token | `work_done_progress_cancel` |
| did_open / did_change / did_close / did_save | existing document-sync machinery (rope edits, incremental sync, origin bookkeeping) — unchanged | `did_open` / `did_change` / `did_close` / `did_save` |
| did_change_configuration | existing configuration flow (workspace-diagnostics settings, watched-section registration) | `did_change_configuration` |
| did_change_workspace_folders | existing workspace-roots update | `did_change_workspace_folders` |
| `$/setTrace`, `$/cancelRequest`, `$/progress` | untouched — async-lsp auto-ignores the `$/` prefix | none — the deliberate exception |

Tests: W0 state machines — watched-files re-reads a file mutated on disk
(temp workspace), delete drops, rename drops, Open-origin immunity; the
existing document-sync tests double as proof that default hooks change
nothing. Hook wiring is pinned by one table-driven test: drive each of the
twelve notifications through `LanguageServerWithState` with a recording
server; assert the hook fired after the internal handler (the did_change
hook observes the post-edit document, the watched-files hook the refreshed
one).

## Error handling

- `ServerError` only below the wire adapter; String-URI parse failures are
  traced-and-skipped entries, not errors (a stream of fallible entries is
  not one failure).
- No panics on client input: unparseable URIs, out-of-range offsets —
  fallible paths or pass-through, per error-handling.md.
- `# Errors`/`# Panics` docs on new public surface; Display lowercase.

## Verification notes

- Every params/result shape above was verified against pinned sources by a
  research pass (LSP-first, registry fallback); anchors cited inline.
- Three semantics are normative from the LSP 3.17 specification (quoted
  above): token encoding is negotiated; edit start/deleteCount are flat-array
  indices; edits are interpretation-free.
- `ParameterLabel::LabelOffsets` unit is not stated by lsp-types; the spec
  says "based on a UTF-16 string representation as `Position` and `Range`
  does". Resolution: the helper converts offsets against the label string
  itself (recounting code units of one string is well-defined regardless of
  which reading is right); with negotiated UTF-16 the literal sentence and
  the conversion agree.
- Known deviations pinned during verification and reflected above:
  `SemanticTokensEdit` has no `end` (it is `start` + `delete_count`);
  `inline_value` result is a single `Option<InlineValue>`, not a Vec;
  `document_color`/`color_presentation` results are bare Vecs;
  file-op params carry `String` URIs; `TypeHierarchyItem.tags` is a single
  `Option<SymbolTag>`; `DocumentOnTypeFormattingParams` has no work-done
  params; untagged responses declare `Flat` first.

## Plan slicing

- **Plan 1 — Harness** (2 tasks): `conversion_tests!` macro + self-test +
  first rows; retrofit of existing conversion tests into rows.
- **Plan 2 — Mechanical batch** (~8 tasks): Task 0 registry + retrofit of
  the 16 wired methods; then family-grouped tasks (helper + methods + rows
  each): goto/highlight cluster; formatting/edits cluster (on_type,
  will_save_wait_until, file-ops + `_workspace_edit`); folding/selection/
  symbols cluster; colors; hierarchy cluster; inlay/inline cluster;
  signature_help; moniker/linked_editing/execute_command tail.
- **Plan 3 — Specials** (~6 tasks): token converter + full/range; delta +
  UTF-8 cache; resolve trio; notification internal handlers; all twelve
  sync hooks + table-driven wiring test; final sweep.

Each plan: its own SDD run, per-task reviews (sonnet implementer, opus
reviewer), final whole-branch review; the owner commits per task.

## Definition of done

`cargo build --all-targets`; `cargo test` × (default,
`--no-default-features`, `--all-features`); `cargo fmt --check`;
`cargo clippy --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`; `cargo dupes check`
exit 0. Dispatch coverage needs no new wire tests (the parametrized
`-32601` test pins every unwired method automatically as the surface
grows — it shrinks as methods land).

## Out of scope

- Server→client surface (publishDiagnostics conversion helpers, refresh
  requests, applyEdit plumbing) — the crate hands the `ClientSocket` to
  implementors; higher-level wrappers are a future decision.
- Notebook document sync (absent from the pinned async-lsp trait).
- Any lsp-poc-derived requirements (out of scope per the product rule).
- async-lsp upgrades (tracked separately by the PR #30 watch).
