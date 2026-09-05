# Macros & Structure — Plan 3: The Migration Sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the remaining 47 requests onto the Plan 2 macros (30 generated, 11 custom, 6 resolve), delete the registry and the 14 obsolete `macro_rules!` definitions, land the completeness wire test and the normative-docs rewrite, and pass the full battery plus the dupes gate with zero behavior change.

**Architecture:** Every request follows the vertical-slice pattern Plan 2 proved on hover: registry row deleted, file gains `#[lsp_request]` + renamed struct, trait gains an `lsp_method!`/`lsp_resolve_method!` block with the row's doc verbatim, the `lsp_dispatch!` table gains one row. Custom/resolve files convert their hand-written `impl Request` hooks into free functions wired through the attribute (`incoming_custom`/`outgoing`/standalone fields), bodies moved verbatim. The final task deletes everything old in one cutover.

**Tech Stack:** the Plan 2 macros (`lsp_request`, `lsp_dispatch`, `lsp_method`, `lsp_resolve_method`), the existing test suites as the behavior-identity pin.

**Spec:** `docs/superpowers/specs/2026-09-04-macros-and-structure-design.md` — all sections; sequencing row 3.

## Global Constraints

- The owner commits; the agent never runs git write commands and never dispatches subagents with worktree isolation — all work in the current branch's working tree.
- **If the Plan 2 Task 1 probe rejected trait-position proc macros**, every `lsp_method!`/`lsp_resolve_method!` block below is written as a plain hand-written trait method instead (same doc, same signature, body `method_not_implemented(stringify!(name))` / `async move { Ok(item) }`), and the Plan 2 fallback note applies (one `.dupes-ignore.toml` family entry in the final task).
- Uniform rename: append `Request` to every marker struct (`Hover`→`HoverRequest` done in Plan 2). No other renames.
- Bodies move **verbatim** — the only sanctioned edits are: (a) the composition rule: when an impl's `modify_params` starts with a single `convert_position`/`convert_range` on one field and continues with other work, it becomes `incoming_position`/`incoming_range` + `incoming_custom(rest)`, standard conversion first (signature_help is the only case); (b) de-aliasing per the no-alias rule below — imports drop `as Lsp…` renames and signatures use the real type names, statements unchanged.
- **No `use … as Name` import aliases.** An alias exists only to resolve a genuine name collision — two same-named types that must coexist in scope (the BaseCar rule, owner 2026-09-05); `as _` trait imports are not aliases. The `Request` rename dissolves the one real collision in `src/requests/` (`SignatureHelp` marker vs `lsp_types::SignatureHelp`), so every `as Lsp…` import in the rewritten files disappears; the surviving collision pairs (`Ts…`/`Lsp…` across `text_utils`/`tree_sitter_utils`, and the public `ErrorCode as ServerErrorCode` facade re-export) are whitelisted in the Task 9 sweep.
- Free-fn naming convention in request files: `fn convert_params(state, document, params)` for `incoming_custom`, `fn convert_response(state, document, response)` for `outgoing`, `fn convert_params_standalone(state, params)` / `fn convert_response_standalone(state, response)` for the standalone pair. When a `conversion.rs` helper already has the exact hook signature (three args in hook order), wire its full path directly instead of a local wrapper (`supertypes`, `subtypes`, `signature_help` outgoing).
- Dupes gate protocol (memory + spec decision 4): zero new entries by construction; any surfaced group gets an avoidance analysis before an entry is even considered.
- Battery after every task: `cargo build --all-targets && cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`; the three-configuration battery (`--no-default-features`, `--all-features`) runs in Tasks 2, 7, and 9.
- On any failure: `superpowers:systematic-debugging` + `no-workarounds`; root cause, never suppression. English only. MSRV 1.88, pinned toolchain.

## The per-request recipe (used by Tasks 1–6)

For each request, five edits (order matters for compilation):

