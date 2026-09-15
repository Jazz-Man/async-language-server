# Workspace Diagnostics Capability Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the implementor's capabilities block authoritative for workspace diagnostics and warn once when an advertised method has no implementation.

**Architecture:** Three moving parts: (1) `configure_capabilities` stops overwriting the implementor's advertised `workspace_diagnostics` (one `Disabled` kill-switch arm remains) and `supported` derives from the final advertisement; (2) a `MethodInventory` in `ServerState` records which `Server` methods the final `InitializeResult` advertised, via a 42-row predicate table; (3) the dispatch engines route default-body errors through `warn_once_default`, so an advertised-but-unimplemented method logs one warning.

**Tech Stack:** Rust (edition 2024), async-lsp 0.2.4 `lsp_types` 0.95.1, `lsp_macros` (proc-macro crate in `macros/`), tracing, tokio (dev-deps for tests).

**Spec:** `docs/superpowers/specs/2026-09-15-workspace-diagnostics-capability-design.md`

## Global Constraints

- The owner commits: each task ends at its verification step; the controller shows the file group and waits. No task runs `git add`/`git commit`.
- No new dependencies; `lsp-types` 0.95.1 shapes only.
- Lint gates are deny: `missing_docs` (rust), clippy `all`/`cargo`/`pedantic`; `expect_used`/`unwrap_used` denied in `src/` (test modules exempt via `clippy.toml`).
- All written artifacts in English; every public item carries `///` docs; fallible public fns carry `# Errors`.
- The battery is the done-bar per task: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets -- -D warnings`, `rtk cargo nextest run --workspace --all-features`; task 5 additionally runs `make battery` (both feature legs + doctests) and `make dupes`.
- Breaking change (task 2): the commit message must say servers relying on the old default force-enable must now advertise `workspace_diagnostics: true` themselves.
- `type_hierarchy_provider` does NOT exist in lsp-types 0.95.1 — the three type-hierarchy trait methods get a `false` predicate with a comment (never advertised ⇒ never warns).

---

### Task 1: `ServerError::MethodNotImplemented` — a distinguishable default

The dispatch guard must tell "the implementor's default ran" apart from "the implementor deliberately returned `METHOD_NOT_FOUND`". The trait defaults go through one helper (`server_trait.rs:703-708`), so give their error its own variant. Wire behavior is unchanged: same code, same message text.

**Files:**
- Modify: `src/error.rs` (variant in `ServerError`, arm in `From<ServerError> for ResponseError`, test)
- Modify: `src/server/server_trait.rs:703-708` (`method_not_implemented` body)

**Interfaces:**
- Consumes: nothing new.
- Produces: `ServerError::MethodNotImplemented { method: &'static str }` (public, additive on the `#[non_exhaustive]` enum); wire mapping → `METHOD_NOT_FOUND` with Display text `LSP method '{method}' has not been implemented`. Task 4's macro hook matches this variant via `state.warn_once_default`.

- [ ] **Step 1: Write the failing tests** (in `src/error.rs`, inside the existing `mod tests`)

```rust
#[test]
fn method_not_implemented_maps_to_method_not_found() {
    let error = ServerError::MethodNotImplemented { method: "hover" };
    assert_eq!(error.to_string(), "LSP method 'hover' has not been implemented");

    let response = ResponseError::from(error);
    assert_eq!(response.code, ErrorCode::METHOD_NOT_FOUND);
    assert_eq!(
        response.message,
        "LSP method 'hover' has not been implemented",
    );
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `rtk cargo nextest run --workspace --all-features -E 'test(method_not_implemented_maps)'`
Expected: FAIL — `no variant or associated item named 'MethodNotImplemented'`

- [ ] **Step 3: Add the variant** (in `src/error.rs`, inside `ServerError`, after the `Rpc` variant)

```rust
    /// A dispatched LSP method whose `Server` trait default ran: the wrapper
    /// dispatches the method, but the implementor did not override it.
    ///
    /// Kept distinct from [`ServerError::Rpc`] so the dispatch layer can tell
    /// a trait default apart from an implementor's deliberate
    /// `METHOD_NOT_FOUND`; both map to the same wire code.
    #[error("LSP method '{method}' has not been implemented")]
    MethodNotImplemented {
        /// The `Server` trait method's name.
        method: &'static str,
    },
```

- [ ] **Step 4: Map it at the boundary** (in `src/error.rs`, in `From<ServerError> for ResponseError`, as the arm *before* the catch-all)

```rust
            ServerError::MethodNotImplemented { method } => ResponseError::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("LSP method '{method}' has not been implemented"),
            ),
