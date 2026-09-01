# LSP Surface — Plan 2: Method Registry + Mechanical Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `method_registry!` (one registry module, three tables, consumed by three stampers) so adding an LSP method becomes one row; retrofit the 16 wired methods onto it; then land the 27 mechanical methods (20 generated rows + 7 custom-hook files) with their conversion helpers and tests.

**Architecture:** A new `src/requests/registry.rs` holds three token tables (`generated_methods!`, `custom_methods!`, `resolve_methods!`) expanded through the macro-passthrough `table!(stamper)` technique — the same pattern async-lsp's `define!` uses. `server_trait.rs` stamps trait methods, `with_state/mod.rs` stamps dispatch entries (the existing `implement_method!`/`implement_resolve_method!` engines remain), and `requests/mod.rs` stamps `Request` impls for generated rows. Spec: `docs/superpowers/specs/2026-09-01-lsp-surface-completion-design.md`, sections "Architecture 1" and "Roadmap".

**Tech Stack:** Rust edition 2024 (`Future` is in the prelude), `macro_rules!` with `pub(crate) use` exports, existing `request_extract_url!`/`request_modify_params_position!`/`conversion_tests!` macros.

## Global Constraints

- Owner commits — every task ends at a review checkpoint with a file list; no git commands anywhere.
- Rows carry TWO method idents: `$trait_name` (our `Server` method) and `$alsp_name` (async-lsp trait method); they differ for `rename_prepare`/`prepare_rename`, `document_format`/`formatting`, `document_range_format`/`range_formatting`, `document_diagnostics`/`document_diagnostic`, `link`/`document_link`.
- Types in rows are written as full paths (`async_lsp::lsp_types::...`) — rows expand in three different scopes.
- UTF-8 invariant: handlers never convert; conversion lives only in `Request` hooks and `src/requests/conversion.rs`. Staleness comes only from `implement_method!`.
- Trait methods use RPITIT exactly like the existing ones: `fn $trait_name(&self, _state: ServerState, _params: $params) -> impl Future<Output = ServerResult<$response>> + Send`.
- Every new public trait method needs a `doc:` literal in house style: names the wire method, says what it returns, names the capability requirement (`Requires a X provider in [`Server::server_capabilities`].`). Resolve methods' docs additionally document the sole-document conversion behavior (copy the pattern from the existing `completion_resolve` doc).
- `Request` trait (exact, `src/requests/mod.rs:71-82`): `type Params; type Response;` and hooks `extract_url(&Self::Params) -> Option<Url>`, `modify_params(&ServerState, &Document, &mut Self::Params)`, `modify_response(&ServerState, &Document, &mut Self::Response)`.
- Tests: `conversion_tests!` rows wherever the pin is a Position (fixture semantics: params built against the emoji document URL; load-bearing columns client UTF-16 2 ↔ UTF-8 byte 4; response closures receive `(plain_url, emoji_url)`); hand-written W0 for selection_range (multi-position), signature_help (label offsets), symbol (URL-less disk-fallback), inline_value (two incoming ranges), the four item-carrying hierarchy methods (incoming item conversion), and the file-ops trio (deep `WorkspaceEdit` shapes). Typed defaults (`WorkDoneProgressParams::default()` etc.), no `use crate::requests::Request;` in test modules (macro uses `$crate::requests::Request`).
- All three feature configurations compile and pass; no `#[allow]`/suppressions; production `src/` unwrap/expect-clean; `# Errors`/`# Panics` not needed (helpers return `()`, never panic — conversions pass through on miss).
- `cargo dupes check` exit 0; one reasoned `.dupes-ignore.toml` entry per genuinely-deliberate group only, never per-row.
- No per-plan final review (owner decision 2026-09-02): one end-of-cycle whole-branch review after Plan 3.

---

### Task 1: The registry + `request_modify_params_range!` + retrofit of the 16 wired methods