1. **Registry**: delete the row from `src/requests/registry.rs` (source of the row's data — copy `doc:`, `params:`, `response:`, and hook fields out BEFORE deleting; for custom/resolve rows the registry carries only doc/params/response, the hooks come from the file's impl).
2. **File** `src/requests/<name>.rs`: place the attribute + struct at the top (before `#[cfg(test)]`), port the impl hooks per the mapping table below, keep tests byte-identical except the struct name.
3. **Trait** `src/server/server_trait.rs`: add the `lsp_method!` (or `lsp_resolve_method!`) block with the row's `doc:` value verbatim as `///` and the full type paths in the signature.
4. **Dispatch** `src/server/with_state/mod.rs`: add one row to the growing `lsp_dispatch!` block (seeded by Plan 2 with hover).
5. **Re-exports** `src/requests/mod.rs`: add (generated) or rename (custom/resolve) the `pub(crate) use <module>::<Name>Request;` line, then `grep -rn '\b<OldName>\b' src/ examples/` and fix stragglers (battery catches the rest).

**Registry-row → attribute mapping** (generated rows):

| row field | attribute field |
|---|---|
| `params: T` | `params = T` |
| `response: T` | `response = T` |
| `document: a.b` | `document(a.b)` |
| `incoming: position at p` | `incoming_position(p)` |
| `incoming: range at r` | `incoming_range(r)` |
| `outgoing: fun` | `outgoing(crate::requests::conversion::fun)` |
| field absent | field absent |

**Worked example** — `declaration` (the shape of all 30 generated migrations; hover in Plan 2 Task 5 is the lived reference):

```rust
// src/requests/declaration.rs (file exists with tests only; add on top):
#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::request::GotoDeclarationParams,
    response = Option<async_lsp::lsp_types::request::GotoDeclarationResponse>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_goto_response),
)]
pub(crate) struct DeclarationRequest;
// tests below: rename `Declaration` → `DeclarationRequest` in the
// conversion_tests! rows' type position and any `use` line.
```

```rust
// src/server/server_trait.rs (inside pub trait Server, keeping registry order):
    lsp_method! {
        /// Handles `textDocument/declaration` requests from the client.
        ///
        /// Returns the declaration locations of the symbol at the position in `params`, or `None`. Requires a declaration provider in [`Server::server_capabilities`].
        fn declaration(
            &self,
            _state: ServerState,
            _params: async_lsp::lsp_types::request::GotoDeclarationParams,
        ) -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::request::GotoDeclarationResponse>>> + Send;
    }
```

```rust
// src/server/with_state/mod.rs (one more row in the lsp_dispatch! block):
        declaration: declaration @ crate::requests::DeclarationRequest,
```

---

### Task 1: Generated requests, part 1 (15)

**Files:** per-recipe ×15: `registry.rs`, the 15 files below, `server_trait.rs`, `with_state/mod.rs`, `requests/mod.rs`.

**Interfaces:** Consumes the Plan 2 macros and hover pattern. Produces 15 migrated requests; `lsp_dispatch!` grows to 16 rows.

- [ ] **Step 1: Migrate, in registry order:** `declaration`, `definition`, `references`, `link` (alsp `document_link`), `rename`, `rename_prepare` (alsp `prepare_rename`), `document_format` (alsp `formatting`), `document_range_format` (alsp `range_formatting`), `implementation`, `type_definition`, `document_highlight`, `on_type_formatting`, `folding_range`, `linked_editing_range`, `code_lens`. New struct names: the capitalized row type + `Request` (`DocumentLinkRequest`, `RenamePrepareRequest`, `DocumentFormatRequest`, `DocumentRangeFormatRequest`, …). Note the attribute field presence per row: `moniker`-style omissions don't occur here except `link`/`folding_range`/`code_lens` (no incoming), `rename` (position), `document_range_format` (range) — the registry row itself is the authority.
- [ ] **Step 2: Battery** (default features + fmt + clippy + doc per Global Constraints). Expected: green; the conversion tests of all 15 pass through attribute-stamped impls.
- [ ] **Step 3: Checkpoint (owner commits)** — `registry.rs`, 15 request files, `server_trait.rs`, `with_state/mod.rs`, `requests/mod.rs`.

### Task 2: Generated requests, part 2 (15)

**Files:** per-recipe ×15 (same five touchpoints).

**Interfaces:** Produces a fully drained `generated_methods!` table (empty shell).