```

- [ ] **Step 5: Route the trait default through the variant** (replace the body of `method_not_implemented`, `src/server/server_trait.rs:703-708`)

```rust
fn method_not_implemented<T>(name: &'static str) -> std::future::Ready<Result<T, ServerError>> {
    std::future::ready(Err(ServerError::MethodNotImplemented { method: name }))
}
```

- [ ] **Step 6: Run the suite** — `rtk cargo nextest run --workspace --all-features`
Expected: PASS (the wire `dispatch.rs` tests keep passing: same code, same message text).

- [ ] **Step 7: Gates** — `rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Pause for the owner's commit** — files: `src/error.rs`, `src/server/server_trait.rs`.

---

### Task 2: The resolution matrix

Replace the capability overwrite pair with the matrix: `Disabled` forces the advertisement off (kill-switch), anything else leaves the implementor's value verbatim, `supported` becomes the final advertised value.

**Files:**
- Modify: `src/workspace/diagnostics.rs:96-178` (`WorkspaceDiagnosticsState::configure`, `configure_capabilities`, `enable_workspace_diagnostics`, `disable_workspace_diagnostics`)
- Modify: `src/workspace/diagnostics.rs:180-197` (`enable_workspace_folder_tracking` gains a `supported` gate at its caller)
- Test: `src/workspace/diagnostics.rs` (`mod tests` — new matrix test; rework any test asserting force-enable)

**Interfaces:**
- Consumes: `WorkspaceDiagnosticsState::new`'s existing fields; `InitializeResult`/`DiagnosticServerCapabilities` from `lsp_types`.
- Produces: `configure_capabilities` with matrix semantics; `supported()` reflects the provider's advertised flag (task 5's `workspace/diagnostic` gating relies on it). No new public API.

- [ ] **Step 1: Write the failing matrix test** (in `diagnostics.rs` `mod tests`; the file already imports `ServerOptions`, `ServerState`, `WorkspaceDiagnostics`, `configure_capabilities`, `ClientSocket`, `DiagnosticServerCapabilities`, `DiagnosticOptions`, `ServerCapabilities`, `InitializeResult`, `ClientCapabilities`)

```rust
    fn matrix_state(
        options: WorkspaceDiagnostics,
    ) -> ServerState {
        #[derive(Default)]
        struct MatrixServer;
        impl Server for MatrixServer {}

        let options = ServerOptions::default().with_workspace_diagnostics(options);
        ServerState::with_options::<MatrixServer>(ClientSocket::new_closed(), &options)
    }

    fn result_with_provider(
        workspace_diagnostics: bool,
    ) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        workspace_diagnostics,
                        ..DiagnosticOptions::default()
                    },
                )),
                ..ServerCapabilities::default()
            },
            ..InitializeResult::default()
        }
    }

    // The spec's resolution matrix, one row per case: (ServerOptions mode,
    // advertised flag in the implementor's provider) => expected final
    // advertisement and handler support.
    #[test]
    fn resolution_matrix_advertises_verbatim_and_gates_support() {
        let cases: [
            (WorkspaceDiagnostics, bool, bool, bool);
            4
        ] = [
            // (mode, implementor's flag) => (advertised, supported)
            (WorkspaceDiagnostics::enabled(), true, true, true),
            (WorkspaceDiagnostics::enabled(), false, false, false),
            (WorkspaceDiagnostics::disabled(), true, false, false),
            (WorkspaceDiagnostics::disabled(), false, false, false),
        ];
        for (i, (mode, flag, advertised, supported)) in cases.into_iter().enumerate() {
            let state = matrix_state(mode);
            let client = ClientCapabilities::default();
            let mut result = result_with_provider(flag);
            configure_capabilities(&state, &mut result, &client);

            let provider = result
                .capabilities
                .diagnostic_provider
                .as_ref()
                .expect("provider survives the merge");
            let DiagnosticServerCapabilities::Options(options) = provider else {
                panic!("case {i}: unexpected provider shape");
            };
            assert_eq!(options.workspace_diagnostics, advertised, "case {i}: advertised");
            assert_eq!(state.workspace_diagnostics().supported(), supported, "case {i}: supported");
        }
    }

    // A provider-less implementor stays provider-less: the framework creates
    // nothing, and the handler stays unsupported.
    #[test]
    fn provider_none_advertises_nothing_and_stays_unsupported() {
        for mode in [WorkspaceDiagnostics::enabled(), WorkspaceDiagnostics::disabled()] {
            let state = matrix_state(mode);
            let mut result = InitializeResult::default();
            configure_capabilities(&state, &mut result, &ClientCapabilities::default());

            assert!(result.capabilities.diagnostic_provider.is_none());
            assert!(!state.workspace_diagnostics().supported());
        }
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `rtk cargo nextest run --workspace --all-features -E 'test(resolution_matrix) + test(provider_none_advertises)'`
Expected: FAIL — `resolution_matrix`: the `Enabled`/`false` row observes advertised `true` (today's force-enable).

- [ ] **Step 3: Implement the matrix** (replacing `configure_capabilities`, `enable_workspace_diagnostics`, `disable_workspace_diagnostics` at `diagnostics.rs:137-178`, and reworking `configure` at `:96-122`)

```rust
impl WorkspaceDiagnosticsState {
    fn configure(
        &self,
        result: &InitializeResult,
        client_capabilities: &ClientCapabilities,
        supported: bool,
    ) {
        self.inner.supported.store(supported, Ordering::Relaxed);

        let workspace = client_capabilities.workspace.as_ref();
        self.inner.client_configuration.store(
            workspace.and_then(|w| w.configuration).unwrap_or(false),
            Ordering::Relaxed,
        );
        self.inner.client_dynamic_configuration.store(
            workspace
                .and_then(|config| config.did_change_configuration.as_ref())
                .and_then(|did_change_configuration| did_change_configuration.dynamic_registration)
                .unwrap_or(false),
            Ordering::Relaxed,
        );
        self.inner.client_refresh.store(
            workspace
                .and_then(|diag| diag.diagnostic.as_ref())
                .and_then(|diagnostic| diagnostic.refresh_support)
                .unwrap_or(false),
            Ordering::Relaxed,
        );
    }
}