**Files:**
- Create: `src/requests/registry.rs`
- Modify: `src/requests/mod.rs` (declare module, add `request_modify_params_range!`, stamper invocation, delete the 10 migrated per-method modules' impls — their files keep only `#[cfg(test)]` blocks where present)
- Modify: `src/server/server_trait.rs` (delete the 15 hand-written method bodies; keep the 4 config methods; stamp from the registry)
- Modify: `src/server/with_state/mod.rs` (replace both hand-maintained tables with stamper invocations)
- Modify: `src/requests/conversion.rs` (six thin outgoing helpers)
- Delete impl bodies from: `hover.rs`, `declaration.rs`, `definition.rs`, `references.rs`, `document_link.rs`, `rename.rs`, `rename_prepare.rs`, `document_format.rs`, `document_range_format.rs` (keep tests; delete files entirely only if they have no tests and no other content)

**Interfaces:**
- Consumes: `request_extract_url!`, `request_modify_params_position!`, `conversion_tests!`, `implement_method!`, `implement_resolve_method!`, all `convert_*` helpers in `conversion.rs`.
- Produces (exact): `crate::requests::registry::{generated_methods, custom_methods, resolve_methods}` (each `pub(crate) use`-exported, each invoked as `table!(stamper_name)`); `request_modify_params_range!` in `src/requests/mod.rs` (mirror of the position macro delegating to `convert_range` with `Direction::Incoming`); conversion helpers `modify_outgoing_hover(&ServerState, &Document, &mut Option<async_lsp::lsp_types::Hover>)`, `modify_outgoing_locations(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::Location>>>)`, `modify_outgoing_document_links(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::DocumentLink>>)`, `modify_outgoing_text_edits(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::TextEdit>>)`, `modify_outgoing_workspace_edit(&ServerState, &Document, &mut Option<async_lsp::lsp_types::WorkspaceEdit>)`, `modify_outgoing_prepare_rename_response(&ServerState, &Document, &mut Option<async_lsp::lsp_types::PrepareRenameResponse>)`.

- [ ] **Step 1: `request_modify_params_range!` in `src/requests/mod.rs`**

After `request_modify_params_position!`:

```rust
/// Implements [`Request::modify_params`] inside an existing `impl Request`
/// block, for a request whose params carry one incoming range at the given
/// field path, e.g. `range`: the generated body delegates to `convert_range`
/// with `Direction::Incoming`.
macro_rules! request_modify_params_range {
    ($($segment:ident).*) => {
        fn modify_params(
            state: &crate::server::ServerState,
            document: &crate::server::Document,
            params: &mut Self::Params,
        ) {
            crate::requests::conversion::convert_range(
                state,
                document,
                &mut params $(.$segment)*,
                crate::requests::conversion::Direction::Incoming,
            );
        }
    };
}
```

- [ ] **Step 2: Create `src/requests/registry.rs` with the three tables holding all 16 retrofitted rows**

```rust
//! The method registry: the single source of truth for (trait method,
//! async-lsp method, Request type, params/response types, doc, hook shape).
//!
//! Three tables, each expanded through a consumer's stamper macro via the
//! `table!(stamper)` passthrough — the same pattern async-lsp's `define!`
//! uses one level down. `generated_methods!` rows fully determine a
//! `Request` impl (extract-url path, incoming hook, outgoing helper);
//! `custom_methods!` rows only bind names/types — the hooks live in the
//! per-method file under `src/requests/`; `resolve_methods!` rows bind the
//! resolve trio. Consumers: `server_trait.rs` (trait methods),
//! `with_state/mod.rs` (dispatch), `requests/mod.rs` (generated impls).
//!
//! Rows carry TWO method idents: the `Server` trait method and the
//! async-lsp trait method (they differ for rename_prepare/prepare_rename,
//! document_format/formatting, document_range_format/range_formatting,
//! document_diagnostics/document_diagnostic, link/document_link). Types are
//! full paths because rows expand in three different scopes.

macro_rules! generated_methods {
    ($m:ident) => {
        $m! {
            hover: hover @ Hover {
                doc: "Handles `textDocument/hover` requests from the client.\n\nReturns hover contents for the position in `params`, or `None` when there is nothing to show. Positions and ranges are UTF-8. Requires a hover provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::HoverParams,
                response: Option<async_lsp::lsp_types::Hover>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_hover,
            }
            declaration: declaration @ Declaration {
                doc: "Handles `textDocument/declaration` requests from the client.\n\nReturns the declaration locations of the symbol at the position in `params`, or `None`. Requires a declaration provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::request::GotoDeclarationParams,
                response: Option<async_lsp::lsp_types::request::GotoDeclarationResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            definition: definition @ Definition {
                doc: "Handles `textDocument/definition` requests from the client.\n\nReturns the definition locations of the symbol at the position in `params`, or `None`. Requires a definition provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::GotoDefinitionParams,
                response: Option<async_lsp::lsp_types::GotoDefinitionResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            references: references @ References {
                doc: "Handles `textDocument/references` requests from the client.\n\nReturns the locations that reference the symbol at the position in `params`, or `None`. Requires a references provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::ReferenceParams,
                response: Option<Vec<async_lsp::lsp_types::Location>>,
                document: text_document_position.text_document,
                incoming: position at text_document_position.position,
                outgoing: modify_outgoing_locations,
            }
            link: document_link @ DocumentLink {
                doc: "Handles `textDocument/documentLink` requests from the client.\n\nReturns links inside the document in `params`, or `None`. Requires a document link provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentLinkParams,
                response: Option<Vec<async_lsp::lsp_types::DocumentLink>>,
                document: text_document,
                outgoing: modify_outgoing_document_links,
            }
            rename: rename @ Rename {
                doc: "Handles `textDocument/rename` requests from the client.\n\nReturns a workspace edit renaming the symbol at the position in `params` to `params.new_name`, or `None` when renaming is not possible. Requires a rename provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::RenameParams,
                response: Option<async_lsp::lsp_types::WorkspaceEdit>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_workspace_edit,
            }
            rename_prepare: prepare_rename @ RenamePrepare {
                doc: "Handles `textDocument/prepareRename` requests from the client.\n\nReturns the range of the symbol at the position in `params` that a rename would apply to, or `None` when renaming is not possible. Requires a rename provider with `prepare_provider` enabled.",
                params: async_lsp::lsp_types::TextDocumentPositionParams,
                response: Option<async_lsp::lsp_types::PrepareRenameResponse>,
                document: text_document,
                incoming: position at position,
                outgoing: modify_outgoing_prepare_rename_response,
            }
            document_format: formatting @ DocumentFormat {
                doc: "Handles `textDocument/formatting` requests from the client.\n\nReturns edits formatting the whole document in `params`, or `None`. Requires a document formatting provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentFormattingParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document,
                outgoing: modify_outgoing_text_edits,
            }
            document_range_format: range_formatting @ DocumentRangeFormat {
                doc: "Handles `textDocument/rangeFormatting` requests from the client.\n\nReturns edits formatting the range in `params`, or `None`. Requires a document range formatting provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentRangeFormattingParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document,
                incoming: range at range,
                outgoing: modify_outgoing_text_edits,
            }
        }
    };
}
pub(crate) use generated_methods;

macro_rules! custom_methods {
    ($m:ident) => {
        $m! {
            completion: completion @ Completion {
                doc: "Handles `textDocument/completion` requests from the client.\n\nReturns completion items at the position in `params`, or `None`. Requires a completion provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CompletionParams,
                response: Option<async_lsp::lsp_types::CompletionResponse>,
            }
            code_action: code_action @ CodeAction {
                doc: "Handles `textDocument/codeAction` requests from the client.\n\nReturns code actions available for the range in `params`, or `None`. Requires a code action provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CodeActionParams,
                response: Option<async_lsp::lsp_types::CodeActionResponse>,
            }
            document_diagnostics: document_diagnostic @ DocumentDiagnostics {
                doc: "Handles `textDocument/diagnostic` requests from the client.\n\nReturns the diagnostics for the document in `params`. The document's current snapshot is available through `state.document(&params.text_document.uri)`. Requires a diagnostic provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentDiagnosticParams,
                response: async_lsp::lsp_types::DocumentDiagnosticReportResult,
            }
        }
    };
}
pub(crate) use custom_methods;

macro_rules! resolve_methods {
    ($m:ident) => {
        $m! {
            completion_resolve: completion_item_resolve @ CompletionResolve {
                doc: "Handles `completionItem/resolve` requests from the client.\n\nFills in additional detail on an item previously returned by [`Server::completion`]. The default implementation resolves the item unchanged; returning the item as-is is always valid. Requires a completion provider with `resolve_provider` enabled. Positions in the incoming item are converted to UTF-8 before the handler runs, and positions in returned edits are converted back to the negotiated encoding — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
                params: async_lsp::lsp_types::CompletionItem,
                response: async_lsp::lsp_types::CompletionItem,
            }
            code_action_resolve: code_action_resolve @ CodeActionResolve {
                doc: "Handles `codeAction/resolve` requests from the client.\n\nFills in additional detail on an action previously returned by [`Server::code_action`]. The default implementation resolves the action unchanged. Requires a code action provider with `resolve_provider` enabled. Positions in the incoming action are converted to UTF-8 before the handler runs, and positions in returned edits are converted back to the negotiated encoding — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
                params: async_lsp::lsp_types::CodeAction,
                response: async_lsp::lsp_types::CodeAction,
            }
            link_resolve: document_link_resolve @ DocumentLinkResolve {
                doc: "Handles `documentLink/resolve` requests from the client.\n\nFills in the target of a link previously returned by [`Server::link`]. The default implementation resolves the link unchanged. Requires a document link provider with `resolve_provider` enabled. The range in the incoming link is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise it passes through unchanged.",
                params: async_lsp::lsp_types::DocumentLink,
                response: async_lsp::lsp_types::DocumentLink,
            }
        }
    };
}
pub(crate) use resolve_methods;
```

(If reading a migrated file reveals a hook irregularity the `generated` grammar cannot express, flip that row to `custom_methods!` — identical doc/params/response, drop the hook fields — and keep the file's impl. Record the flip.)

- [ ] **Step 3: The three stampers**

In `src/requests/mod.rs` (after the existing macros; `mod registry;` + `use` as needed):

```rust
/// Stamps `Request` impls for the registry's generated rows.
macro_rules! registry_request_impls {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
            $(document: $($dseg:ident).+,)?
            $(incoming: position at $($pseg:ident).+,)?
            $(incoming: range at $($rseg:ident).+,)?
            $(outgoing: $outgoing:ident,)?
        }
    )*) => {
        $(
            pub struct $req;

            impl Request for $req {
                type Params = $params;
                type Response = $response;

                $(request_extract_url!($($dseg).+);)?
                $(request_modify_params_position!($($pseg).+);)?
                $(request_modify_params_range!($($rseg).+);)?
                $(
                fn modify_response(
                    state: &crate::server::ServerState,
                    document: &crate::server::Document,
                    response: &mut Self::Response,
                ) {
                    $crate::requests::conversion::$outgoing(state, document, response);
                }
                )?
            }
        )*
    };
}

crate::requests::registry::generated_methods!(registry_request_impls);
```

In `src/server/server_trait.rs` — delete the 15 hand-written method bodies (keep the 4 config methods and `method_not_implemented`), then:

```rust
/// Stamps `Server` trait methods for registry rows (normal methods default
/// to `METHOD_NOT_FOUND`; hook fields are matched and discarded here).
macro_rules! registry_trait_methods {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
            $(document: $($dseg:ident).+,)?
            $(incoming: position at $($pseg:ident).+,)?
            $(incoming: range at $($rseg:ident).+,)?
            $(outgoing: $outgoing:ident,)?
        }
    )*) => {
        $(
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                _params: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                method_not_implemented(stringify!($trait_name))
            }
        )*
    };
}

/// Stamps `Server` trait methods for resolve rows (default: item unchanged).
macro_rules! registry_trait_resolve_methods {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
        }
    )*) => {
        $(
            #[doc = $doc]
            fn $trait_name(
                &self,
                _state: ServerState,
                item: $params,
            ) -> impl Future<Output = ServerResult<$response>> + Send {
                async move { Ok(item) }
            }
        )*
    };
}

crate::requests::registry::generated_methods!(registry_trait_methods);
crate::requests::registry::custom_methods!(registry_trait_methods);
crate::requests::registry::resolve_methods!(registry_trait_resolve_methods);
```

(Note: `registry_trait_methods!` must ALSO match `custom_methods!` rows — same matcher works: custom rows simply have no hook fields, all of which are optional. Declare `custom_methods` before use if import ordering requires; `server_trait.rs` needs no new imports — types come as full paths and `ServerState`/`ServerResult`/`method_not_implemented`/`Future` are already in scope.)

In `src/server/with_state/mod.rs` — replace both hand-maintained tables:

```rust
/// Stamps dispatch entries for registry rows through the existing engine.
macro_rules! registry_dispatch {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
            $(document: $($dseg:ident).+,)?
            $(incoming: position at $($pseg:ident).+,)?
            $(incoming: range at $($rseg:ident).+,)?
            $(outgoing: $outgoing:ident,)?
        }
    )*) => {
        implement_methods!(
            $( $alsp_name => $trait_name @ crate::requests::$req, )*
        );
    };
}

macro_rules! registry_dispatch_resolve {
    ( $(
        $trait_name:ident : $alsp_name:ident @ $req:ident {
            doc: $doc:literal,
            params: $params:ty,
            response: $response:ty,
        }
    )*) => {
        $(
            implement_resolve_method!($alsp_name => $trait_name @ crate::requests::$req);
        )*
    };
}

crate::requests::registry::generated_methods!(registry_dispatch);
crate::requests::registry::custom_methods!(registry_dispatch);
crate::requests::registry::resolve_methods!(registry_dispatch_resolve);
```

- [ ] **Step 4: Six thin outgoing helpers in `src/requests/conversion.rs`**

```rust
/// Converts a hover's optional range from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_hover(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspHover>,
) {
    if let Some(hover) = response
        && let Some(range) = hover.range.as_mut()
    {
        convert_range(state, document, range, Direction::Outgoing);
    }
}

/// Converts each location of a references response from UTF-8 to the
/// client encoding.
pub(crate) fn modify_outgoing_locations(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspLocation>>,
) {
    if let Some(locations) = response {
        for loc in locations {
            convert_location(state, document, loc, Direction::Outgoing);
        }
    }
}

/// Converts each link's range of a documentLink response from UTF-8 to
/// the client encoding.
pub(crate) fn modify_outgoing_document_links(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspDocumentLink>>,
) {
    if let Some(links) = response {
        for link in links {
            convert_range(state, document, &mut link.range, Direction::Outgoing);
        }
    }
}

/// Converts each edit's range of a formatting-family response from UTF-8
/// to the client encoding.
pub(crate) fn modify_outgoing_text_edits(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspTextEdit>>,
) {
    if let Some(edits) = response {
        for edit in edits {
            convert_text_edit(state, document, edit, Direction::Outgoing);
        }
    }
}

/// Converts a rename response's workspace edit from UTF-8 to the client
/// encoding (per-URL against tracked documents, falling back to the
/// request document).
pub(crate) fn modify_outgoing_workspace_edit(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspWorkspaceEdit>,
) {
    if let Some(edit) = response {
        convert_workspace_edit(state, document, edit, Direction::Outgoing);
    }
}

/// Converts a prepareRename response's range from UTF-8 to the client
/// encoding; the placeholder and default-behavior variants carry no
/// positions.
pub(crate) fn modify_outgoing_prepare_rename_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspPrepareRenameResponse>,
) {
    if let Some(response) = response {
        match response {
            LspPrepareRenameResponse::Range(range)
            | LspPrepareRenameResponse::RangeWithPlaceholder { range, .. } => {
                convert_range(state, document, range, Direction::Outgoing);
            }
            LspPrepareRenameResponse::DefaultBehavior { .. } => {}
        }
    }
}
```

(Extend the file's `use async_lsp::lsp_types::{...}` with `Hover as LspHover, DocumentLink as LspDocumentLink, PrepareRenameResponse as LspPrepareRenameResponse` — the existing aliases already cover the rest.)

- [ ] **Step 5: Migrate the ten per-method files**

For each of `hover.rs`, `declaration.rs`, `definition.rs`, `references.rs`, `document_link.rs`, `rename.rs`, `rename_prepare.rs`, `document_format.rs`, `document_range_format.rs`: delete the `pub struct` + `impl Request` block (now stamped); keep `#[cfg(test)] mod tests` exactly as-is; if the file becomes tests-only it stays as the row's test home. In `src/requests/mod.rs` delete the corresponding `mod x;` + `pub(crate) use x::X;` lines for tests-only files and add `mod registry;`. The three custom files (`completion.rs`, `code_action.rs`, `document_diagnostics.rs`) and three resolve files keep everything — only their trait/dispatch bindings now come from the tables.

- [ ] **Step 6: Battery**

Run: `cargo build --all-targets && cargo test && cargo test --no-default-features && cargo test --all-features && cargo fmt --check && cargo clippy --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && cargo dupes check`
Expected: all green, identical test counts to pre-task (159 lib + 1 architecture + 12 doctests default), dupes 0/0. Any test-count change means the stamping diverged from the hand-written code — investigate, do not patch over.

- [ ] **Step 7: Review checkpoint**

Report for review. Files: registry.rs (new), requests/mod.rs, server_trait.rs, with_state/mod.rs, conversion.rs, the ten migrated files. The owner commits.

---

### Task 2: Goto cluster — implementation, type_definition, document_highlight

**Files:**
- Modify: `src/requests/registry.rs` (three rows appended to `generated_methods!`)
- Modify: `src/requests/conversion.rs` (one helper)
- Create: `src/requests/implementation.rs`, `src/requests/type_definition.rs`, `src/requests/document_highlight.rs` (test homes)

**Interfaces:**
- Consumes: registry grammar + stampers (Task 1); `modify_outgoing_goto_response` (exists).
- Produces: `modify_outgoing_document_highlights(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::DocumentHighlight>>)`.

- [ ] **Step 1: Helper in `conversion.rs`**

```rust
/// Converts each highlight's range of a documentHighlight response from
/// UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_document_highlights(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspDocumentHighlight>>,
) {
    if let Some(highlights) = response {
        for highlight in highlights {
            convert_range(state, document, &mut highlight.range, Direction::Outgoing);
        }
    }
}
```

(`DocumentHighlight as LspDocumentHighlight` added to the alias import.)

- [ ] **Step 2: Three rows in `generated_methods!`** (docs in house style; both goto methods reuse `GotoDefinitionParams`-shaped aliases per the verification report — `GotoImplementationParams`/`GotoTypeDefinitionParams` are aliases of `GotoDefinitionParams`, and the responses alias `GotoDefinitionResponse`)

```rust
implementation: implementation @ Implementation {
    doc: "Handles `textDocument/implementation` requests from the client.\n\nReturns the implementation locations of the symbol at the position in `params`, or `None`. Requires an implementation provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::request::GotoImplementationParams,
    response: Option<async_lsp::lsp_types::request::GotoImplementationResponse>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_goto_response,
}
type_definition: type_definition @ TypeDefinition {
    doc: "Handles `textDocument/typeDefinition` requests from the client.\n\nReturns the type definition locations of the symbol at the position in `params`, or `None`. Requires a type definition provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::request::GotoTypeDefinitionParams,
    response: Option<async_lsp::lsp_types::request::GotoTypeDefinitionResponse>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_goto_response,
}
document_highlight: document_highlight @ DocumentHighlight {
    doc: "Handles `textDocument/documentHighlight` requests from the client.\n\nReturns the highlights of the symbol at the position in `params`, or `None`. Requires a document highlight provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::DocumentHighlightParams,
    response: Option<Vec<async_lsp::lsp_types::DocumentHighlight>>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_document_highlights,
}
```

- [ ] **Step 3: Test rows** — create each file as:

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::Implementation;

    conversion_tests! {
        implementation_round_trips_both_directions: Implementation {
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

(The struct-literal defaults above use typed `WorkDoneProgressParams::default()` / `PartialResultParams::default()` — the Plan 1 D4 rule; import those two types alongside the rest.)

`type_definition.rs` and `document_highlight.rs`: same skeleton, `use super::TypeDefinition;` / `use super::DocumentHighlight;`; `document_highlight.rs` swaps params to `DocumentHighlightParams { text_document_position_params, work_done_progress_params: WorkDoneProgressParams::default(), partial_result_params: PartialResultParams::default() }` (same field spellings) and the response to `Some(vec![DocumentHighlight { range: same_line(0, 4, 4), kind: None }])` with getter `|r| r.as_ref().expect("highlights present")[0].range.start`.

- [ ] **Step 4: Battery + checkpoint** — `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo dupes check`; report (files: registry.rs, conversion.rs, three new test files; owner commits).

---

### Task 3: Edits cluster — on_type_formatting, will_save_wait_until, will_create/rename/delete_files

**Files:**
- Modify: `src/requests/registry.rs` (five rows), `src/requests/conversion.rs` (file-ops helper)
- Create: `src/requests/on_type_formatting.rs`, `will_save_wait_until.rs`, `will_create_files.rs`, `will_rename_files.rs`, `will_delete_files.rs` (test homes; the three file-ops carry hand-written tests)

**Interfaces:**
- Consumes: `modify_outgoing_text_edits`, `modify_outgoing_workspace_edit` (Task 1).
- Produces: `modify_outgoing_file_ops_edit(&ServerState, &Document, &mut Option<async_lsp::lsp_types::WorkspaceEdit>)` — identical body to `modify_outgoing_workspace_edit`; NOT created — reuse `modify_outgoing_workspace_edit` directly (rows name it).

- [ ] **Step 1: Five rows**

```rust
on_type_formatting: on_type_formatting @ OnTypeFormatting {
    doc: "Handles `textDocument/onTypeFormatting` requests from the client.\n\nReturns edits formatting around the typed character at the position in `params`, or `None`. Requires a document on-type formatting provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::DocumentOnTypeFormattingParams,
    response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
    document: text_document_position.text_document,
    incoming: position at text_document_position.position,
    outgoing: modify_outgoing_text_edits,
}
will_save_wait_until: will_save_wait_until @ WillSaveWaitUntil {
    doc: "Handles `textDocument/willSaveWaitUntil` requests from the client.\n\nReturns edits applied to the document before it is saved, or `None`. Requires `will_save_wait_until` enabled in the text-document sync options of [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::WillSaveTextDocumentParams,
    response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
    document: text_document,
    outgoing: modify_outgoing_text_edits,
}
will_create_files: will_create_files @ WillCreateFiles {
    doc: "Handles `workspace/willCreateFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are created, or `None`. Requires `workspace.fileOperations.willCreate` in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::CreateFilesParams,
    response: Option<async_lsp::lsp_types::WorkspaceEdit>,
    outgoing: modify_outgoing_workspace_edit,
}
will_rename_files: will_rename_files @ WillRenameFiles {
    doc: "Handles `workspace/willRenameFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are renamed, or `None`. Requires `workspace.fileOperations.willRename` in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::RenameFilesParams,
    response: Option<async_lsp::lsp_types::WorkspaceEdit>,
    outgoing: modify_outgoing_workspace_edit,
}
will_delete_files: will_delete_files @ WillDeleteFiles {
    doc: "Handles `workspace/willDeleteFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are deleted, or `None`. Requires `workspace.fileOperations.willDelete` in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::DeleteFilesParams,
    response: Option<async_lsp::lsp_types::WorkspaceEdit>,
    outgoing: modify_outgoing_workspace_edit,
}
```

(Note `on_type_formatting`'s flattened field is `text_document_position` per the verification report — `DocumentOnTypeFormattingParams` flattens `TextDocumentPositionParams` under that name and carries no work-done params.)

- [ ] **Step 2: Test rows** — `on_type_formatting.rs` and `will_save_wait_until.rs` get `conversion_tests!` rows:

```rust
// on_type_formatting.rs
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentOnTypeFormattingParams, FormattingOptions, TextDocumentIdentifier,
        TextDocumentPositionParams, TextEdit,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::OnTypeFormatting;

    conversion_tests! {
        on_type_formatting_round_trips_both_directions: OnTypeFormatting {
            params: |uri| DocumentOnTypeFormattingParams {
                text_document_position: TextDocumentPositionParams::new(
                    TextDocumentIdentifier::new(uri),
                    line_position(0, 2),
                ),
                ch: "{".into(),
                options: FormattingOptions::default(),
            },
            incoming: |p| p.text_document_position.position,
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

`will_save_wait_until.rs`: same skeleton with `WillSaveTextDocumentParams { text_document: TextDocumentIdentifier::new(uri), reason: TextDocumentSaveReason::Manual }`, NO `incoming:` block, outgoing pair identical.

- [ ] **Step 3: Hand-written file-ops tests** — one representative per trio member, same shape (`will_create_files.rs` shown; rename uses `FileRename { old_uri, new_uri }`, delete uses `FileDelete { uri }`):

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{CreateFilesParams, FileCreate, TextEdit, WorkspaceEdit};

    use crate::requests::Request;
    use crate::testing::{same_line, state_with_documents};

    use super::WillCreateFiles;

    #[test]
    fn will_create_files_edits_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut changes = HashMap::new();
        changes.insert(
            emoji,
            vec![TextEdit { range: same_line(0, 4, 4), new_text: "x".into() }],
        );
        let mut response = Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        });

        <WillCreateFiles as Request>::modify_response(&state, &document, &mut response);

        let edits = response.expect("edit present").changes.expect("changes present");
        // Keyed at the emoji document: UTF-8 byte 4 converts to client 2.
        assert_eq!(edits.values().next().expect("one file")[0].range, same_line(0, 2, 2));
    }
}
```

- [ ] **Step 4: Battery + checkpoint** — full battery per Global Constraints; report; owner commits.

---

### Task 3.5: Dispatch conversion fallbacks (engine)

Added after the Task 3 review confirmed the engine gap (owner decision
2026-09-02): URL-less requests never run their stamped `outgoing:` hook in
dispatch, and untracked-URL requests pass positions through unconverted.
Spec section "Architecture 1.5 — Dispatch conversion fallbacks" is the
contract.

**Files:**
- Modify: `src/server/with_state/mod.rs` (implement_method! + two helpers)
- Test: `src/server/with_state/tests.rs` (three dispatch tests), `src/requests/conversion.rs` (document_changes pin)

**Interfaces:**
- Produces: `fn conversion_document(state: &ServerState, url: Option<&Url>) -> Option<Document>` and `fn read_document_from_disk(url: &Url) -> Option<Document>` (both private to `with_state`), used by `implement_method!` on BOTH sides of the handler call (params side and response side resolve fresh, mirroring the tracked path's re-fetch).

- [ ] **Step 1: The engine change** — in `implement_method!`, replace both `if let Some(url) = url.as_ref() { if let Some(doc) = state.document(url) {` blocks:

```rust
// Params side:
let mut ver: Option<i32> = None;
if let Some(url) = url.as_ref() {
    if let Some(tracked) = state.document(url) {
        ver.replace(tracked.version());
    }
}
let params_doc = conversion_document(&state, url.as_ref());
if let Some(doc) = params_doc.as_ref() {
    <$request_type as crate::requests::Request>::modify_params(&state, doc, &mut params);
}
```

```rust
// Response side (after the handler await):
if let Some(url) = url.as_ref()
    && let Some(tracked) = state.document(url)
    && ver.is_some_and(|v| v != tracked.version())
{
    return Err(ResponseError::new(
        ErrorCode::CONTENT_MODIFIED,
        "document was modified during processing",
    ));
}
if let Some(doc) = conversion_document(&state, url.as_ref()) {
    <$request_type as crate::requests::Request>::modify_response(&state, &doc, &mut result);
}
```

And the two helpers (above or below the macro):

```rust
/// Resolves the document a request's conversions run against: the
/// tracked snapshot for `url` when tracked; otherwise, for file URLs, a
/// per-request snapshot read from disk (best-effort — unreadable or
/// non-file URLs convert nothing, the historical behavior); for URL-less
/// requests, the sole tracked document when exactly one is tracked (the
/// resolve-family heuristic), else none.
fn conversion_document(state: &ServerState, url: Option<&Url>) -> Option<Document> {
    match url {
        Some(url) => state.document(url).or_else(|| read_document_from_disk(url)),
        None => {
            let documents = state.documents();
            (documents.len() == 1).then(|| documents[0].clone())
        }
    }
}

/// Reads a per-request document snapshot from a file URL. Blocking by
/// design, matching the crate's other disk reads; never panics on
/// external input — failures return `None` and conversion is skipped.
fn read_document_from_disk(url: &Url) -> Option<Document> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(Document::from_disk_text(url.clone(), text))
}
```

(`Document::from_disk_text` is the plan's name — use the SAME construction
`src/workspace/diagnostics.rs` uses when loading Workspace-origin documents
(read that file first; if it wraps a different constructor or carries
matcher/language work, extract just the text-to-Document part and record
the actual name as a deviation).)

- [ ] **Step 2: Dispatch tests** in `src/server/with_state/tests.rs` (drive through `LanguageServerWithState` like the existing `drive_link_resolve` pattern — read it first and follow its capture-server + `futures::executor::block_on` shape):
  1. `url_less_response_converts_against_sole_document` — one tracked document; a capture server whose `will_create_files` returns `Some(WorkspaceEdit)` with a UTF-8 range at byte 4 keying the tracked (emoji) URL; assert the dispatched response carries client column 2.
  2. `untracked_url_converts_against_disk` — `temp_workspace` file containing `🙂abc`, never opened; a SECOND unrelated document tracked (so the sole-doc heuristic cannot fire); request `hover` against the file's URL with the handler returning a range at byte 4; assert client column 2 in the response (params side: send client column 2, handler records byte 4).
  3. `url_less_passes_through_without_sole_document` — zero and two tracked documents: response returns unconverted (UTF-8 as-is).
- [ ] **Step 3: `document_changes` pin** — unit test in `src/requests/conversion.rs`'s test module (or sibling) covering `convert_workspace_edit`'s `DocumentChanges::Edits` branch: build a `WorkspaceEdit` with `document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit { text_document: OptionalVersionedTextDocumentIdentifier { uri: emoji, version: None }, edits: vec![OneOf::Left(TextEdit { range: same_line(0, 4, 4), .. })] }]))`, convert Outgoing, assert column 2.
- [ ] **Step 4: Full battery + dupes; report; owner commits.**

### Task 4: Folding / selection / linked editing / code lens

**Files:**
- Modify: `src/requests/registry.rs` (folding_range, linked_editing_range, code_lens rows), `src/requests/conversion.rs` (three helpers), `src/requests/custom_methods!` gets nothing — `selection_range` is a custom row
- Create: `src/requests/folding_range.rs`, `linked_editing_range.rs`, `code_lens.rs`, `selection_range.rs`

**Interfaces:**
- Consumes: registry grammar; `convert_optional_vec`.
- Produces: `modify_outgoing_folding_ranges(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::FoldingRange>>)`, `modify_outgoing_linked_editing_ranges(&ServerState, &Document, &mut Option<async_lsp::lsp_types::LinkedEditingRanges>)`, `modify_outgoing_code_lenses(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::CodeLens>>)`; custom impl `SelectionRange` with hooks converting `params.positions` (loop, `Direction::Incoming`) and the response's linked list (walk `parent`).

- [ ] **Step 1: Three helpers** (folding converts only the `Option<u32>` character columns — lines are encoding-independent; `None` characters default to line length client-side and stay `None`):

```rust
/// Converts each folding range's optional character columns from UTF-8 to
/// the client encoding; line numbers are encoding-independent.
pub(crate) fn modify_outgoing_folding_ranges(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspFoldingRange>>,
) {
    if let Some(ranges) = response {
        for range in ranges {
            convert_folding_range(state, document, range, Direction::Outgoing);
        }
    }
}

