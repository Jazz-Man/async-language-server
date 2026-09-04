# Project structure research — workspace split, visibility, and the `oneshot` question

**Date:** 2026-09-05
**Scope:** read-only research for the cycle that (a) migrates the 15 `macro_rules!` into `lsp_macros` (DSL design is out of scope here) and (b) restructures the repository into a multi-crate workspace with tighter visibility.
**Tree examined:** main checkout, branch `feature/macros` @ `d389adf`, plus the uncommitted workspace wiring (`[workspace] members = ["macros", "."]` in the root `Cargo.toml`, untracked `macros/`). Note: this research worktree sits 31 commits behind `feature/macros`; all file inventories and line counts below were taken from the main checkout's working tree, which is the current truth.

---

## 1. Module inventory

93 `.rs` files under `src/` (14,183 lines total, tests included), plus `tests/architecture.rs` (64), `examples/` (186), `macros/` (107 incl. manifest). 50 of the 93 files live in `src/requests/`.

### 1.1 Files and roles

| file | lines | role |
|---|---:|---|
| `src/lib.rs` | 24 | Crate root: README as crate docs (`#![doc = include_str!]`), `lsp_types` + `tree_sitter` re-exports, private `documents`/`error`/`requests`/`workspace`, public `oneshot`/`text_utils`/`tree_sitter_utils`(gated)/`server`, `#[cfg(test)] pub(crate) testing`. |
| `src/error.rs` | 192 | The typed error domain: `ServerError` + `ServerResult`, `RangeError`, `QueryError`, `ServerErrorCode` re-export of `async_lsp::ErrorCode`. Scopeless (not in any arch-lint scope); no production deps — only doc links outward. |
| `src/testing.rs` | 229 | `#[cfg(test)] pub(crate)` shared W0 fixtures (`line_position`, `url`, `state_with_documents`, `temp_workspace`, …) + the `conversion_tests!` table macro. |
| `src/tree_sitter_utils.rs` | 221 | Feature-gated tree-sitter↔LSP coordinate helpers, node-search combinators, re-exports `QueryError`. |
| `src/text_utils/mod.rs` (+`conversions` 155, `encoding` 149, `position` 127, `range_ext/*` 98+193+288 prod) | ~1,180 prod | The encoding machinery: `Encoding`, `position_to_encoding`, `Position`, `RangeExt` over byte/LSP/tree-sitter ranges. The arch-lint bottom leaf. |
| `src/documents/document.rs` | 361 | `Document` snapshot (Rope + optional Language/Tree), `DocumentReader`, `DocumentQueryCapture`; `Document::query` (gated). Fields are `pub(crate)` and constructed by struct literal in `server/state` and `server/with_state`. |
| `src/documents/matcher.rs` | 256 | `DocumentMatcher` (URL globs + language-ids + optional grammar), `DocumentMatchers` (`pub(crate)`). |
| `src/workspace/walker.rs` | 144 | `WorkspaceWalker`/`WorkspaceWalkConfig` (`pub(crate)`), `path_to_url`. `ignore`-based scanning. |
| `src/workspace/diagnostics.rs` | 565 | `workspace/diagnostic` implementation: capability registration, configuration plumbing, walking + per-document diagnostics merge. All `pub(crate)`. |
| `src/oneshot/mod.rs` + `server.rs` (120) + `workspace_diagnostics.rs` (447) | 581 | Clientless batch entry point (deep-dive in §3). |
| `src/server/mod.rs` | 39 | The facade: re-exports `Server`, `ServerState`, `ServerOptions` trio, `serve`, `Document`/`DocumentReader`/`DocumentMatcher`/`DocumentQueryCapture`, error quartet; `pub(crate)` seam: `LanguageServerWithState`, `read_document_from_disk`, `CachedSemanticTokens`. |
| `src/server/server_trait.rs` | 237 | The `Server` trait: 4 config methods + 48 request methods stamped from the registry + 12 notification hooks. |
| `src/server/serve.rs` | 142 | `serve()` over process stdio behind the tower stack; `pub(crate) run_over_streams` + `TokioReader`/`TokioWriter` (the wire-test seam). |
| `src/server/options.rs` | 208 | `ServerOptions`, `WorkspaceDiagnostics`, `WorkspaceDiagnosticsSetting`, `ConfigurationKey`. |
| `src/server/state/{mod,documents,workspace}.rs` | 153+484+154 | `ServerState` + document store (open/change/close/save, disk recovery), workspace roots/encoding/semantic-tokens cache. |
| `src/server/with_state/{mod,initialize}.rs` | 414+120 | `LanguageServerWithState` (`pub(crate)`): async-lsp `LanguageServer` impl — dispatch engine (staleness `CONTENT_MODIFIED`, conversion-document resolution) and initialize flow. |
| `src/server/testing.rs` | 237 | Wire-tier scaffolding: `spawn_wire_server`, `RawClient`, `EchoServer`, `bounded`. |
| `src/server/tests/*` (6 files) | 654 | Wire tests: conversion, dispatch, lifecycle, robustness, staleness, termination. |
| `src/server/state/tests.rs`, `src/server/with_state/tests.rs` | 472, 1555 | W0 tests for state and dispatch. |
| `src/requests/mod.rs` | 206 | `Request` trait (`pub`), 3 helper `macro_rules!` (`request_extract_url!`, `request_modify_params_position!`, `request_modify_params_range!`), `registry_request_impls!` stamper (generates 31 unit structs + impls), re-exports of the hand-written request structs. |
| `src/requests/registry.rs` | 355 | The single source of truth: three declarative tables — `generated_methods!` (31 rows), `custom_methods!` (11), `resolve_methods!` (6) — stamped by consumers in `server_trait.rs`, `with_state/mod.rs`, `requests/mod.rs`. |
| `src/requests/conversion.rs` | 1191 | The `Direction` + `convert_*` / `modify_outgoing_*` family (43 fns, all `pub(crate)`). |
| `src/requests/<48 per-method files>` | 24–203 each | One `pub struct X;` + `impl Request` per LSP request (hand-written hooks for the 17 custom/resolve shapes; 31 unit structs are generated). Most carry an inline W0 `conversion_tests!` block. |
| `tests/architecture.rs` | 64 | arch-lint wiring: layer rules from `arch-lint.toml` + behavioral rules (NoSyncIo, NoErrorSwallowing, …). |
| `examples/minimal.rs`, `examples/tree_sitter.rs` | 88, 98 | The two documented paths; import only `lsp_types` + `server::{Server, serve, …}`. |
| `macros/` (untracked) | 92 + 15 | `lsp_macros` proc-macro crate: syn 3.0.4 + quote 1.0.47, placeholder `#[lsp_request]` attribute with copy-paste-invalid code (see §6/§7). |