pub(crate) fn configure_capabilities(
    state: &ServerState,
    result: &mut InitializeResult,
    client_capabilities: &ClientCapabilities,
) {
    let workspace_diagnostics = state.workspace_diagnostics();

    // The one deliberate override: the kill-switch forces the advertisement
    // off no matter what the implementor declared. Everything else is the
    // implementor's value, verbatim.
    if matches!(
        &workspace_diagnostics.inner.options,
        WorkspaceDiagnostics::Disabled
    ) {
        set_workspace_diagnostics_advertised(result, false);
    }

    let supported = advertised_workspace_diagnostics(result);
    workspace_diagnostics.configure(result, client_capabilities, supported);

    if supported {
        enable_workspace_folder_tracking(result);
    }
}

/// Reads the provider's advertised `workspace_diagnostics` flag; no provider
/// advertises nothing.
fn advertised_workspace_diagnostics(result: &InitializeResult) -> bool {
    result
        .capabilities
        .diagnostic_provider
        .as_ref()
        .is_some_and(|provider| match provider {
            DiagnosticServerCapabilities::Options(options) => options.workspace_diagnostics,
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                options.diagnostic_options.workspace_diagnostics
            }
        })
}

fn set_workspace_diagnostics_advertised(result: &mut InitializeResult, advertised: bool) {
    if let Some(provider) = result.capabilities.diagnostic_provider.as_mut() {
        match provider {
            DiagnosticServerCapabilities::Options(options) => {
                options.workspace_diagnostics = advertised;
            }
            DiagnosticServerCapabilities::RegistrationOptions(options) => {
                options.diagnostic_options.workspace_diagnostics = advertised;
            }
        }
    }
}
```

Delete `enable_workspace_diagnostics` entirely. In `enable_workspace_folder_tracking` (`:180-197`) the `if result.capabilities.diagnostic_provider.is_none() { return; }` early-return stays as-is — the `supported` gate at the caller replaces the old unconditional call.

Also update `WorkspaceDiagnosticsState::new`'s `supported` initialization (`:48-51`): it starts `false` unconditionally (`AtomicBool::new(false)`) — before `initialize` nothing is advertised.

- [ ] **Step 4: Rework tests pinning the old force-ON behavior** — `disabled_options_force_workspace_diagnostics_capability_off` keeps its meaning under the kill-switch (verify it still passes; adjust only if it asserted force-enable of a `false` provider under `Enabled`, which no other test does — run the whole `workspace::diagnostics` module to find out).

Run: `rtk cargo nextest run --workspace --all-features -E 'test(workspace::diagnostics)'`
Expected: PASS (new matrix tests green; at most mechanical fixes to tests that asserted the force-enable).

- [ ] **Step 5: Full suite + gates** — `rtk cargo nextest run --workspace --all-features && rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, clean.

- [ ] **Step 6: Pause for the owner's commit** — files: `src/workspace/diagnostics.rs`. Commit message must carry the breaking-change note: *"Breaking: servers must advertise `workspace_diagnostics: true` themselves; the framework no longer force-enables it (only `WorkspaceDiagnostics::disabled()` overrides, as a kill-switch)."*