fn convert_folding_range(
    state: &ServerState,
    document: &Document,
    range: &mut LspFoldingRange,
    direction: Direction,
) {
    if let Some(character) = range.start_character.as_mut() {
        let mut position = LspPosition {
            line: range.start_line,
            character: *character,
        };
        convert_position(state, document, &mut position, direction);
        *character = position.character;
    }
    if let Some(character) = range.end_character.as_mut() {
        let mut position = LspPosition {
            line: range.end_line,
            character: *character,
        };
        convert_position(state, document, &mut position, direction);
        *character = position.character;
    }
}

/// Converts each range of a linkedEditingRange response from UTF-8 to the
/// client encoding.
pub(crate) fn modify_outgoing_linked_editing_ranges(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspLinkedEditingRanges>,
) {
    if let Some(ranges) = response {
        for range in &mut ranges.ranges {
            convert_range(state, document, range, Direction::Outgoing);
        }
    }
}

/// Converts each code lens's range from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_code_lenses(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspCodeLens>>,
) {
    if let Some(lenses) = response {
        for lens in lenses {
            convert_range(state, document, &mut lens.range, Direction::Outgoing);
        }
    }
}
```

(`FoldingRange as LspFoldingRange`, `LinkedEditingRanges as LspLinkedEditingRanges`, `CodeLens as LspCodeLens` join the alias import.)

- [ ] **Step 2: Three generated rows + one custom row**

```rust
folding_range: folding_range @ FoldingRange {
    doc: "Handles `textDocument/foldingRange` requests from the client.\n\nReturns the folding ranges of the document in `params`, or `None`. Requires a folding range provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::FoldingRangeParams,
    response: Option<Vec<async_lsp::lsp_types::FoldingRange>>,
    document: text_document,
    outgoing: modify_outgoing_folding_ranges,
}
linked_editing_range: linked_editing_range @ LinkedEditingRange {
    doc: "Handles `textDocument/linkedEditingRange` requests from the client.\n\nReturns the ranges that rename together with the symbol at the position in `params`, or `None`. Requires a linked editing range provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::LinkedEditingRangeParams,
    response: Option<async_lsp::lsp_types::LinkedEditingRanges>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_linked_editing_ranges,
}
code_lens: code_lens @ CodeLens {
    doc: "Handles `textDocument/codeLens` requests from the client.\n\nReturns the code lenses of the document in `params`, or `None`. Requires a code lens provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::CodeLensParams,
    response: Option<Vec<async_lsp::lsp_types::CodeLens>>,
    document: text_document,
    outgoing: modify_outgoing_code_lenses,
}
```

Custom row in `custom_methods!`:

```rust
selection_range: selection_range @ SelectionRange {
    doc: "Handles `textDocument/selectionRange` requests from the client.\n\nReturns the selection-range chains for the positions in `params`, or `None`; `positions[i]` must be contained in `result[i].range`. Requires a selection range provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::SelectionRangeParams,
    response: Option<Vec<async_lsp::lsp_types::SelectionRange>>,
}
```

`src/requests/selection_range.rs`:

```rust
use async_lsp::lsp_types::SelectionRangeParams as LspSelectionRangeParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range, convert_range_at_url},
};