- [ ] **Step 1: Migrate:** `will_save_wait_until`, `document_color`, `color_presentation` (range incoming), `prepare_call_hierarchy` (`CallHierarchyPrepareRequest`), `prepare_type_hierarchy` (`TypeHierarchyPrepareRequest`), `moniker` (**no outgoing**), `will_create_files`, `will_rename_files`, `will_delete_files` (**no document**, outgoing present), `inlay_hint` (range), `document_symbol`, `execute_command` (**no document, no incoming, no outgoing**), `semantic_tokens_full`, `semantic_tokens_range` (range), `semantic_tokens_full_delta` (`SemanticTokensFullDeltaRequest`).
- [ ] **Step 2: Full three-configuration battery.** Expected: green.
- [ ] **Step 3: Checkpoint.**

### Task 3: Custom five — code_action, completion, document_diagnostics, inline_value, selection_range

**Files:** `registry.rs` (5 rows from `custom_methods!`), the 5 files, `server_trait.rs` (5 `lsp_method!` blocks), `with_state/mod.rs` (5 rows), `requests/mod.rs`.

**Interfaces:** Produces the custom-migration shape: `document(...)` + `incoming_custom(self::convert_params)` and/or `outgoing(self::convert_response)` with bodies moved verbatim.

- [ ] **Step 1: `completion.rs` in full** (the worked custom example; current impl has extract_url + incoming position + custom outgoing — note it KEEPS the standard incoming field):

```rust
use async_lsp::lsp_types::{CompletionParams, CompletionResponse};

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_completion_text_edit, convert_text_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CompletionParams,
    response = Option<async_lsp::lsp_types::CompletionResponse>,
    document(text_document_position.text_document),
    incoming_position(text_document_position.position),
    outgoing(self::convert_response),
)]
pub(crate) struct CompletionRequest;

/// Converts completion edits in the response back to the client encoding
/// (the outgoing hook; body unchanged from the pre-migration impl).
fn convert_response(state: &ServerState, document: &Document, response: &mut Option<CompletionResponse>) {
    if let Some(response) = response.as_mut() {
        let items = match response {
            CompletionResponse::Array(v) => v,
            CompletionResponse::List(v) => v.items.as_mut(),
        };
        for item in items {
            if let Some(edit) = item.text_edit.as_mut() {
                convert_completion_text_edit(state, document, edit, Direction::Outgoing);
            }
            if let Some(edits) = item.additional_text_edits.as_mut() {
                for edit in edits {
                    convert_text_edit(state, document, edit, Direction::Outgoing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // unchanged except: `use crate::requests::CompletionRequest;` and
    // `<CompletionRequest as Request>` (two sites in the hand-written test)
    // … existing body …
}
```

- [ ] **Step 2: The other four** — same recipe, hooks per their current impls:

```text
code_action:          document(text_document), incoming_custom(self::convert_params),
                      outgoing(self::convert_response)
                      (params body: convert_range on params.range + the context
                       diagnostics loop; response body: the actions loop — both verbatim)
document_diagnostics: document(text_document), outgoing(self::convert_response)   [no incoming]
selection_range:      document(text_document), incoming_custom(self::convert_params),
                      outgoing(self::convert_response)
inline_value:         document(text_document), incoming_custom(self::convert_params),
                      outgoing(self::convert_response)
```

Struct renames: `CodeActionRequest`, `DocumentDiagnosticsRequest`, `SelectionRangeRequest`, `InlineValueRequest`; tests rename their `use super::{…}` accordingly. Trait blocks use `lsp_method!` with the rows' docs verbatim (alsp names: `code_action`, `document_diagnostic`, `selection_range`, `inline_value` — note `document_diagnostics: document_diagnostic`).

- [ ] **Step 3: Battery + checkpoint** — the five files' existing W0 tests (they call the hooks directly) now exercise the attribute-wired free fns.

### Task 4: Hierarchy/calls quartet — incoming_calls, outgoing_calls, supertypes, subtypes

**Files:** `registry.rs` (4 rows), the 4 files, `server_trait.rs`, `with_state/mod.rs`, `requests/mod.rs`.