---

### Task 3: The `MethodInventory` — advertised-method predicates + warn-once

New `src/server/inventory.rs` (declared in `src/server/mod.rs` alongside the other modules — check the actual `mod` list there; it is NOT publicly exported): the 42 non-resolve dispatch rows, each with a capability predicate over the final `ServerCapabilities`; `warn_once_default` answers whether a warning fired.

**Files:**
- Create: `src/server/inventory.rs`
- Modify: `src/server/mod.rs` (module declaration — follow the existing private-module pattern), `src/server/state/mod.rs` (field + accessors)
- Test: inline `mod tests` in `inventory.rs`; completeness test added to `src/server/tests/dispatch.rs`

**Interfaces:**
- Consumes: `ServerError::MethodNotImplemented` (task 1); `ServerCapabilities` from `lsp_types`.
- Produces:
  - `pub(crate) struct MethodInventory` — cheap-`Clone` handle (`Arc` inner), `Debug`.
  - `pub(crate) const METHOD_NAMES: &[&str]` — the 42 trait-method names in dispatch-table order.
  - `MethodInventory::new() -> Self` — nothing advertised, nothing warned.
  - `MethodInventory::from_capabilities(caps: &ServerCapabilities) -> Self`.
  - `fn advertised(&self, method: &str) -> bool`.
  - `fn warn_once_default(&self, method: &'static str, error: &ServerError) -> bool` — `true` iff this call emitted the warning (advertised ∧ default-error ∧ first hit).
  - `ServerState::set_advertised_methods(&mut self, caps: &ServerCapabilities)` and `ServerState::warn_once_default(&self, method: &'static str, error: &ServerError) -> bool` (state/mod.rs, next to `set_position_encoding`).

- [ ] **Step 1: Create `src/server/inventory.rs`** with the table and mechanics:

```rust
use crate::error::ServerError;
use async_lsp::lsp_types::{
    CodeActionProviderCapability, ColorProviderCapability, DiagnosticServerCapabilities,
    OneOf, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensRegistrationOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The `lsp_dispatch!` table's non-resolve trait methods, in table order.
///
/// The resolve family is absent on purpose: its defaults resolve the item
/// unchanged and never produce `method_not_implemented`. The two type
/// hierarchy *calls* (`supertypes`, `subtypes`) and `prepare_type_hierarchy`
/// are present but can never be advertised — lsp-types 0.95.1 carries no
/// type-hierarchy capability field — so their predicates are `false`.
pub(crate) const METHOD_NAMES: &[&str] = &[
    "hover",
    "declaration",
    "definition",
    "references",
    "link",
    "rename",
    "rename_prepare",
    "document_format",
    "document_range_format",
    "implementation",
    "type_definition",
    "document_highlight",
    "on_type_formatting",
    "folding_range",
    "linked_editing_range",
    "code_lens",
    "will_save_wait_until",
    "document_color",
    "color_presentation",
    "prepare_call_hierarchy",
    "prepare_type_hierarchy",
    "moniker",
    "will_create_files",
    "will_rename_files",
    "will_delete_files",
    "inlay_hint",
    "document_symbol",
    "execute_command",
    "semantic_tokens_full",
    "semantic_tokens_range",
    "semantic_tokens_full_delta",
    "completion",
    "code_action",
    "document_diagnostics",
    "selection_range",
    "inline_value",
    "incoming_calls",
    "outgoing_calls",
    "supertypes",
    "subtypes",
    "symbol",
    "signature_help",
];

/// Which `Server` methods the final `InitializeResult` advertised, and
/// which of them have already drawn their single default-warning.
#[derive(Debug, Clone)]
pub(crate) struct MethodInventory {
    inner: Arc<InventoryInner>,
}

#[derive(Debug)]
struct InventoryInner {
    advertised: Box<[bool]>,
    warned: Box<[AtomicBool]>,
}

impl Default for MethodInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl MethodInventory {
    /// An inventory advertising nothing; the state before `initialize`.
    pub(crate) fn new() -> Self {
        let empty = vec![false; METHOD_NAMES.len()].into_boxed_slice();
        let unwarned = METHOD_NAMES.iter().map(|_| AtomicBool::new(false)).collect();
        Self {
            inner: Arc::new(InventoryInner {
                advertised: empty,
                warned: unwarned,
            }),
        }
    }

    /// Derives the advertised set from the capabilities the wrapper is about
    /// to send: a predicate per method, mirroring the capability its `///`
    /// doc names.
    pub(crate) fn from_capabilities(caps: &ServerCapabilities) -> Self {
        let advertised = METHOD_NAMES
            .iter()
            .map(|name| advertised(name, caps))
            .collect();
        let warned = METHOD_NAMES.iter().map(|_| AtomicBool::new(false)).collect();
        Self {
            inner: Arc::new(InventoryInner {
                advertised,
                warned,
            }),
        }
    }

    fn advertised(&self, method: &str) -> bool {
        index_of(method)
            .map(|index| self.inner.advertised[index])
            .unwrap_or(false)
    }

    /// Warns once per method when a trait default ran for an advertised
    /// method. Returns whether this call emitted the warning.
    pub(crate) fn warn_once_default(
        &self,
        method: &'static str,
        error: &ServerError,
    ) -> bool {
        if !matches!(error, ServerError::MethodNotImplemented { .. }) {
            return false;
        }
        let Some(index) = index_of(method) else {
            return false;
        };
        if !self.inner.advertised[index] {
            return false;
        }
        if self.inner.warned[index].swap(true, Ordering::Relaxed) {
            return false;
        }
        tracing::warn!(
            "LSP method '{method}' is advertised in the server capabilities but not \
             implemented; remove the capability or override the method"
        );
        true
    }
}