pub struct SelectionRange;

impl Request for SelectionRange {
    type Params = LspSelectionRangeParams;
    type Response = Option<Vec<async_lsp::lsp_types::SelectionRange>>;

    request_extract_url!(text_document);

    fn modify_params(
        state: &ServerState,
        document: &Document,
        params: &mut Self::Params,
    ) {
        for position in &mut params.positions {
            super::conversion::convert_position(state, document, position, Direction::Incoming);
        }
    }

    fn modify_response(
        state: &ServerState,
        document: &Document,
        response: &mut Self::Response,
    ) {
        let Some(chains) = response else { return };
        for chain in chains {
            let mut current = Some(chain);
            while let Some(node) = current {
                convert_range(state, document, &mut node.range, Direction::Outgoing);
                current = node.parent.as_deref_mut();
            }
        }
    }
}
```

(`convert_range_at_url` import is unused — drop it from the `use` list.)

- [ ] **Step 3: Tests** — `conversion_tests!` rows for `linked_editing_range` (incoming position + outgoing `ranges[0].range.start`) and `code_lens` (outgoing-only: `CodeLens { range: same_line(0, 4, 4), command: None, data: None }`, getter `[0].range.start`); hand-written for `folding_range` (u32 columns) and `selection_range` (multi-position), both following the Plan 1 hand-written pattern against `state_with_documents`:

```rust
// folding_range.rs — hand-written: pin u32 character columns
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::FoldingRange;

    use crate::requests::Request;
    use crate::testing::state_with_documents;

    use super::FoldingRange as FoldingRangeRequest;

    #[test]
    fn folding_range_characters_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(vec![FoldingRange {
            start_line: 0,
            start_character: Some(4),
            end_line: 0,
            end_character: Some(5),
            kind: None,
            collapsed_text: None,
        }]);

        <FoldingRangeRequest as Request>::modify_response(&state, &document, &mut response);

        let range = response.expect("ranges present")[0].clone();
        assert_eq!(range.start_character, Some(2));
        assert_eq!(range.end_character, Some(3));
    }
}
```

(Import only `FoldingRange`, `Request`, `state_with_documents` — the outgoing-only test needs no params builder or range fixtures; drop the `same_line_unused` placeholder import shown above if it survives copy-paste review.)

`selection_range.rs` hand-written test: params with `positions: vec![line_position(0, 2), line_position(0, 3)]`, assert after `modify_params` both are byte columns 4 and 5; response chain `SelectionRange { range: same_line(0, 4, 5), parent: Some(Box::new(SelectionRange { range: same_line(0, 4, 4), parent: None })) }`, assert after `modify_response` the leaf is `same_line(0, 2, 3)` and the parent `same_line(0, 2, 2)`.

- [ ] **Step 4: Battery + checkpoint**; report; owner commits.

---

### Task 5: Colors — document_color, color_presentation

**Files:**
- Modify: `src/requests/registry.rs` (two rows), `src/requests/conversion.rs` (two helpers)
- Create: `src/requests/document_color.rs`, `src/requests/color_presentation.rs`

**Interfaces:**
- Produces: `modify_outgoing_color_informations(&ServerState, &Document, &mut Vec<async_lsp::lsp_types::ColorInformation>)` (bare Vec — not Option, per the verification report), `modify_outgoing_color_presentations(&ServerState, &Document, &mut Vec<async_lsp::lsp_types::ColorPresentation>)`.

- [ ] **Step 1: Helpers**

```rust
/// Converts each color information's range from UTF-8 to the client
/// encoding. The documentColor result is a bare vector.
pub(crate) fn modify_outgoing_color_informations(
    state: &ServerState,
    document: &Document,
    response: &mut Vec<LspColorInformation>,
) {
    for information in response {
        convert_range(state, document, &mut information.range, Direction::Outgoing);
    }
}