### 1.2 The 15 `macro_rules!`

| macro | home | consumed by |
|---|---|---|
| `generated_methods!` / `custom_methods!` / `resolve_methods!` | `requests/registry.rs` | data tables, expanded via `table!(stamper)` passthrough in three scopes |
| `registry_request_impls!` | `requests/mod.rs` | stamps the 31 generated `Request` impls from `generated_methods!` |
| `request_extract_url!` / `request_modify_params_position!` / `request_modify_params_range!` | `requests/mod.rs` | per-method `impl Request` blocks |
| `registry_trait_methods!` / `registry_trait_resolve_methods!` | `server/server_trait.rs` | stamps 48+6 trait methods from the same tables |
| `implement_method!` / `implement_methods!` / `implement_resolve_method!` / `registry_dispatch!` / `registry_dispatch_resolve!` | `server/with_state/mod.rs` | stamps the dispatch engine entries from the same tables |
| `conversion_tests!` | `src/testing.rs` | W0 request-conversion tests |

Eight of the fifteen are *stamper* macros over the registry tables; the registry tables themselves are declarative data. Any proc-macro migration that keeps the tables as data only needs to replace the stamper layer (or attach attributes to per-method files) — but note `generated_methods!`/`custom_methods!`/`resolve_methods!` expand inside three different modules and today rely on textual passthrough (`table!(stamper)`), which a proc macro cannot do the same way; this is a design input, not a decision (see §6.4).

### 1.3 Inter-module dependency arrows (production code, from `use` statements)

```
error ........................... (nothing; doc links only)
text_utils ...................... error (RangeError)
tree_sitter_utils [gated] ....... text_utils, error
documents ....................... server::DocumentMatcher (facade path to its own
                                  sibling), tree_sitter_utils [gated]
requests ........................ server (ServerState, Document), text_utils
workspace ....................... requests (Request hook on DocumentDiagnostics —
                                  the one blessed edge), server (ServerState,
                                  ServerOptions, WorkspaceDiagnostics…), error
server .......................... documents, requests, workspace, text_utils, error
oneshot ......................... server (LanguageServerWithState, Server),
                                  workspace (walker, path_to_url), documents
                                  (DocumentMatchers), error
```