fn index_of(method: &str) -> Option<usize> {
    METHOD_NAMES.iter().position(|name| *name == method)
}

/// Presence semantics for `Option<OneOf<bool, Options>>` capabilities:
/// `Left(false)` is an explicit no, everything else present advertises.
fn advertised_bool<T>(provider: &Option<OneOf<bool, T>>) -> bool {
    provider
        .as_ref()
        .is_some_and(|one_of| matches!(one_of, OneOf::Left(true) | OneOf::Right(_)))
}

fn semantic_tokens_options(
    caps: &ServerCapabilities,
) -> Option<&SemanticTokensOptions> {
    match caps.semantic_tokens_provider.as_ref()? {
        SemanticTokensServerCapabilities::SemanticTokensOptions(options) => Some(options),
        SemanticTokensServerCapabilities::RegistrationOptions(registration) => {
            Some(&registration.semantic_tokens_options)
        }
    }
}

fn advertised(name: &str, caps: &ServerCapabilities) -> bool {
    match name {
        "hover" => caps.hover_provider.is_some(),
        "declaration" => caps.declaration_provider.is_some(),
        "definition" => caps.definition_provider.is_some(),
        "references" => caps.references_provider.is_some(),
        "link" => caps.document_link_provider.is_some(),
        "rename" => advertised_bool(&caps.rename_provider),
        "rename_prepare" => caps
            .rename_provider
            .as_ref()
            .is_some_and(|provider| {
                matches!(
                    provider,
                    OneOf::Right(options) if options.prepare_provider == Some(true),
                )
            }),
        "document_format" => advertised_bool(&caps.document_formatting_provider),
        "document_range_format" => advertised_bool(&caps.document_range_formatting_provider),
        "implementation" => caps.implementation_provider.is_some(),
        "type_definition" => caps.type_definition_provider.is_some(),
        "document_highlight" => advertised_bool(&caps.document_highlight_provider),
        "on_type_formatting" => caps.document_on_type_formatting_provider.is_some(),
        "folding_range" => caps.folding_range_provider.is_some(),
        "linked_editing_range" => caps.linked_editing_range_provider.is_some(),
        "code_lens" => caps.code_lens_provider.is_some(),
        "will_save_wait_until" => matches!(
            caps.text_document_sync.as_ref(),
            Some(TextDocumentSyncCapability::Options(options))
                if options.will_save_wait_until == Some(true),
        ),
        // `color_presentation` is the resolve leg of the color provider: the
        // same capability advertises both.
        "document_color" | "color_presentation" => caps
            .color_provider
            .as_ref()
            .is_some_and(|provider| matches!(provider, ColorProviderCapability::Simple(true) | ColorProviderCapability::Options(_))),
        "prepare_call_hierarchy" | "incoming_calls" | "outgoing_calls" => {
            caps.call_hierarchy_provider.is_some()
        }
        // lsp-types 0.95.1 carries no type-hierarchy capability field: these
        // can never be advertised, so they never warn.
        "prepare_type_hierarchy" | "supertypes" | "subtypes" => false,
        "moniker" => advertised_bool(&caps.moniker_provider),
        "will_create_files" => caps
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.file_operations.will_create.is_some()),
        "will_rename_files" => caps
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.file_operations.will_rename.is_some()),
        "will_delete_files" => caps
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.file_operations.will_delete.is_some()),
        "inlay_hint" => caps.inlay_hint_provider.is_some(),
        "document_symbol" => advertised_bool(&caps.document_symbol_provider),
        "execute_command" => caps.execute_command_provider.is_some(),
        "semantic_tokens_full" => semantic_tokens_options(caps).is_some_and(|options| options.full.is_some()),
        "semantic_tokens_range" => semantic_tokens_options(caps).is_some_and(|options| options.range.is_some()),
        "semantic_tokens_full_delta" => semantic_tokens_options(caps).is_some_and(|options| {
            matches!(
                options.full.as_ref(),
                Some(OneOf::Right(full)) if full.delta == Some(true),
            )
        }),
        "completion" => caps.completion_provider.is_some(),
        "code_action" => caps.code_action_provider.is_some(),
        "document_diagnostics" => caps.diagnostic_provider.is_some(),
        "selection_range" => caps.selection_range_provider.is_some(),
        "inline_value" => advertised_bool(&caps.inline_value_provider),
        "symbol" => advertised_bool(&caps.workspace_symbol_provider),
        "signature_help" => caps.signature_help_provider.is_some(),
        other => unreachable!("'{other}' is not a dispatch-table method; METHOD_NAMES and this match must stay aligned"),
    }
}
```

If `advertised` compiles without the `unreachable!` arm being reachable — good; clippy may want the match to be exhaustive over the `&str`: the `other` arm is the exhaustiveness escape and is deliberately unreachable for anything in `METHOD_NAMES`. (The `advertised_bool`/OneOf imports: drop unused ones if the compiler flags them — e.g. `CodeActionProviderCapability` is only needed if `code_action` resolves presence through it; plain `is_some()` covers `Simple(false)` poorly, so mirror the bool rule: `"code_action" => match caps.code_action_provider.as_ref() { Some(CodeActionProviderCapability::Simple(false)) => false, Some(_) => true, None => false }`.)

- [ ] **Step 2: W0 tests, inline in `inventory.rs`**

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DiagnosticOptions, DiagnosticServerCapabilities, HoverProviderCapability,
        OneOf, RenameOptions, ServerCapabilities,
    };

    use super::{METHOD_NAMES, MethodInventory};
    use crate::error::ServerError;

    fn error(method: &'static str) -> ServerError {
        ServerError::MethodNotImplemented { method }
    }

    fn server_error() -> ServerError {
        ServerError::rpc(async_lsp::ErrorCode::METHOD_NOT_FOUND, "deliberate".into())
    }

    #[test]
    fn method_names_match_the_dispatch_table_order() {
        assert_eq!(METHOD_NAMES.len(), 42);
        assert_eq!(METHOD_NAMES[0], "hover");
        assert_eq!(METHOD_NAMES.last().copied(), Some("signature_help"));
        // The resolve family never warns and never appears.
        assert!(!METHOD_NAMES.contains(&"completion_resolve"));
        // A default error for a method outside the table is a no-op.
        let inventory = MethodInventory::new();
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }

    #[test]
    fn unadvertised_methods_never_warn() {
        let inventory = MethodInventory::from_capabilities(&ServerCapabilities::default());
        assert!(!inventory.advertised("hover"));
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }

    #[test]
    fn advertised_defaults_warn_exactly_once() {
        let caps = ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                ..RenameOptions::default()
            })),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DiagnosticOptions::default(),
            )),
            ..ServerCapabilities::default()
        };
        let inventory = MethodInventory::from_capabilities(&caps);

        // `rename_prepare` is advertised: rename_provider is OneOf::Right
        // with prepare_provider on — exactly the prepare predicate's shape.
        assert!(inventory.advertised("rename_prepare"));
        // `document_diagnostics` is advertised by provider presence; its
        // `workspace_diagnostics` flag does not affect the method-level
        // advertisement.
        assert!(inventory.advertised("document_diagnostics"));

        assert!(inventory.warn_once_default("hover", &error("hover")));
        assert!(!inventory.warn_once_default("hover", &error("hover")));
        // A deliberate METHOD_NOT_FOUND from an overridden method is not a
        // default hit and draws no warning.
        assert!(!inventory.warn_once_default("rename", &server_error()));
        // Inventory clones share the warned state: no second warning.
        let clone = inventory.clone();
        assert!(!clone.warn_once_default("hover", &error("hover")));
    }
}
```