/// Converts each presentation's edits from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_color_presentations(
    state: &ServerState,
    document: &Document,
    response: &mut Vec<LspColorPresentation>,
) {
    for presentation in response {
        if let Some(edit) = presentation.text_edit.as_mut() {
            convert_text_edit(state, document, edit, Direction::Outgoing);
        }
        if let Some(edits) = presentation.additional_text_edits.as_mut() {
            for edit in edits {
                convert_text_edit(state, document, edit, Direction::Outgoing);
            }
        }
    }
}
```

- [ ] **Step 2: Rows** (note: the stamped `modify_response` signature takes `&mut Self::Response` where Response is the bare `Vec<...>` — helpers match exactly)

```rust
document_color: document_color @ DocumentColor {
    doc: "Handles `textDocument/documentColor` requests from the client.\n\nReturns all color references found in the document in `params`. Requires a color provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::DocumentColorParams,
    response: Vec<async_lsp::lsp_types::ColorInformation>,
    document: text_document,
    outgoing: modify_outgoing_color_informations,
}
color_presentation: color_presentation @ ColorPresentation {
    doc: "Handles `textDocument/colorPresentation` requests from the client.\n\nReturns the presentations for the color at the range in `params`. Sent as the resolve leg of a document color provider.",
    params: async_lsp::lsp_types::ColorPresentationParams,
    response: Vec<async_lsp::lsp_types::ColorPresentation>,
    document: text_document,
    incoming: range at range,
    outgoing: modify_outgoing_color_presentations,
}
```

- [ ] **Step 3: Tests** — hand-written W0 for both (getter shapes are not Position rows: documentColor pins a full range; color_presentation pins an incoming range + outgoing edit range). Follow the folding_range hand-written pattern; columns: incoming client range `same_line(0, 2, 3)` → UTF-8 `same_line(0, 4, 5)`; outgoing built `same_line(0, 4, 4)` → client `same_line(0, 2, 2)`. `Color { red: 1.0, green: 0.0, blue: 0.0, alpha: 1.0 }` for the color field; `ColorPresentation { label: "rgb(255, 0, 0)".into(), text_edit: Some(...), additional_text_edits: None }`.

- [ ] **Step 4: Battery + checkpoint**; report; owner commits.

---

### Task 6: Hierarchies + moniker — prepare/incoming/outgoing calls, prepare/super/sub types, moniker

**Files:**
- Modify: `src/requests/registry.rs` (three generated rows: prepare_call_hierarchy, prepare_type_hierarchy, moniker; four custom rows: incoming_calls, outgoing_calls, supertypes, subtypes), `src/requests/conversion.rs` (two item helpers + incoming item helper)
- Create: `src/requests/prepare_call_hierarchy.rs`, `prepare_type_hierarchy.rs`, `moniker.rs`, `incoming_calls.rs`, `outgoing_calls.rs`, `supertypes.rs`, `subtypes.rs`

**Interfaces:**
- Produces: `modify_outgoing_call_hierarchy_items(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::CallHierarchyItem>>)`, `modify_outgoing_type_hierarchy_items(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>)`; item converters `convert_call_hierarchy_item(&ServerState, &Document, &mut async_lsp::lsp_types::CallHierarchyItem, Direction)` and `convert_type_hierarchy_item(...)` converting `range` + `selection_range` per-URL (against `item.uri`, falling back to the request document).

- [ ] **Step 1: Helpers** — both item converters follow this shape:

```rust
/// Converts a call hierarchy item's two ranges between encodings, against
/// the item's own document when tracked, falling back to `document`.
pub(crate) fn convert_call_hierarchy_item(
    state: &ServerState,
    document: &Document,
    item: &mut LspCallHierarchyItem,
    direction: Direction,
) {
    let uri = item.uri.clone();
    convert_range_at_url(state, document, &uri, &mut item.range, direction);
    convert_range_at_url(state, document, &uri, &mut item.selection_range, direction);
}
```

(`convert_type_hierarchy_item` identical over `LspTypeHierarchyItem`; the two `modify_outgoing_*_items` loop `convert_optional_vec`-style over `Option<Vec<_>>` with the respective converter; `CallHierarchyIncomingCall`/`OutgoingCall` converters convert `from`/`to` item plus every range in `from_ranges`, both directions — write `convert_call_hierarchy_incoming_call`/`convert_call_hierarchy_outgoing_call` direction-parameterized and thin outgoing `modify_*` wrappers.)

- [ ] **Step 2: Rows** — generated:

```rust
prepare_call_hierarchy: prepare_call_hierarchy @ CallHierarchyPrepare {
    doc: "Handles `textDocument/prepareCallHierarchy` requests from the client.\n\nReturns the call hierarchy items for the symbol at the position in `params`, or `None`. Requires a call hierarchy provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::CallHierarchyPrepareParams,
    response: Option<Vec<async_lsp::lsp_types::CallHierarchyItem>>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_call_hierarchy_items,
}
prepare_type_hierarchy: prepare_type_hierarchy @ TypeHierarchyPrepare {
    doc: "Handles `textDocument/prepareTypeHierarchy` requests from the client.\n\nReturns the type hierarchy items for the symbol at the position in `params`, or `None`. Requires a type hierarchy provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::TypeHierarchyPrepareParams,
    response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_type_hierarchy_items,
}
moniker: moniker @ Moniker {
    doc: "Handles `textDocument/moniker` requests from the client.\n\nReturns the symbol monikers at the position in `params`, or `None`. Requires a moniker provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::MonikerParams,
    response: Option<Vec<async_lsp::lsp_types::Moniker>>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
}
```

Custom rows (docs in house style; params carry `item`):

```rust
incoming_calls: incoming_calls @ IncomingCalls {
    doc: "Handles `callHierarchy/incomingCalls` requests from the client.\n\nReturns the callers of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.",
    params: async_lsp::lsp_types::CallHierarchyIncomingCallsParams,
    response: Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
}
outgoing_calls: outgoing_calls @ OutgoingCalls {
    doc: "Handles `callHierarchy/outgoingCalls` requests from the client.\n\nReturns the callees of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. Conversion as per `incoming_calls`.",
    params: async_lsp::lsp_types::CallHierarchyOutgoingCallsParams,
    response: Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>,
}
supertypes: supertypes @ Supertypes {
    doc: "Handles `typeHierarchy/supertypes` requests from the client.\n\nReturns the supertypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.",
    params: async_lsp::lsp_types::TypeHierarchySupertypesParams,
    response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
}
subtypes: subtypes @ Subtypes {
    doc: "Handles `typeHierarchy/subtypes` requests from the client.\n\nReturns the subtypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. Conversion as per `supertypes`.",
    params: async_lsp::lsp_types::TypeHierarchySubtypesParams,
    response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
}
```

`incoming_calls.rs` (the other three mirror it with their params/response types):

```rust
use async_lsp::lsp_types::CallHierarchyIncomingCallsParams as LspCallHierarchyIncomingCallsParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_call_hierarchy_incoming_call},
};