Test-only edges (all through `#[cfg(test)]`): nearly every module → `crate::testing`; `server/testing.rs` → `serve::{run_over_streams, TokioReader, TokioWriter}`.

`arch-lint.toml` already machine-checks this shape: five scopes (`text-utils`, `documents`, `workspace`, `requests`, `server`, `oneshot`) with `deny-scope-dep` rules — `text_utils` is the leaf, the three domains are siblings below the wiring, `server` must not depend on `oneshot`, and `workspace → requests` is the single blessed sibling edge. `tests/architecture.rs` enforces it.

---

## 2. Public-surface audit

### 2.1 What is actually public

The real API is everything reachable from `src/lib.rs`:

- `async_language_server::lsp_types` (re-export), `::tree_sitter` (gated re-export);
- `oneshot::{workspace_diagnostics, WorkspaceDiagnosticConfig, WorkspaceDiagnosticReport, DocumentDiagnostics}`;
- `text_utils::{position_to_encoding, Encoding, Position, RangeExt, RangeError}`;
- `tree_sitter_utils::{QueryError, ts_point_to_lsp_position, ts_range_to_lsp_range, ts_range_contains_lsp_position, ts_range_contains_ts_point, lsp_position_to_ts_point, find_child, find_ancestor, find_descendant, find_nearest}` (gated);
- `server::{Server, ServerState, ServerOptions, WorkspaceDiagnostics, WorkspaceDiagnosticsSetting, ConfigurationKey, serve, Document, DocumentReader, DocumentMatcher, DocumentQueryCapture (gated), RangeError, ServerError, ServerErrorCode, ServerResult}`.

That is **~36 nameable items** — the surface product.md prescribes, and it matches what the examples consume (`lsp_types` + `server::*` only).

### 2.2 Where visibility is loose

- **`src/requests/` is the one real offender: 49 `pub` items in a private module.** `pub trait Request` + 17 per-file `pub struct` request types + 31 `pub struct`s generated by `registry_request_impls!`. Because `mod requests;` is private, none of these is externally nameable — every one of the 49 is a mechanical `pub` → `pub(crate)` candidate. The conversions family next door (43 fns) is already `pub(crate)`, so the inconsistency is visible in-file.
- Everything else already observes the discipline: `workspace/` re-exports are all `pub(crate)`; `serve.rs` internals (`run_over_streams`, `TokioReader/Writer`) are `pub(crate)`; `LanguageServerWithState` is `pub(crate)`; `ServerState`'s internals are `pub(crate)` with exactly three public methods (`client`, `document`, `documents`); `DocumentMatchers` is `pub(crate)`.
- Items re-exported through the `server` facade (`Document`, `DocumentMatcher`, the error types) are `pub` at definition *by necessity*: a `pub use` through `pub mod server` requires `pub` at the source (E0364 otherwise). They are not `pub(crate)` candidates without breaking the facade. The facade re-export breadth (16 lines in `server/mod.rs`) *is* the product surface, not leakage — though it is worth a deliberate look during the cycle whether e.g. `ConfigurationKey`'s constructors need to be that public.
- One latent quirk: `QueryError` is nameable only via `tree_sitter_utils` (gated). That is consistent today because `Document::query` is also gated — no gap, but the coupling is implicit and worth a comment when things move.

### 2.3 Bottom line

[Inference] The "too many `pub` items" concern resolves to a single, bounded change: ~49 `pub` → `pub(crate)` conversions confined to `src/requests/`, plus one forward-looking decision on `pub trait Request` (see §6.4 — its visibility is coupled to the macro ABI). The rest of the tree already matches the intended surface. An LSP findReferences pass over the 49 confirms they are referenced only from `crate::requests::*`, `crate::server::with_state`, and `crate::workspace` (the `DocumentDiagnostics` hook), never through any public path.

---

## 3. `oneshot` deep-dive

### 3.1 What it does

`oneshot` runs a `Server` over files on disk with **no LSP client and no transport**. Its own module doc:

> "Run a `Server` over workspace files on disk, without an LSP client or transport. `workspace_diagnostics()` drives the same stateful wrapper as the live server path: it initializes a workspace, opens each matched document, and requests diagnostics once — useful for CLI-style batch runs."

