# Workspace Diagnostics Capability — Design

Date: 2026-09-15. Branch: `feature/workspace-diagnostics-fix`.
Research basis: `docs/superpowers/research/2026-09-15-lsp-poc-performance.md`
§2.2 and §4 (row 5). Status: approved design, pending implementation plan.

## Problem

Two defects in how the wrapper manages workspace diagnostics:

1. **The dead flag.** `configure_capabilities`
   (`src/workspace/diagnostics.rs:137-152`) force-overwrites the implementor's
   advertised `workspace_diagnostics` value in both directions:
   `ServerOptions::Enabled` flips an explicit `false` to `true`, and
   `ServerOptions::Disabled` flips an explicit `true` to `false`
   (`enable_workspace_diagnostics` / `disable_workspace_diagnostics`,
   `diagnostics.rs:154-178`). The implementor's capabilities block — its only
   voice in the protocol — is overridden, and `lsp-poc`'s explicit
   `workspace_diagnostics: false` was silently ignored.
2. **Handler gating ignores the advertisement.**
   `WorkspaceDiagnosticsState::configure` (`diagnostics.rs:96-101`) derives
   `supported` from `ServerOptions != Disabled` plus mere *presence* of a
   `diagnostic_provider` — it never reads the provider's advertised
   `workspace_diagnostics` value. A provider advertising `false` runs the
   handler; the wire behavior can diverge from the advertisement.

Generalized principle behind both: nothing verifies that an advertised
capability is backed by machinery or an implementation, and the framework
fills in what the implementor did not say.

## Goals

1. The implementor's capabilities block is authoritative: the advertised
   `workspace_diagnostics` value is whatever the implementor declared,
   verbatim.
2. `ServerOptions::Disabled` remains as a documented kill-switch that wins
   over the implementor.
3. The `workspace/diagnostic` handler is gated by the final advertisement:
   the wire behavior always matches what the client was told.
4. A runtime consistency guard warns once per method when a method the
   server advertised reaches its default (unimplemented) trait method.
5. The framework never creates or fills capabilities the implementor did not
   declare.

## Non-goals

- **Compile-time warnings for advertised-but-unimplemented methods.**
  Considered and rejected: Rust offers no way to detect whether a downstream
  impl block overrides a defaulted trait method — a proc macro sees only the
  impl's tokens, not the semantics of `server_capabilities()`, and a
  mandatory attribute macro generating capabilities from annotated methods
  would break the design center (implement `Server`, override a few methods,
  no required attributes). Recorded here so the question stays answered.
- Walk caching / `spawn_blocking` for the refresh walk; the derived-data
  cache hook (separate cycles, tracked in the research report §4).
- Consumer changes in `lsp-poc`'s repository (§ Breaking changes).
- Changing the resolve family's identity defaults (they never produce
  `method_not_implemented` and never warn).

## Semantics — the resolution matrix

Evaluated in `configure_capabilities`, replacing the current
enable/disable overwrite pair:

| `ServerOptions::workspace_diagnostics` | implementor's `diagnostic_provider` | advertised `workspace_diagnostics` | handler `supported` |
|---|---|---|---|
| `Disabled` | any, or `None` | `false` (kill-switch; the one deliberate override) | `false` |
| `Enabled` or `Configurable(_)` | `Some(provider)` | the provider's value, verbatim | the advertised value |
| `Enabled` or `Configurable(_)` | `None` | no provider is created; nothing advertised | `false` |

Rules:

- Both `DiagnosticServerCapabilities` arms (`Options`, `RegistrationOptions`)
  are treated symmetrically.
- Framework machinery — workspace-folder tracking on the diagnostic provider,
  dynamic registration, configuration polling, refresh support, the runtime
  setting — activates only when `supported` is `true`.
- `WorkspaceDiagnosticsState::new` no longer derives `supported` from
  `ServerOptions`; it starts `false` (the handler refuses work before
  `initialize`, consistent with lifecycle gating) and `configure_capabilities`
  sets it from the final advertisement.

## The advertised-method inventory and the dispatch guard

- After the wrapper computes the final `InitializeResult`, it derives the
  **advertised set**: one row per `Server` method in a static
  method → capability-predicate table, each predicate mirroring the
  capability named in the method's `///` docs (`lsp_method!` /
  `lsp_resolve_method!`), evaluated against the sent result. Lifecycle
  methods (`initialize`, `shutdown`) carry no capability and never enter the
  inventory, and neither does the wrapper-served `workspace/diagnostic` — it
  has no trait default to hit and is gated by the resolution matrix's
  `supported` instead.
- **Dispatch guard**: when a dispatched call returns the
  `method_not_implemented` error for a method present in the advertised set,
  the wrapper logs `tracing::warn!` **once per method per process** —
  "advertised in capabilities but not implemented: remove the capability or
  override the method". The wire response is unchanged (`METHOD_NOT_FOUND` —
  spec-legal); the warning gives the implementor the cause.
- Methods absent from the advertised set never warn — implementing more than
  advertised is legitimate (clients just never ask).

## Error handling

No new error paths. The guard is log-only; responses stay
`METHOD_NOT_FOUND`. The matrix removes the conflict cases instead of
handling them.

## Testing (W0 tier)

- Merge matrix, inline in `diagnostics.rs` tests: provider ∈
  {`None`, `Options(workspace_diagnostics: false)`, `Options(true)`,
  `RegistrationOptions(true)`} × options ∈ {`Disabled`, `Enabled`,
  `Configurable`} → assert the advertised value and `supported`.
- `supported` is `false` before `configure_capabilities` runs, and stays
  `false` for a `None` provider.
- Inventory completeness: the predicate table covers exactly the dispatch
  table's method set (a W0 test next to the existing dispatch fixture).
- Guard: a stub server advertising a method it does not override triggers
  the warning exactly once across repeated dispatches; a non-advertised
  override never warns; a resolve-family method never warns.
- Existing tests pinning the force-enable behavior are reworked to the
  matrix (e.g. `disabled_options_force_workspace_diagnostics_capability_off`
  keeps its meaning under the kill-switch rule).
- Doctests: `with_workspace_diagnostics` and the `WorkspaceDiagnostics`
  variant docs document the kill-switch and the verbatim rule; examples stay
  `--no-default-features`-clean.

## Breaking changes

- Servers that relied on the default force-enable must now advertise
  `workspace_diagnostics: true` in their capabilities block. Breaking-change
  note goes in the commit message (git-dependency distribution; no semver
  net — `product.md`).
- `lsp-poc` follow-up, in its own repository, not this spec's scope: flip
  `workspace_diagnostics: false` → `true` in `server_capabilities`
  (`crates/lsp-poc/src/server.rs:277`).