pub struct IncomingCalls;

impl Request for IncomingCalls {
    type Params = LspCallHierarchyIncomingCallsParams;
    type Response = Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>;

    request_extract_url!(item.uri);

    fn modify_params(
        state: &ServerState,
        document: &Document,
        params: &mut Self::Params,
    ) {
        convert_call_hierarchy_item(state, document, &mut params.item, Direction::Incoming);
    }

    fn modify_response(
        state: &ServerState,
        document: &Document,
        response: &mut Self::Response,
    ) {
        if let Some(calls) = response {
            for call in calls {
                convert_call_hierarchy_incoming_call(state, document, call, Direction::Outgoing);
            }
        }
    }
}
```

(The `use super::conversion::{...}` line imports both `convert_call_hierarchy_item` and `convert_call_hierarchy_incoming_call`; `extract_url!(item.uri)` gives staleness against the item's own document. `outgoing_calls.rs` mirrors with `convert_call_hierarchy_outgoing_call` over `to`; `supertypes.rs`/`subtypes.rs` mirror with `convert_type_hierarchy_item` on both sides over `params.item`.)

- [ ] **Step 3: Tests** — `conversion_tests!` incoming+outgoing rows for `prepare_call_hierarchy`/`prepare_type_hierarchy` (response items built with `uri: emoji`, `range: same_line(0, 4, 4)`, `selection_range: same_line(0, 4, 4)`, `name: "f".into()`, `kind: SymbolKind::FUNCTION`, `tags: None`, `detail: None`, `data: None`; getter `[0].range.start`); incoming-only row for `moniker`; hand-written for the four item-carriers: params `item` built at client columns, assert UTF-8 after `modify_params`, and response round-trip after `modify_response` (follow the Plan 1 hand-written pattern).

- [ ] **Step 4: Battery + checkpoint**; report; owner commits.

---

### Task 7: Inlay hint / inline value / signature help

**Files:**
- Modify: `src/requests/registry.rs` (two generated rows: inlay_hint, signature_help; one custom row: inline_value), `src/requests/conversion.rs` (three helpers + label-offset converter)
- Create: `src/requests/inlay_hint.rs`, `inline_value.rs`, `signature_help.rs`

**Interfaces:**
- Produces: `modify_outgoing_inlay_hints(&ServerState, &Document, &mut Option<Vec<async_lsp::lsp_types::InlayHint>>)` (converts `position`, every `text_edits` range, and every label-part `location`); `modify_outgoing_inline_value(&ServerState, &Document, &mut Option<async_lsp::lsp_types::InlineValue>)` (match on the three variants, convert each `range`); `modify_outgoing_signature_help(&ServerState, &Document, &mut Option<async_lsp::lsp_types::SignatureHelp>>)` (converts `ParameterLabel::LabelOffsets` against the containing label string); `convert_label_offsets(label: &str, offsets: &mut [u32; 2], from: Encoding, to: Encoding)`.

- [ ] **Step 1: Label-offset converter** (offsets count code units of the label string; conversion between unit systems of one string is exact — the spec's Verification-notes resolution):

```rust
/// Recounts `[start, end]` code-unit offsets of `label` from one encoding
/// to another. Signature-help parameter labels are offsets into their
/// containing signature label string, so conversion needs only the string
/// itself, never the document.
fn convert_label_offsets(label: &str, offsets: &mut [u32; 2], from: Encoding, to: Encoding) {
    if from == to {
        return;
    }
    for offset in &mut offsets {
        let mut seen_from: u32 = 0;
        let mut converted: u32 = 0;
        for ch in label.chars() {
            if seen_from >= *offset {
                break;
            }
            let units_from = match from {
                Encoding::UTF8 => ch.len_utf8() as u32,
                Encoding::UTF16 => ch.len_utf16() as u32,
                Encoding::UTF32 => 1,
            };
            let units_to = match to {
                Encoding::UTF8 => ch.len_utf8() as u32,
                Encoding::UTF16 => ch.len_utf16() as u32,
                Encoding::UTF32 => 1,
            };
            seen_from += units_from;
            converted += units_to;
        }
        *offset = converted;
    }
}
```

`modify_outgoing_signature_help` walks `signatures[].parameters[].label` and, on `ParameterLabel::LabelOffsets([s, e])`, applies `convert_label_offsets(signature.label_str, &mut [s, e], Encoding::UTF8, state.get_position_encoding())` then writes the variant back.

- [ ] **Step 2: Rows + inline_value custom file**

```rust
inlay_hint: inlay_hint @ InlayHint {
    doc: "Handles `textDocument/inlayHint` requests from the client.\n\nReturns inlay hints for the range in `params`, or `None`. Requires an inlay hint provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::InlayHintParams,
    response: Option<Vec<async_lsp::lsp_types::InlayHint>>,
    document: text_document,
    incoming: range at range,
    outgoing: modify_outgoing_inlay_hints,
}
signature_help: signature_help @ SignatureHelp {
    doc: "Handles `textDocument/signatureHelp` requests from the client.\n\nReturns signature help at the position in `params`, or `None`. Requires a signature help provider in [`Server::server_capabilities`]. Parameter label offsets are recounted between UTF-8 and the negotiated encoding against the label string itself.",
    params: async_lsp::lsp_types::SignatureHelpParams,
    response: Option<async_lsp::lsp_types::SignatureHelp>,
    document: text_document_position_params.text_document,
    incoming: position at text_document_position_params.position,
    outgoing: modify_outgoing_signature_help,
}
inline_value: inline_value @ InlineValue {
    doc: "Handles `textDocument/inlineValue` requests from the client.\n\nReturns a single inline value computed for the range in `params`, or `None`. Requires an inline value provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::InlineValueParams,
    response: Option<async_lsp::lsp_types::InlineValue>,
}
```

`inline_value.rs` (custom: two incoming ranges — `range` and `context.stopped_location`):

```rust
use async_lsp::lsp_types::InlineValueParams as LspInlineValueParams;

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct InlineValue;