- [ ] **Step 3: Wire into `ServerState`** (`src/server/state/mod.rs`): add `use crate::server::MethodInventory;`-style import consistent with the crate's re-exports (check `src/server/mod.rs` for how siblings are exported; `MethodInventory` itself stays `pub(crate)`), a field `advertised_methods: MethodInventory` on `ServerState` (it is `Clone + Debug`-compatible: `MethodInventory` derives both), initialization `MethodInventory::new()` in `with_options`, and two accessors next to `set_position_encoding`:

```rust
    /// Records which `Server` methods the capabilities sent to the client
    /// advertise. Called once per `initialize`, after the final
    /// `InitializeResult` is composed.
    pub(crate) fn set_advertised_methods(&mut self, caps: &ServerCapabilities) {
        self.advertised_methods = MethodInventory::from_capabilities(caps);
    }

    /// Warns once per method when an advertised method's trait default ran;
    /// returns whether this call warned. See [`MethodInventory`].
    pub(crate) fn warn_once_default(
        &self,
        method: &'static str,
        error: &ServerError,
    ) -> bool {
        self.advertised_methods.warn_once_default(method, error)
    }
```

(`use crate::error::ServerError;` and `use async_lsp::lsp_types::ServerCapabilities;` in state/mod.rs as needed.)