Mechanics (`oneshot/server.rs`): `OneshotServer` wraps `LanguageServerWithState::new(ClientSocket::new_closed(), server)` — the closed socket makes every client-bound send a no-op — then drives the async-lsp `LanguageServer` trait methods *directly*: `initialize` (UTF-8 encoding advertised, roots as workspace folders) → `initialized` → `did_open` per matched document → `document_diagnostic` per document. Because it goes through `LanguageServerWithState`, the full production path executes: matching, document tracking, the dispatch engine, staleness detection, and UTF-8 conversion. The one boundary conversion it owns is `ResponseError → ServerError::rpc`, exactly the "clientless entry point" edge `error-handling.md` prescribes. It reuses `WorkspaceWalker`, `WorkspaceWalkConfig`, `path_to_url`, and `DocumentMatchers` unchanged.

Public API: one function (`workspace_diagnostics`) + three data types (`WorkspaceDiagnosticConfig` with walk knobs, `WorkspaceDiagnosticReport`, `DocumentDiagnostics` with `is_empty`/`diagnostics()`).

### 3.2 Consumers

- Its own doctest (a complete ~70-line `Server` + batch run example — the only in-repo place that teaches the "diagnostics server without transport" path);
- Four inline W0 tests (discovery, gitignore behavior, ignore toggle, open-before-request);
- A README "Tour" bullet: "**`oneshot`** — run a `Server` over files on disk with no LSP client: CLI-style batch diagnostics."
- No `examples/` usage, no other `src/` consumer, no mention in `CLAUDE.md` beyond the architecture summary.

### 3.3 What it shares with `server`

It is a thin *driver* over `LanguageServerWithState` — no duplication of state or dispatch logic. Its only private machinery is `OneshotServer` (120 lines) and document discovery (~90 lines). Crucially, it depends on `LanguageServerWithState`, which is `pub(crate)`: **`oneshot` cannot leave the crate without either making that type public or re-implementing the wrapper.**

### 3.4 Options

1. **Keep as-is (recommended).** It is the designed top layer of the arch-lint stack ("server must not depend on oneshot" — a direction that only exists because oneshot is a separate top module), it costs 581 lines, and it is the only path that exercises `LanguageServerWithState` clientlessly — which the doctest and batch-diagnostic use case need. Zero changes, zero churn.
2. **Fold into `server/`** (e.g. `server::oneshot`). Rejected: it inverts the layer story (server wiring would then contain a consumer of itself), breaks the arch-lint rule that gives the layering its teeth, and buys nothing — the "one module" it joins is not a pain point.
3. **Own crate** (`als-oneshot`). Rejected today: it has no independent compile identity — it depends on the whole core (state, dispatch, walker, matchers), so a split saves no compile time; it forces `LanguageServerWithState` (or a new public constructor/seam) into the public surface; and proj-split-crates' own criterion ("do not invent a package for a helper that has no independent user") fails — its only user is the owner's batch runs over the same facade. Revisit only if a second *clientless* entry point family grows around it (e.g. oneshot hover/rename), in which case "oneshot as the facade's second adapter" still beats a crate.

[Inference] Verdict: keep `oneshot` as a top-level module of the facade crate. The owner's confusion ("what is it for vs `server`") is a naming/docs gap, not a structural one: `server` is the *capability layer* (implement this), `oneshot` is a *runner* over it (no client attached). One sentence in `README.md` and the module docs would close it.

---

## 4. rust-skills digest (rules bearing on this decision)

Cited by id; applied to this repo:

**Workspace vs single crate / splitting**
- `proj-flat-small` — don't over-organize small projects. *Here:* 14k lines/93 files is past "flat", but the domain-module layout (2026-08-30 structure cycle) already provides the organization; this rule cautions against a second layer of nesting inside crates.
- `proj-split-crates` — extract modules that are *independently useful*; "do not invent a package for a helper that has no independent user." *Here:* the decisive criterion. `text_utils`, `documents`, `requests` have no independent user — everything funnels into one facade consumed by the owner's servers.
- `proj-workspace-large` — workspaces for *large* projects. *Here:* borderline; the macros crate alone already justifies a two-member workspace.
- `proj-workspace-deps` — `[workspace.dependencies]` inheritance. *Here:* adopt the moment there are ≥2 members (async-lsp, tokio, thiserror versions pinned once).
- `lint-workspace-lints` — `[workspace.lints]` + per-member `lints.workspace = true`. *Here:* directly relevant — the root's `[lints]` table is package-local, and the new `macros/` member currently escapes every lint the crate enforces (its placeholder uses `unwrap`/`expect`/`.unwrap()` in macros that the main crate denies).
- `proj-dependency-policy`, `proj-semver-contract` — lockfile committed, deliberate bumps. *Here:* one shared `Cargo.lock` already exists; keep it at the workspace root.