impl Request for InlineValue {
    type Params = LspInlineValueParams;
    type Response = Option<async_lsp::lsp_types::InlineValue>;

    request_extract_url!(text_document);

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_range(state, document, &mut params.range, Direction::Incoming);
        convert_range(
            state,
            document,
            &mut params.context.stopped_location,
            Direction::Incoming,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        let Some(value) = response else { return };
        let range = match value {
            async_lsp::lsp_types::InlineValue::Text(v) => &mut v.range,
            async_lsp::lsp_types::InlineValue::VariableLookup(v) => &mut v.range,
            async_lsp::lsp_types::InlineValue::EvaluatableExpression(v) => &mut v.range,
        };
        convert_range(state, document, range, Direction::Outgoing);
    }
}
```

- [ ] **Step 3: Tests** — `conversion_tests!` row for `inlay_hint` (incoming range + outgoing `position`: build `InlayHint { position: line_position(0, 4), label: InlayHintLabel::String("x".into()), .. }` — spell all remaining fields `None`/defaults explicitly since `InlayHint` may not derive `Default`; getter `r.as_ref().expect("hint present").position`, returns 2); hand-written for `signature_help` (build `SignatureHelp { signatures: vec![SignatureInformation { label: "🙂f(a)".into(), parameters: Some(vec![ParameterInformation { label: ParameterLabel::LabelOffsets([4, 5]), documentation: None }]), .. }], .. }` — assert offsets become `[2, 3]` after `modify_response`) and for `inline_value` (both incoming ranges + one variant's outgoing range).

- [ ] **Step 4: Battery + checkpoint**; report; owner commits.

---

### Task 8: Symbols + execute_command — document_symbol, workspace symbol, execute_command

**Files:**
- Modify: `src/requests/registry.rs` (two generated rows: document_symbol, execute_command; one custom row: symbol), `src/requests/conversion.rs` (two helpers + disk-fallback machinery)
- Create: `src/requests/document_symbol.rs`, `symbol.rs`, `execute_command.rs`

**Interfaces:**
- Produces: `modify_outgoing_document_symbols(&ServerState, &Document, &mut Option<async_lsp::lsp_types::DocumentSymbolResponse>>)` (Flat: convert each `SymbolInformation.location` per-URL; Nested: recurse `children`, convert `range` + `selection_range`); custom `Symbol` impl whose `modify_response` converts `WorkspaceSymbolResponse` with store-first / disk-fallback / pass-through and a per-request cache (`HashMap<Url, Option<Document>>`).

- [ ] **Step 1: Helpers**

```rust
/// Converts a nested document-symbol tree's ranges from UTF-8 to the
/// client encoding.
fn convert_document_symbol(
    state: &ServerState,
    document: &Document,
    symbol: &mut LspDocumentSymbol,
) {
    convert_range(state, document, &mut symbol.range, Direction::Outgoing);
    convert_range(state, document, &mut symbol.selection_range, Direction::Outgoing);
    if let Some(children) = symbol.children.as_mut() {
        for child in children {
            convert_document_symbol(state, document, child);
        }
    }
}