- [ ] **Step 4: Completeness test** (append to `src/server/tests/dispatch.rs`) — first restate `wired_requests()` as triples `(wire_name, trait_method, params)` adding the trait name from the `lsp_dispatch!` table (wire → trait mapping: `documentLink → link`, `prepare_rename → rename_prepare`, `formatting → document_format`, `rangeFormatting → document_range_format`, `onTypeFormatting → on_type_formatting`, `willSaveWaitUntil → will_save_wait_until`, `documentColor → document_color`, `prepareCallHierarchy → prepare_call_hierarchy`, `prepareTypeHierarchy → prepare_type_hierarchy`, `willCreateFiles → will_create_files`, `willRenameFiles → will_rename_files`, `willDeleteFiles → will_delete_files`, `documentSymbol → document_symbol`, `executeCommand → execute_command`, `semanticTokens/full → semantic_tokens_full`, `semanticTokens/range → semantic_tokens_range`, `semanticTokens/full/delta → semantic_tokens_full_delta`, `documentDiagnostic → document_diagnostics`, `incomingCalls → incoming_calls`, `outgoingCalls → outgoing_calls`, `supertypes → supertypes`, `subtypes → subtypes`, `signatureHelp → signature_help`, all others identical). Then:

```rust
#[test]
fn inventory_covers_exactly_the_non_resolve_dispatch_rows() {
    use crate::server::inventory::METHOD_NAMES;

    let resolve = [
        "completion_resolve",
        "code_action_resolve",
        "link_resolve",
        "code_lens_resolve",
        "inlay_hint_resolve",
        "workspace_symbol_resolve",
    ];
    for (_, trait_method, _) in wired_requests() {
        let present = METHOD_NAMES.contains(&trait_method);
        assert_eq!(
            present,
            !resolve.contains(&trait_method),
            "{trait_method}: exactly the non-resolve rows are inventoried",
        );
    }
}
```

(`wired_requests` keeps its wire test usage; the iterator item becomes a 3-tuple — update the two destructure sites in `wired_methods_dispatch`.)

- [ ] **Step 5: Suite + gates** — `rtk cargo nextest run --workspace --all-features && rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, clean. (Compile errors around optional-vs-plain `file_operations` fields or `SemanticTokensRegistrationOptions` shapes: consult `lsp-types-0.95.1/src/{lib,semantic_tokens}.rs` and adapt the accessor chain — the predicate intent is fixed by the spec.)

- [ ] **Step 6: Pause for the owner's commit** — files: `src/server/inventory.rs` (new), `src/server/mod.rs`, `src/server/state/mod.rs`, `src/server/tests/dispatch.rs`.

---

### Task 4: The dispatch guard hook

Both dispatch engines call `server.#trait_method(...).await?`. Intercept the error before `?`: default errors on advertised methods warn once, then the error proceeds unchanged.

**Files:**
- Modify: `macros/src/dispatch.rs:137` (resolve core) and `:167` (URL-anchored core) — the `let mut result = server.#trait_method(state.clone(), params).await?;` line in each
- Test: `macros/src/dispatch.rs` `mod tests` (needle assertions on the emitted code)

**Interfaces:**
- Consumes: `ServerState::warn_once_default(&self, method: &'static str, error: &ServerError) -> bool` (task 3); `stringify!(#trait_method)` available at expansion site.
- Produces: dispatch behavior — errors now pass through `state.warn_once_default(...)` before conversion. Wire responses byte-identical.