**Module organization**
- `proj-mod-rs-dir` — one layout style, consistently. *Here:* `mod.rs` style, additionally locked by `clippy::self_named_module_files`; any new crate copies it.
- `proj-feature-additive` — features strictly additive. *Here:* both existing features qualify; any per-crate feature re-declaration must keep them additive across the workspace.
- `proj-prelude-module` — no glob preludes. *Here:* the `server` facade re-export list is the prelude-equivalent; keep it named, never `pub use server::*` (next rule).
- `proj-no-glob-reexport` — no `pub use foo::*`; every public name needs a reviewable line. *Here:* already followed (facade re-exports are enumerated); becomes more important if a facade re-exports across crate boundaries.

**Visibility discipline**
- `proj-pub-crate-internal` — `pub(crate)` for internal APIs. *Here:* the direct mandate for the §2.3 sweep in `src/requests/`.
- `proj-pub-super-parent` — `pub(super)` for module-group-shared helpers. *Here:* `oneshot`'s `pub(super)` use is already exemplary; requests/registry stamper items could use `pub(super)`/`pub(crate)` granularity.
- `proj-pub-use-reexport` — one public path per owned item; foreign types imported from their defining crate. *Here:* the facade gives each type exactly one path; the `lsp_types`/`tree_sitter` root re-exports are the deliberate exception (facade convenience, matches downstream imports).
- `api-inherent-core` — core operations as inherent methods. *Here:* `ServerState`/`Document` already do this; preserve when moving.

**Macro-specific (directly applicable to the parallel lsp_macros work)**
- `macro-proc-two-crate` — proc macros in a dedicated `proc-macro = true` crate, re-exported from the facade; generated code should refer to facade types. *Here:* the two-crate split exists; the *facade re-export* of the macros does not yet (`async_language_server` does not re-export `lsp_macros`), and the placeholder emits `crate::…` paths instead of facade-absolute ones.
- `macro-absolute-std-paths` — emitted paths must resolve in the caller's namespace. *Here:* the placeholder's `crate::requests::Request`, `crate::server::ServerState`, `crate::server::Document` only resolve when expanded *inside* `async-language-server` (see §6.4).
- `macro-proc-error-spans` — never `panic!`/`unwrap`/`expect` in a proc macro; return spanned `syn::Error`. *Here:* the placeholder violates this throughout (`parse_macro_input!` is fine; `.expect("Failed to parse…")`, `.unwrap()`, `.expect("Missing 'method'…")` are not).
- `macro-declarative-before-proc` — prefer `macro_rules!` when the transform is writable-by-example. *Here:* the honest counterweight to the migration plan: eight of the fifteen macros are declarative stampers over data tables — the classic macro-by-example shape. The proc-macro case is strongest for attribute-driven per-request definitions; the plan should say which macros actually need AST power.
- `macro-private-helpers` — route macro-generated references through a `#[doc(hidden)]` private module. *Here:* the pattern serde uses (`#[doc(hidden)] mod private;` — verified in §5) is the template if the macros become downstream-visible.
- `doc-crate-readme` — `#![doc = include_str!("../README.md")]`. *Here:* already done at the facade; per-member crates should not repeat it unless they get their own READMEs.

---

## 5. Ecosystem patterns (verified against the repositories)

1. **tokio** — workspace members `tokio`, `tokio-macros`, `tokio-test`, `tokio-stream`, `tokio-util` (+ internal `benches`, `examples`, `stress-test`, `tests-build`, `tests-integration`), resolver 2, `[workspace.lints.rust]` at the root. Splitting criteria visible here: the proc-macro (tokio-macros) is isolated because it must compile for the host with a different dependency graph (syn/quote); the *test harness* (tokio-test) is a separate crate because test support must be dependency-weight-free for consumers; everything else stays in the one facade. tokio re-exports macro internals; users add only `tokio`.
   *(Verified: raw `Cargo.toml` on `master`.)*
2. **serde** — `serde` facade + `serde_derive` proc-macro crate (+ newer `serde_core`); derives are opt-in behind the facade feature (`#[cfg(feature = "serde_derive")] pub use serde_derive::{Deserialize, Serialize};`), core traits re-exported from `serde_core`, and generated code is served through `#[doc(hidden)] mod private;` — commented in-source as "Used by generated code and doc tests. Not public API."
   *(Verified: `serde/src/lib.rs` on `master`.)* Splitting criteria: proc-macro isolation, feature-gated re-export, hidden-but-stable path for generated code. The docs note they even inline `serde_core` for docs.rs because cross-crate re-exports render poorly — a real cost of splitting that our single-facade approach avoids.