**Interfaces:** Produces the `document(item)` shape (single-segment field path) and the direct-conversion-path wiring.

- [ ] **Step 1: `incoming_calls.rs` in full** (worked example):

```rust
use async_lsp::lsp_types::CallHierarchyIncomingCallsParams;

use crate::server::{Document, ServerState};

use super::conversion::{
    Direction, convert_call_hierarchy_incoming_call, convert_call_hierarchy_item,
    convert_optional_vec,
};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CallHierarchyIncomingCallsParams,
    response = Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
    document(item),
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct IncomingCallsRequest;

/// Converts the item's ranges to UTF-8 (the incoming hook; body verbatim).
fn convert_params(
    state: &ServerState,
    document: &Document,
    params: &mut CallHierarchyIncomingCallsParams,
) {
    convert_call_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
}

/// Converts the returned calls' ranges back (the outgoing hook; body verbatim).
fn convert_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_call_hierarchy_incoming_call,
    );
}

#[cfg(test)]
mod tests {
    // unchanged except `use crate::requests::{IncomingCallsRequest, Request};`
    // and `<IncomingCallsRequest as Request>` (three call sites) … existing body …
}
```

- [ ] **Step 2: The other three:**

```text
outgoing_calls: same shape; convert_response body = convert_optional_vec(...,
                 convert_call_hierarchy_outgoing_call); struct OutgoingCallsRequest
supertypes:     document(item), incoming_custom(self::convert_params) [verbatim
                convert_type_hierarchy_item body], outgoing wires the conversion
                helper DIRECTLY (its signature already matches the hook):
                outgoing(crate::requests::conversion::modify_outgoing_type_hierarchy_items);
                struct SupertypesRequest
subtypes:       identical to supertypes; struct SubtypesRequest
```

Trait `lsp_method!` blocks per rows (docs verbatim); dispatch rows `incoming_calls: incoming_calls @ …`, `outgoing_calls: outgoing_calls @ …`, `supertypes: supertypes @ …`, `subtypes: subtypes @ …`.

- [ ] **Step 3: Battery + checkpoint.**

### Task 5: The two specials — symbol, signature_help

**Files:** `registry.rs` (2 rows), `symbol.rs`, `signature_help.rs`, `server_trait.rs`, `with_state/mod.rs`, `requests/mod.rs`.

**Interfaces:** Produces the standalone-only and composition shapes.

- [ ] **Step 1: `symbol.rs`** — no document, no incoming; the standalone hook body moves verbatim into `convert_locations`; the existing free fn `convert_symbol_location` and its doc stay untouched:

```rust
#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::WorkspaceSymbolParams,
    response = Option<async_lsp::lsp_types::WorkspaceSymbolResponse>,
    outgoing_standalone(self::convert_locations),
)]
pub(crate) struct SymbolRequest;

// Keep the two explanatory comments from the old impl block (no request URL,
// no staleness; state-driven shape where the trait's delegating default keeps
// this hook running in every dispatch state) — as a doc comment on
// `convert_locations` or just above the attribute, whichever reads better.

/// Converts each symbol location against its own document — tracked
/// store-first, else a per-request cached disk snapshot, else left
/// unchanged (the standalone outgoing hook; body verbatim).
fn convert_locations(state: &ServerState, response: &mut Option<WorkspaceSymbolResponse>) {
    // … the current modify_response_standalone body, verbatim …
}
```

(`HashMap`/`Url`/`Direction`/`convert_range` imports and `convert_symbol_location` stay; tests rename `Symbol` → `SymbolRequest`.)

- [ ] **Step 2: `signature_help.rs`** — the composition case: the impl's `modify_params` begins with one `convert_position` on `text_document_position_params.position` (becomes `incoming_position`) followed by the label-offsets block (becomes `incoming_custom`); the outgoing helper already matches the hook signature, wired directly:

```rust
#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::SignatureHelpParams,
    response = Option<async_lsp::lsp_types::SignatureHelp>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    incoming_custom(self::convert_context_label_offsets),
    outgoing(crate::requests::conversion::modify_outgoing_signature_help),
)]
pub(crate) struct SignatureHelpRequest;

/// Converts the echoed context help's label offsets to UTF-8 (the custom
/// incoming step, composed after the standard position conversion; body
/// verbatim apart from the extracted leading position conversion).
fn convert_context_label_offsets(
    state: &ServerState,
    document: &Document,
    params: &mut SignatureHelpParams,
) {
    if let Some(help) = params
        .context
        .as_mut()
        .and_then(|context| context.active_signature_help.as_mut())
    {
        convert_signature_help_label_offsets(state, document, help, Direction::Incoming);
    }
}
```

(Tests: `use crate::requests::SignatureHelpRequest;` replacing the `SignatureHelp as SignatureHelpRequest` alias; the `conversion_tests!` row type becomes `SignatureHelpRequest`.)

- [ ] **Step 3: Battery + checkpoint** — with Tasks 3–5 covering 5 + 4 + 2 rows, `custom_methods!` is now an empty shell; only `resolve_methods!` still carries rows.

### Task 6: Resolve six

**Files:** `registry.rs` (all 6 `resolve_methods!` rows), the 6 files, `server_trait.rs` (6 `lsp_resolve_method!` blocks — signatures use the final named parameter `item`), `with_state/mod.rs` (6 `resolve(...)` rows), `requests/mod.rs`.

**Interfaces:** Produces the drained `resolve_methods!` table; all 48 rows now live in `lsp_dispatch!`.

- [ ] **Step 1: `completion_resolve.rs` in full** (worked example; keep the local `convert_completion_item` fn and its doc untouched):

```rust
use async_lsp::lsp_types::CompletionItem;

use crate::server::{Document, ServerState};

use super::conversion::{Direction, convert_completion_text_edit, convert_text_edit};

#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::CompletionItem,
    response = async_lsp::lsp_types::CompletionItem,
    incoming_custom(self::convert_params),
    outgoing(self::convert_response),
)]
pub(crate) struct CompletionResolveRequest;

// CompletionItem doesn't contain a document URI — keep this comment; the
// resolve dispatch engine supplies the sole tracked document.

/// Converts the item's edits to UTF-8 (the incoming hook; body verbatim).
fn convert_params(state: &ServerState, document: &Document, params: &mut CompletionItem) {
    convert_completion_item(state, document, params, Direction::Incoming);
}

/// Converts the item's edits back to the client encoding (body verbatim).
fn convert_response(state: &ServerState, document: &Document, response: &mut CompletionItem) {
    convert_completion_item(state, document, response, Direction::Outgoing);
}

// … convert_completion_item unchanged …
// … tests unchanged except CompletionResolve → CompletionResolveRequest …
```

- [ ] **Step 2: The other five, from their current impls** (same recipe — hooks to free fns verbatim, `code_lens_resolve` included with its one-line `convert_range` bodies; `workspace_symbol_resolve` is the standalone pair, wrappers over the existing `convert_workspace_symbol_location`):

```text
code_action_resolve:        incoming_custom(self::convert_params), outgoing(self::convert_response)
document_link_resolve:      incoming_custom(self::convert_params), outgoing(self::convert_response)
code_lens_resolve:          incoming_custom(self::convert_params), outgoing(self::convert_response)
inlay_hint_resolve:         incoming_custom(self::convert_params), outgoing(self::convert_response)
workspace_symbol_resolve:   incoming_standalone(self::convert_params_standalone),
                            outgoing_standalone(self::convert_response_standalone)
                            (both wrappers call convert_workspace_symbol_location verbatim;
                             keep the impl-block explanatory comments)
```

Struct renames: `CodeActionResolveRequest`, `DocumentLinkResolveRequest`, `CodeLensResolveRequest`, `InlayHintResolveRequest`, `WorkspaceSymbolResolveRequest`. Trait blocks via `lsp_resolve_method!` with docs verbatim and the final parameter named `item`. Dispatch rows: `resolve(completion_resolve: completion_item_resolve @ crate::requests::CompletionResolveRequest),` etc. (alsp names from the registry rows: `completion_item_resolve`, `code_action_resolve`, `document_link_resolve`, `code_lens_resolve`, `inlay_hint_resolve`, `workspace_symbol_resolve`).