- [ ] **Step 1: Update the macro tests first** — in `engine_emits_url_anchored_skeleton`, add `"warn_once_default"` to the needle list; in `engine_emits_sole_document_path_for_resolve_rows`, likewise assert `text.contains("warn_once_default")`.

- [ ] **Step 2: Run them to see them fail** — `rtk cargo nextest run -p lsp_macros`
Expected: FAIL — needles missing.

- [ ] **Step 3: Replace the call line in both cores** (`macros/src/dispatch.rs`) with:

```rust
            let mut result = match server.#trait_method(state.clone(), params).await {
                Ok(result) => result,
                Err(error) => {
                    state.warn_once_default(stringify!(#trait_method), &error);
                    return Err(error.into());
                }
            };
```

(`state` is the cloned `ServerState` already captured by `wrapped()`; `ServerError: Into<ResponseError>` via the existing `From` impl, so `error.into()` preserves today's `?` conversion.)

- [ ] **Step 4: Run macro tests + full suite** — `rtk cargo nextest run --workspace --all-features`
Expected: PASS — including the wire `dispatch.rs` tests (message text unchanged; `EchoServer` implements `hover`, and unadvertised defaults never warn).

- [ ] **Step 5: Gates** — `rtk cargo fmt --check && rtk cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Pause for the owner's commit** — files: `macros/src/dispatch.rs`.

---

### Task 5: Initialize wiring, docs, and the full battery

**Files:**
- Modify: `src/server/with_state/initialize.rs` (one call after the final capabilities exist)
- Modify: `src/server/options.rs` (`with_workspace_diagnostics` doc + doctest), `src/server/options.rs` `WorkspaceDiagnostics` variant docs (kill-switch and verbatim rule)
- Test: battery (both legs + doctests), `make dupes`

**Interfaces:**
- Consumes: `ServerState::set_advertised_methods` (task 3).
- Produces: end-to-end behavior — the inventory reflects exactly what was sent.

- [ ] **Step 1: Wire the inventory** — in `initialize.rs`, after the `text_document_sync` insertion block (after line 71, before the encoding step at line 74), insert:

```rust
        // 4a. Record which methods the final capabilities advertise: the
        //    dispatch guard warns only for advertised defaults, so it must
        //    see the result exactly as the client receives it.
        self.state.set_advertised_methods(&result.capabilities);
```

(The call must stay **after** step 4: `will_save_wait_until`'s predicate reads `text_document_sync`, which step 4 inserts.)

- [ ] **Step 2: Docs** — update `src/server/options.rs`:
  - `WorkspaceDiagnostics::Disabled` doc: "Do not handle workspace diagnostics. This is the kill-switch: it forces the advertised `workspace_diagnostics` capability off regardless of the implementor's declaration."
  - `WorkspaceDiagnostics::Enabled` doc: "Handle workspace diagnostics; the implementor's advertised `workspace_diagnostics` value is left verbatim."
  - `WorkspaceDiagnostics::Configurable` doc: "Leave the implementor's advertised `workspace_diagnostics` value untouched and toggle handling using a setting."
  - `with_workspace_diagnostics` doc: one sentence stating the implementor's capabilities block is authoritative except for the `Disabled` kill-switch.
  - `Server` trait doc (`src/server/server_trait.rs:14-24`): add a paragraph — "The wrapper warns once per method when a method advertised in `server_capabilities` reaches its default implementation; implement what you advertise, or drop the capability."

- [ ] **Step 3: Full battery** — `rtk make battery && rtk make dupes`
Expected: all green (both feature legs, doctests, dylint, dupes 0.0%). Fix findings at their root; no suppressions.

- [ ] **Step 4: Pause for the owner's commit** — files: `src/server/with_state/initialize.rs`, `src/server/options.rs`, `src/server/server_trait.rs`.

---

## Self-Review

- **Spec coverage:** resolution matrix → Task 2; `supported` from final advertisement → Task 2 (Step 3); framework never creates a provider → Task 2 (provider-less test); inventory + predicates → Task 3; warn-once guard → Tasks 3+4; wire behavior unchanged → Tasks 1+4 (message text preserved); W0 tier throughout → Tasks 1-4; docs/doctests → Task 5; breaking-change note → Task 2 Step 6. No gaps.
- **Placeholder scan:** none — every step carries code or an exact command. Compile-shape caveats are called out with their resolution path (Task 3 Step 5), not left open.
- **Type consistency:** `MethodInventory::{new, from_capabilities, advertised, warn_once_default}` and `ServerState::{set_advertised_methods, warn_once_default}` are named identically across Tasks 3, 4, and 5; `ServerError::MethodNotImplemented { method: &'static str }` matches task 1's definition and task 4's `stringify!` payload.