pub(crate) fn modify_outgoing_document_symbols(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspDocumentSymbolResponse>,
) {
    let Some(response) = response else { return };
    match response {
        LspDocumentSymbolResponse::Flat(symbols) => {
            for symbol in symbols {
                convert_location(state, document, &mut symbol.location, Direction::Outgoing);
            }
        }
        LspDocumentSymbolResponse::Nested(symbols) => {
            for symbol in symbols {
                convert_document_symbol(state, document, symbol);
            }
        }
    }
}
```

- [ ] **Step 2: Rows + the custom symbol file**

```rust
document_symbol: document_symbol @ DocumentSymbol {
    doc: "Handles `textDocument/documentSymbol` requests from the client.\n\nReturns the symbol tree of the document in `params` (nested when the client supports it, flat otherwise), or `None`. Requires a document symbol provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::DocumentSymbolParams,
    response: Option<async_lsp::lsp_types::DocumentSymbolResponse>,
    document: text_document,
    outgoing: modify_outgoing_document_symbols,
}
execute_command: execute_command @ ExecuteCommand {
    doc: "Handles `workspace/executeCommand` requests from the client.\n\nExecutes the command in `params` and returns an opaque result. Requires an execute command provider in [`Server::server_capabilities`].",
    params: async_lsp::lsp_types::ExecuteCommandParams,
    response: Option<async_lsp::lsp_types::LSPAny>,
}
symbol: symbol @ Symbol {
    doc: "Handles `workspace/symbol` requests from the client.\n\nReturns the workspace-wide symbols matching the query, or `None`. Requires a workspace symbol provider in [`Server::server_capabilities`]. Symbol locations convert against their own document when tracked; untracked files are read from disk once per request (cached); unreadable locations pass through unchanged.",
    params: async_lsp::lsp_types::WorkspaceSymbolParams,
    response: Option<async_lsp::lsp_types::WorkspaceSymbolResponse>,
}
```

`src/requests/symbol.rs` — custom impl: `extract_url` default (None, no staleness); `modify_params` no-op (query only); `modify_response` walks the response — `Flat(Vec<SymbolInformation>)`: convert each `location` via store-first lookup (`state.document(&loc.uri)`), else disk-fallback reading the file bytes to a `Document` (cache `HashMap<Url, Option<Document>>` per call; parse failures cache `None` and pass through), else pass-through; `Nested(Vec<WorkspaceSymbol>)`: same for `location: OneOf::Left(location)`, `Right(_)` has no range and passes through untouched. Build the fallback `Document` via the same construction `workspace/diagnostics.rs` uses for loading workspace files (reuse its loader if one exists — check `src/workspace/diagnostics.rs` first; if it has a private loader, extract or mirror it minimally, recording the choice).

- [ ] **Step 3: Tests** — `conversion_tests!` outgoing row for `document_symbol` (Nested: `DocumentSymbolResponse::Nested(vec![DocumentSymbol { name: "f".into(), detail: None, kind: SymbolKind::FUNCTION, tags: None, deprecated: None, range: same_line(0, 4, 4), selection_range: same_line(0, 4, 4), children: Some(vec![...leaf...]) }])`, getter `[0].range.start` returns 2; a second hand-written assertion pins the recursive child); hand-written for `symbol` (three branches: tracked doc converts; untracked-but-on-disk converts via a `temp_workspace` file; nonexistent URI passes through) and a no-op sanity test for `execute_command` (params unchanged through `modify_params`, response value unchanged through `modify_response`).

- [ ] **Step 4: Battery + checkpoint**; report; owner commits.

---

### Task 9: Final sweep for Plan 2

**Files:**
- Modify: `src/lib.rs` or module docs if any new public surface needs re-export notes (none expected — trait surface grows via the registry, re-exports unchanged)
- Verify `.dupes-ignore.toml` state

- [ ] **Step 1: Cross-check the spec's Class A/A2 tables against the registry** — every one of the 27 methods from Tasks 2–8 appears in exactly one table (`generated_methods!` or `custom_methods!`), rows total 16 retrofitted + 27 new = 43 across the three tables. Any mismatch: fix before proceeding.
- [ ] **Step 2: Full battery** (`cargo build --all-targets && cargo test && cargo test --no-default-features && cargo test --all-features && cargo fmt --check && cargo clippy --all-targets -- -D warnings && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps && cargo dupes check`).
- [ ] **Step 3: Review checkpoint**; report (files: whatever the sweep touched, possibly none); owner commits.