- [ ] **Step 3: Full three-configuration battery + checkpoint.** All three registry tables are now empty shells.

### Task 7: Cutover — the final dispatch table and the deletions

**Files:** `src/server/with_state/mod.rs`, `src/server/server_trait.rs`, `src/requests/mod.rs`, delete `src/requests/registry.rs`.

**Interfaces:** Produces the end state: 48 rows in one `lsp_dispatch!`, zero `macro_rules!` in `src/`, no registry.

- [ ] **Step 1: Finalize the `lsp_dispatch!` table in `with_state/mod.rs`** — replace the growing block with this complete table (registry order; hover row from Plan 2 included):

```rust
    lsp_dispatch! {
        hover: hover @ crate::requests::HoverRequest,
        declaration: declaration @ crate::requests::DeclarationRequest,
        definition: definition @ crate::requests::DefinitionRequest,
        references: references @ crate::requests::ReferencesRequest,
        link: document_link @ crate::requests::DocumentLinkRequest,
        rename: rename @ crate::requests::RenameRequest,
        rename_prepare: prepare_rename @ crate::requests::RenamePrepareRequest,
        document_format: formatting @ crate::requests::DocumentFormatRequest,
        document_range_format: range_formatting @ crate::requests::DocumentRangeFormatRequest,
        implementation: implementation @ crate::requests::ImplementationRequest,
        type_definition: type_definition @ crate::requests::TypeDefinitionRequest,
        document_highlight: document_highlight @ crate::requests::DocumentHighlightRequest,
        on_type_formatting: on_type_formatting @ crate::requests::OnTypeFormattingRequest,
        folding_range: folding_range @ crate::requests::FoldingRangeRequest,
        linked_editing_range: linked_editing_range @ crate::requests::LinkedEditingRangeRequest,
        code_lens: code_lens @ crate::requests::CodeLensRequest,
        will_save_wait_until: will_save_wait_until @ crate::requests::WillSaveWaitUntilRequest,
        document_color: document_color @ crate::requests::DocumentColorRequest,
        color_presentation: color_presentation @ crate::requests::ColorPresentationRequest,
        prepare_call_hierarchy: prepare_call_hierarchy @ crate::requests::CallHierarchyPrepareRequest,
        prepare_type_hierarchy: prepare_type_hierarchy @ crate::requests::TypeHierarchyPrepareRequest,
        moniker: moniker @ crate::requests::MonikerRequest,
        will_create_files: will_create_files @ crate::requests::WillCreateFilesRequest,
        will_rename_files: will_rename_files @ crate::requests::WillRenameFilesRequest,
        will_delete_files: will_delete_files @ crate::requests::WillDeleteFilesRequest,
        inlay_hint: inlay_hint @ crate::requests::InlayHintRequest,
        document_symbol: document_symbol @ crate::requests::DocumentSymbolRequest,
        execute_command: execute_command @ crate::requests::ExecuteCommandRequest,
        semantic_tokens_full: semantic_tokens_full @ crate::requests::SemanticTokensFullRequest,
        semantic_tokens_range: semantic_tokens_range @ crate::requests::SemanticTokensRangeRequest,
        semantic_tokens_full_delta: semantic_tokens_full_delta @ crate::requests::SemanticTokensFullDeltaRequest,
        completion: completion @ crate::requests::CompletionRequest,
        code_action: code_action @ crate::requests::CodeActionRequest,
        document_diagnostics: document_diagnostic @ crate::requests::DocumentDiagnosticsRequest,
        selection_range: selection_range @ crate::requests::SelectionRangeRequest,
        incoming_calls: incoming_calls @ crate::requests::IncomingCallsRequest,
        outgoing_calls: outgoing_calls @ crate::requests::OutgoingCallsRequest,
        supertypes: supertypes @ crate::requests::SupertypesRequest,
        subtypes: subtypes @ crate::requests::SubtypesRequest,
        inline_value: inline_value @ crate::requests::InlineValueRequest,
        symbol: symbol @ crate::requests::SymbolRequest,
        signature_help: signature_help @ crate::requests::SignatureHelpRequest,
        resolve(completion_resolve: completion_item_resolve @ crate::requests::CompletionResolveRequest),
        resolve(code_action_resolve: code_action_resolve @ crate::requests::CodeActionResolveRequest),
        resolve(link_resolve: document_link_resolve @ crate::requests::DocumentLinkResolveRequest),
        resolve(code_lens_resolve: code_lens_resolve @ crate::requests::CodeLensResolveRequest),
        resolve(inlay_hint_resolve: inlay_hint_resolve @ crate::requests::InlayHintResolveRequest),
        resolve(workspace_symbol_resolve: workspace_symbol_resolve @ crate::requests::WorkspaceSymbolResolveRequest),
    }
```