3. **syn / quote / proc-macro2** — three crates layered by dependency direction (syn → quote (optional, behind `printing`) → proc-macro2), fine-grained additive features (`derive`, `parsing`, `printing`, `clone-impls`, `visit`, `fold`, `extra-traits`, `full`); syn's own workspace members are only dev/tooling crates. Splitting criterion: publish-independent versioning of the base layer (proc-macro2 changes at its own cadence), and feature-gated compilation cost.
   *(Verified: syn 3.0.5 `Cargo.toml` on `master`.)*
4. **tower** — members `tower`, `tower-layer`, `tower-service`, `tower-test` with `[workspace.dependencies]` inheritance; the two trait crates are minimal and stable, the facade re-exports them. Splitting criterion: trait-object stability — the tiny `Service`/`Layer` crates pin the vocabulary types so adapters can depend on them without the full facade.
   *(Verified: raw `Cargo.toml` on `master`.)*
5. **async-lsp (our direct upstream)** — deliberately *not* split: a single crate where capabilities (`client-monitor`, `omni-trait`, `stdio`, `tracing`, `forward`) are features, not crates. A framework of comparable purpose and size chose feature boundaries over crate boundaries.
   *(Verified: `Cargo.toml` 0.2.4 on `main`.)*

**Contraindications to splitting** (from the same set): small surface (async-lsp), single consumer identity (tokio-test is split only because test users must not pay for it — but it exists *as a dependency of tests*, not as an alternative facade), shared test harness (serde keeps doctest/generated-code plumbing inside the facade), and cross-crate rustdoc degradation (serde's documented docs.rs workaround).

---

## 6. Candidate target architectures

Facts that constrain all options:

- **The facade paths are a de-facto ABI.** Downstream servers import `async_language_server::lsp_types` and `async_language_server::server::*` (both examples, the README, the doctest). Any split must keep these paths resolving from the facade crate, or every downstream consumer breaks (no semver net — product.md).
- **`Document`'s fields are `pub(crate)`** and constructed by struct literal in `server/state/documents.rs` and `server/with_state/mod.rs`. Field privacy is crate-scoped: `documents/` cannot move to a different crate from `server/state` + `with_state` without adding constructor API.
- **`LanguageServerWithState` is `pub(crate)`** and is `oneshot`'s engine (§3.3): `oneshot` and `with_state` must share a crate.
- **Features are per-crate.** `tracing` = `dep:tracing` + `async-lsp/tracing` + `debug!/info!` calls scattered through `with_state`, `state`, `serve`; `tree-sitter` = `dep:tree-sitter` + `tree_sitter_utils` + `Document`'s gated fields/methods + `matcher` grammar field + `range_ext/tree_sitter` + the root `tree_sitter` re-export. Any crate that hosts gated code needs its own feature, and the facade forwards (`tracing = ["als-core/tracing", "dep:tracing", "async-lsp/tracing"]`).
- **Shared test fixtures are `#[cfg(test)] pub(crate)`** (`src/testing.rs`) and the wire harness reaches `pub(crate) run_over_streams` — both are crate-local by construction. A split either duplicates them per crate or promotes them into a `test-util`-style member (which makes them `pub` — a surface decision, tokio's `tokio-test` precedent).
- **README coupling:** `#![doc = include_str!("../README.md")]` belongs to whichever crate is the facade; member crates without READMEs simply don't do this.

### Option A — facade + `lsp_macros` (minimal)

Two workspace members: `async-language-server` (unchanged layout) + `macros/`. The restructure inside the single lib is the §2.3 visibility pass (49 `pub` → `pub(crate)` in `src/requests/`), plus workspace plumbing: `[workspace.dependencies]`, `[workspace.lints]` inherited by both members, `resolver = "2"` (already present), lints coverage for `lsp_macros` (currently lint-free).

- Features: unchanged.
- Fixtures/wire tests: unchanged (single crate).
- README/docs: unchanged.
- Downstream churn: zero.
- Macro interaction: emitted `crate::…` paths work as long as the macro expands inside `async-language-server`; the in-crate migration can proceed regardless of the path question.
- The 2026-08-30 structure design (D6: public API frozen, rustdoc-inventory diff) remains the template for the visibility pass.

**This is what the evidence supports.** It delivers both cycle goals — proc macros and visibility discipline — with the entire risk budget spent on code the compiler checks.

### Option B — facade + `als-core` + `lsp_macros`

`als-core` holds `documents/`, `requests/`, `workspace/`, `server/state`, `server/with_state`, `error.rs`, `text_utils`, `tree_sitter_utils`; the facade holds `serve.rs` (tower stack over stdio), the `server` facade module (re-exporting core types at today's paths), `oneshot` (stays with `with_state`? — no: `oneshot` needs `LanguageServerWithState`, so oneshot must follow `with_state` into core, or `LanguageServerWithState` becomes public — see risks).

- Module moves: everything above; the `server` facade becomes a re-export shell.
- Features: core declares `tracing`/`tree-sitter`; facade forwards both plus `async-lsp/tracing` and re-exports `tree_sitter` from core's dependency.
- Fixtures: `crate::testing` must either be duplicated into core (wire fixtures stay at the facade, since `run_over_streams` lives there) or promoted to a third `als-test-util` member with `pub` items. `with_state/tests.rs` (1555 lines) moves with core.
- README: facade keeps `#![doc]`; core needs its own (or none).
- Downstream churn: zero *if* the facade re-exports are complete and byte-identical; otherwise total breakage.
- Macro interaction: unchanged for in-crate use; for downstream use the macro must emit `::async_language_server::…` paths, which the facade must guarantee forever (a second, stronger ABI lock).
- What it buys: compile-time isolation of the transport/middleware from the core — meaningful only if a consumer embeds the core without `serve()` (e.g. an in-process host). [Inference] The owner's servers all run over stdio today, so that consumer is hypothetical.

### Option C — macros + utils + core + facade (maximal layering)

Additionally extracts `text_utils` (+ maybe `documents`) into an `als-text`/`als-docs` leaf crate, mirroring syn/quote/proc-macro2 layering.

- Blocked by evidence, not taste: `text_utils` has no independent user (proj-split-crates' explicit anti-pattern); `Document`'s `pub(crate)` fields forbid separating `documents` from `server/state` without new constructor API; the `workspace → requests` blessed edge would become an inter-crate dependency (acceptable directionally, but it means the "sibling" modules are not independently compilable anyway); and `tree-sitter` gating would have to be declared in three manifests (leaf range_ext, documents, core).
- [Inference] This is the architecture to *avoid* for this repository at its current size and audience.

### 6.4 The macros-layout interaction (flagged, not designed)

The `lsp_macros` placeholder (untracked `macros/src/lib.rs`) emits, from the call site:

- `impl crate::requests::Request for #struct_name` — resolves only inside `async-language-server` (in a downstream crate, `crate::` is the *downstream* crate);
- a `modify_response` body typed against `crate::server::ServerState` and `crate::server::Document`;
- a commented-out `crate::requests::registry::register::<#struct_name>()` — that path does not exist today (the registry is a set of macro tables, not a runtime registry).

Consequences for any layout:

1. **In-crate migration (Options A/B):** `crate::`-rooted emission works wherever the expansion lands inside the facade crate — *provided* `requests::Request`, `server::ServerState`, `server::Document` keep exactly those in-crate paths. Note `Request` is currently `pub` in a private module: the macro can still name it in-crate even if it becomes `pub(crate)`. **The §2.3 visibility sweep and the macro migration must agree on `Request`'s visibility in the same change**, or one of them breaks.
2. **Downstream use (any option):** emitted paths must become absolute (`::async_language_server::requests::Request`), which requires `requests` (or a `#[doc(hidden)]` private module per `macro-private-helpers`) to become part of the *public* facade surface — a deliberate surface growth that contradicts the visibility goal unless it is scoped to a hidden module. serde's `#[doc(hidden)] mod private;` is the verified precedent for exactly this tension.
3. **If a split happens (Option B+):** the emitted paths must point at the facade (which re-exports the types), never at `als_core::…` — otherwise the macro pins consumers to an internal crate name.
4. The proc-macro crate should adopt workspace lints and spanned-error discipline before any real migration (its current placeholder panics via `expect`/`unwrap`, violating `macro-proc-error-spans`, and carries Ukrainian comments, violating the English-artifacts rule; the uncommitted `[workspace] members` comment in the root `Cargo.toml` is likewise Ukrainian).

---

## 7. Risks and migration notes

- **Cyclic-dependency risk between proposed crates.** The current module graph has two path round-trips that are harmless in one crate and hostile between crates: (a) `documents → server::DocumentMatcher` (a facade path to its own sibling — a split turns this into documents→core or an import fix); (b) `workspace → requests` (the blessed `Request`-hook edge). Any crate boundary that separates workspace from requests is fine directionally, but a boundary that separates *requests* from *server/state* is not trivial: the `Request` trait signature takes `&ServerState` and `&Document`, so the trait and the state types must share a crate — they do in every option above, but it forecloses a "requests-only" crate.
- **`Document` field privacy** is the hard technical blocker for moving `documents/` away from `server/state` + `with_state` (§6). Either add constructors (surface growth) or keep them together.
- **`oneshot` + `LanguageServerWithState`** must share a crate (§3.3); splitting oneshot out forces a public wrapper type.
- **Feature propagation.** `tracing` must forward through `async-lsp/tracing` at the facade and re-declare `dep:tracing` in every crate with `debug!`/`info!` calls (with_state, state, serve, workspace diagnostics); `tree-sitter` must be declared in every crate that has gated code (leaf range_ext, documents, core) and the root `tree_sitter` re-export must keep resolving from the facade. Verification cost multiplies: the battery's three feature configurations become N-crate combinations; `--no-default-features` per member needs CI attention.
- **Test-harness duplication.** `src/testing.rs` fixtures are `pub(crate)` — not importable across crates. Options: duplicate (violates the duplication gate's spirit), promote to a `test-util` member with `pub` items and an additive feature (`test-util-feature`), or keep tests where the fixtures live. The wire harness additionally binds to `pub(crate) run_over_streams` — it must stay in the crate that owns `serve.rs`. `cargo dupes check` scopes across members is [Unverified] — the gate should be re-run per-crate after any split.
- **Downstream-visible churn.** Two kinds: (a) facade re-export completeness — Option B/C survive only with byte-identical `async_language_server::…` paths, verifiable with the 2026-08-30 D6 rustdoc-inventory diff; (b) crate *names* — if any member is ever consumed directly, its name is a new pin; keep `async-language-server` as the only consumer-facing name.
- **Workspace plumbing costs.** `[workspace.lints]` + `lints.workspace = true` per member (the macros crate currently escapes all lints); `[workspace.dependencies]` to prevent version drift; `arch-lint.toml` scope paths must be rewritten (or the layer rules retired in favor of the compiler-enforced crate graph — arguably a gain: `deny-scope-dep` becomes `Cargo.toml` dependency edges); `dupes.toml`/`.dupes-ignore.toml` scope per crate; CI runs the battery per member or via `--workspace`.
- **Proc-macro compile cost.** `syn 3` with `features = ["full"]` is a heavy host-side dependency; every build of the facade pays for it once the macros are used in-tree (`macro-declarative-before-proc` notes the cost). Consider narrowing syn features at adoption time.
- **Sequencing risk (macros × restructure).** The migration of 15 `macro_rules!` touches `requests/mod.rs`, `registry.rs`, `server_trait.rs`, `with_state/mod.rs` — the same files a visibility pass touches. Doing the `pub` → `pub(crate)` sweep and the macro migration in one change makes review hard; doing visibility first means the macro output's target visibility is already settled. [Inference] Visibility first, macros second.
- **Registry duplication risk.** The registry tables are the single source of truth for 48 requests across three expansion sites. Whatever shape the proc macros take, a design that lets a per-file attribute *and* the registry table both define the same request creates two sources of truth — the macros design needs to decide which side owns each fact (this is flagged, per the brief, not designed here).

---

## Provenance

- Line counts: `wc -l` over the main checkout (`feature/macros` @ `d389adf` + uncommitted workspace wiring), 2026-09-05.
- Dependency arrows: `grep` over `use crate::…` per module, cross-checked against `arch-lint.toml` scopes and per-file imports.
- Public-surface audit: `src/lib.rs` / `src/server/mod.rs` re-export reads + per-file `pub` declaration census + targeted reference checks (`crate::requests::*` consumers); LSP-assisted verification was planned but the research worktree's rust-analyzer index is 31 commits stale, so literal-path checks were used instead — appropriate here, since every claim involved literal use-paths rather than type inference.
- Oneshot: full read of `src/oneshot/*`, consumer grep across `src/`, `examples/`, `README.md`, `docs/`.
- rust-skills: rules read from `~/.claude/skills/rust-skills/rules/` (ids cited inline).
- Ecosystem: tokio, serde, syn, tower, async-lsp manifests/sources fetched from their repositories on 2026-09-05 (paths quoted in §5); anything not fetched is labeled [Unverified].