- [ ] **Step 2: Delete the old machinery.**
  - `with_state/mod.rs`: the five macro definitions (`implement_method!`, `implement_methods!`, `implement_resolve_method!`, `registry_dispatch!`, `registry_dispatch_resolve!`) and the three registry invocations; `conversion_document` and `read_document_from_disk` STAY (the generated code calls them).
  - `server_trait.rs`: the two stamper definitions and the three registry invocations.
  - `requests/mod.rs`: the three helper macros, `registry_request_impls!`, its invocation, `pub(crate) mod registry;`, and the registry mention in the module docs; the `Request` trait and conversion re-exports stay.
  - Delete `src/requests/registry.rs`.
- [ ] **Step 3: Verify the sweep is total:**

```bash
grep -rn "macro_rules!" src/            # expected: empty
grep -rn "registry" src/                # expected: empty
grep -rnE "^pub (trait|struct)" src/requests/   # expected: empty
```

- [ ] **Step 4: Full three-configuration battery + doc build + `cargo expand -p async-language-server server::with_state` (the 48 dispatch methods must match the pre-cycle expansion modulo the `Request` renames).**
- [ ] **Step 5: Checkpoint.**

### Task 8: Completeness pin, oneshot sentence, normative docs

**Files:** `src/server/tests/` (the wire test, alongside the parametrized unwired test), `README.md`, `src/oneshot/mod.rs`, `.claude/rules/structure.md`, `.claude/rules/testing.md`, `CLAUDE.md`.

**Interfaces:** Consumes the 48-method table. Produces the drift pin and the rewritten rules.

- [ ] **Step 1: The completeness wire test** — in the file holding `unwired_methods_return_method_not_found`, mirror its parametrized shape and minimally-valid-params fixture: a `wired_methods_dispatch` test iterating all 48 method names (the `lsp_dispatch!` table is the list), sending each over the wire harness against `EchoServer`, asserting the response is NOT an error with code `-32601`. The one silent gap of the design (trait method without a dispatch row) closes here; everything else already fails compilation.

- [ ] **Step 2: The oneshot sentence** — in `README.md`'s oneshot Tour bullet and `src/oneshot/mod.rs`'s module doc, add one clarifying sentence: `server` is the capability layer (implement `Server`); `oneshot` is a clientless runner driving the same engine (spec decision 9).

- [ ] **Step 3: Normative docs rewrite.**
  - `.claude/rules/structure.md`: "Adding an LSP method touches three places" becomes — (1) the request file: `#[lsp_request(...)]` + struct + inline tests (`src/requests/<method>.rs`); (2) the trait method: `lsp_method!`/`lsp_resolve_method!` block with the doc (`src/server/server_trait.rs`); (3) one row in the `lsp_dispatch!` table (`src/server/with_state/mod.rs`). "The `Request` pattern" section: the hook list stays; the `request_extract_url!`/`request_modify_params_position!` sentence becomes the attribute-field mapping (`document(...)`, `incoming_position(...)`, `incoming_range(...)`, `incoming_custom(...)`, `outgoing(...)`, standalone pair); the `implement_method!` staleness paragraph names `lsp_dispatch!` instead; drop the registry preamble sentence.
  - `.claude/rules/testing.md`: in "Adding a test for a new `Server` method", the sentence "The `Request` impl uses the shared macros for the common shapes — `request_extract_url!` … `request_modify_params_position!` …" becomes "The `#[lsp_request]` attribute fields cover the common shapes (`document(...)`, `incoming_position(...)`); hand-write hooks only for response-shaped or multi-position methods, as free `convert_*` fns wired through `incoming_custom`/`outgoing`".
  - `CLAUDE.md` Architecture: "The `implement_method!` macro glues…" → "The `lsp_dispatch!` table glues each async-lsp method to a `Server` method through the request's hooks, plus staleness detection…"; "one line in the `implement_methods!` table" → "one row in the `lsp_dispatch!` table".
  - `README.md`: `grep -n "registry\|macro_rules" README.md` — update any hit to the new architecture (expected: none, but verify).
- [ ] **Step 4: Battery (`cargo test` covers the new wire test) + checkpoint.**

### Task 9: Final verification — the whole gate stack

- [ ] **Step 1: Full battery, three configurations, both crates** (the exact CI set).
- [ ] **Step 2: `cargo test --test architecture`** — arch-lint scopes still green (the requests scope lost `registry.rs`; layer rules unchanged).
- [ ] **Step 3: `cargo dupes check`** — expected outcome per spec decision 4: ZERO new entries. The pre-existing entries covering the parallel hook families ("resolve twins", "single-field Request hooks", "call-hierarchy…", "one-line modify_response hooks") now match free fns instead of trait-impl methods; fingerprints may shift as members churn — verify by comment-out probe, update the fingerprint in place, reason text unchanged. If a NEW group appears: write the avoidance analysis FIRST (memory: dupes-ignore-minimal); only a genuinely irreducible family earns an entry, with its reasoning in the file.
- [ ] **Step 4: Import-alias sweep.** `grep -rnE "use .+ as [A-Za-z_]+" src/ | grep -v " as _"` must return ONLY the collision whitelist: the `Ts…`/`Lsp…` pairs in `text_utils/position.rs`, `text_utils/encoding.rs`, `text_utils/range_ext/{lsp,tree_sitter,tree_sitter_tests}.rs`, `tree_sitter_utils.rs` (tree-sitter vs LSP same-named types), and the public `pub use async_lsp::ErrorCode as ServerErrorCode` facade re-export in `error.rs`. Every survivor gains a one-line comment naming the type it collides with — self-documenting justification; any alias that cannot name its collision is removed and the real name used (battery re-run). The `src/requests/` rewrite removes its ~14 stylistic `Lsp…` aliases during Tasks 3–6, and the test-local `X as XRequest` aliases dissolve into direct renamed imports via the recipe.
- [ ] **Step 5: `cargo expand` diff on `with_state`** — already taken in Task 7 Step 4; re-confirm clean after docs (no code change expected).
- [ ] **Step 6: Final checkpoint (owner commits)** — the cycle's working-tree state is the whole branch's deliverable; the end-of-cycle whole-branch review follows per the SDD flow.

---

## Self-Review (performed at plan writing)

- **Spec coverage:** 30 generated (Tasks 1–2), 17 custom/resolve (Tasks 3–6, per-file shapes read from the actual sources at plan time), rename ×48 (recipe + per-task renames), trait ×48 (`lsp_method!`/`lsp_resolve_method!` or the documented fallback), dispatch consolidation + registry/macro deletion (Task 7), completeness pin + oneshot sentence + normative docs (Task 8), battery ×3 + dupes protocol + expand diff + arch-lint (Tasks 7/9). ✓
- **Placeholder scan:** every step carries exact content — full worked files for `declaration`, `completion`, `incoming_calls`, `completion_resolve`, exact attribute shapes for `symbol`/`signature_help`, explicit field lists for the remaining files whose bodies are verbatim moves from in-repo sources, and the verbatim 48-row final table. The wire test in Task 8 mirrors a named in-repo parametrized test with its fixture. ✓
- **Type consistency:** struct names follow the uniform `Request` suffix (48 listed in Task 7's table); alsp names match the registry (`completion_item_resolve`, `document_diagnostic`, `formatting`, `range_formatting`, `document_link`, `prepare_rename`); free-fn names follow the convention in Global Constraints; the `document(item)` single-segment path is exercised by the quartet. ✓
