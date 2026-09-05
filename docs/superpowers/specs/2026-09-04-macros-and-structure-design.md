# Macros & Structure — Design

**Cycle opened:** 2026-09-04 · **Design finalized:** 2026-09-05 · **Branch:** `feature/macros`
**Inputs:** frozen task prompt v2 + v3 amendment (owner chat, this cycle); structure research
`docs/superpowers/research/2026-09-05-project-structure-research.md` (sonnet[1m], read-only).
**Status:** presented section-by-section and approved in brainstorm; the dupes × trait question
resolved as T2 + family entry (owner, 2026-09-05).

## Goal

Migrate all 15 custom `macro_rules!` into the `lsp_macros` proc-macro crate as **five
procedural macros**, restructure request registration into distributed per-file form,
declare the `Server` trait with real docs and signatures (macro-stamped default bodies),
run the visibility sweep, complete workspace plumbing, and make `tracing` a permanent
dependency — with **zero behavior change** and **zero downstream-visible API change except
one owner-approved breaking change: the `tracing` feature is removed** (decision 10).

## Owner decisions log

1. **Consolidation — all four candidates:** helpers #12–14 become plain functions inside
   `lsp_macros` (not macros); stamper pairs merge; the three registry tables die in favor of
   distributed registration; the two dispatch engines share one skeleton. 15 definitions → 5
   (`lsp_request`, `lsp_dispatch`, `lsp_method`, `lsp_resolve_method`, `conversion_tests`).
2. **Distributed per-file registration:** one request file = one `#[lsp_request(...)]`
   invocation + colocated test macros. No central registry.
3. **Workspace architecture W1:** two members — the facade (current crate, layout unchanged)
   + `lsp_macros`. No `als-core`, no leaf crates (W2/W3 rejected on the research evidence:
   no independent consumers, hard blockers in `Document` field privacy and the
   `Request`↔`ServerState` binding; async-lsp itself chose features over crates).
4. **Trait site T4:** the 48 `Server` trait methods are written as real declarations —
   docs and signatures verbatim in source — with only the one-line default bodies stamped
   by `lsp_method!` / `lsp_resolve_method!`. **Zero new `.dupes-ignore.toml` entries**
   (gate condition, owner 2026-09-05: entries stay minimal and only genuinely justified;
   any proposed addition must carry an avoidance analysis first — this decision is that
   analysis for the trait). Backed by rust-skills `macro-prefer-functions` (stamping a
   trait for many methods is the sanctioned macro case) and `macro-no-rewrite-item` (the
   written signature is untouched; only the mechanical body is appended). **Documented
   fallback** if the trait-position probe fails (rustc rejecting a function-like proc
   macro in trait item position): plain hand-written methods plus exactly one "deliberate
   parallel family" entry (the notification-forwards precedent).
5. **Struct rename:** uniform `Request` suffix on all 48 marker structs.
6. **No `method = "..."` field:** no consumer exists; the wire-method string lives only
   inside async-lsp.
7. **Spike D3 is informational, not a gate:** migration proceeds regardless of
   rust-analyzer's behavior; the spike documents expected DX and decides the D6 form.
8. **tests/ directory rejected** for conversion tests (public-API visibility blocker +
   normative testing rule); per-request files host struct + attribute + inline tests.
9. **`oneshot` stays** a top-level module of the facade (research §3: folding inverts the
   arch-lint layering; a split forces `LanguageServerWithState` public). The owner-facing
   confusion is a docs gap — one README/module-doc sentence ships with this cycle.
10. **`tracing` becomes permanent** (owner, 2026-09-05, closing the prior cycle's
    deferral): the feature is removed, `tracing` and `async-lsp/tracing` are
    unconditional, `TracingLayer` is always in the middleware stack, and the 23
    `#[cfg(feature = "tracing")]` sites across 8 files disappear. Rationale: no real
    no-tracing consumer exists and the cfg noise is real (10 sites in `with_state/mod.rs`
    alone). Breaking for any consumer naming the feature — stated in the commit message
    per product.md. Landed in Plan 1: the manifest task absorbs the workspace side; a
    dedicated task (before the visibility sweep, since `cfg` against a removed feature
    fails `-D warnings`) deletes the sites and rewrites the five doc sites (tech.md,
    structure.md, error-handling.md, CLAUDE.md, README).
11. **No import aliases without a genuine collision** (owner, 2026-09-05): `use … as
    Name` exists only when two same-named types must coexist in scope (the "BaseCar"
    rule); `as _` trait imports are not aliases. The `Request` rename dissolves the one
    real collision in `src/requests/` (`SignatureHelp` marker vs the LSP type); the
    surviving whitelist is the `Ts…`/`Lsp…` tree-sitter-vs-LSP pairs in
    `text_utils`/`tree_sitter_utils` and the public `ErrorCode as ServerErrorCode`
    facade re-export. Enforced by the plans' global constraints and Plan 3's final
    sweep, where every survivor names its collision in a one-line comment.

## Architecture

Two workspace members. A request flows through three places, each real code or a thin macro:

```rust
// ① src/requests/hover.rs — REGISTRATION (distributed, one file per request)
#[lsp_request(
    params = async_lsp::lsp_types::HoverParams,
    response = Option<async_lsp::lsp_types::Hover>,
    document(text_document_position_params.text_document),
    incoming_position(text_document_position_params.position),
    outgoing(crate::requests::conversion::modify_outgoing_hover),
)]
pub struct HoverRequest;

// ② src/server/server_trait.rs — PUBLIC API (T4: real docs + signature,
//    the macro appends only the default body)
lsp_method! {
    /// Handles `textDocument/hover` requests from the client. ...
    fn hover(&self, _state: ServerState, _params: HoverParams)
        -> impl Future<Output = ServerResult<Option<Hover>>> + Send;
}

// ③ src/server/with_state/mod.rs — DISPATCH (one macro, thin rows)
lsp_dispatch! {
    hover: hover @ HoverRequest,
    // ... 41 more normal rows, then 6 resolve rows
    resolve(completion_resolve: completion_resolve @ CompletionResolveRequest),
}
```

**Deleted:** `registry.rs` (all three tables), `registry_request_impls!`,
`registry_trait_methods!`/`registry_trait_resolve_methods!`,
`registry_dispatch!`/`registry_dispatch_resolve!`, `implement_method!`,
`implement_resolve_method!`, `implement_methods!`, `request_extract_url!`,
`request_modify_params_position!`, `request_modify_params_range!` — the entire current
macro layer except `conversion_tests!`, whose definition moves.

**Data lives once:** types + hooks → ①; docs + trait signatures → ②; ident triples → ③
(the one irreducible mirror of method names between ② and ③, pinned by the completeness
test below). Adding a method remains the documented three-place pattern — all three places
now visible code.

## The `#[lsp_request]` attribute

Surface syntax uses valid attribute meta forms (NameValue for types, list-form for wiring):

| field | form | stamps |
|---|---|---|
| `params = <type path>` | NameValue | `type Params` |
| `response = <type path>` | NameValue | `type Response` |
| `document(<field path>)` | list | `extract_url` → `Some(params.<path>.uri.clone())` |
| `incoming_position(<field path>)` | list | `modify_params` → `convert_position(Incoming)` |
| `incoming_range(<field path>)` | list | `modify_params` → `convert_range(Incoming)` |
| `incoming_custom(<fn path>)` | list | extra step appended to `modify_params` (composes with `incoming_position`/`incoming_range`: standard conversion first, then the custom fn) |
| `outgoing(<fn path>)` | list | `modify_response` → calls the fn |
| `incoming_standalone(<fn path>)` | list | `modify_params_standalone` |
| `outgoing_standalone(<fn path>)` | list | `modify_response_standalone` |

Rules:

- All paths are full (`crate::requests::conversion::modify_outgoing_hover`) — this is what
  makes Go To Definition / Find Usages work at the call site.
- Fields are optional per request exactly as registry rows are today (`moniker` has no
  `outgoing`; `execute_command` has neither document nor hooks; resolve rows carry
  standalone hooks). Unspecified hooks keep the trait's delegating defaults — the
  delegation semantics of `modify_params`/`modify_response` are preserved verbatim.
- **Custom/generated unification:** the 17 hand-written `impl Request` blocks become
  attributes whose complex hooks are fn paths. The hook logic moves verbatim into free
  functions in the same file (module-level `fn convert_completion_response(state,
  document, response)`), directly unit-testable, called through the attribute wiring.
- **Validation:** unknown field, missing `params`/`response`, a malformed field path
  (not `a.b.c` segments), duplicate hook kinds — `syn::Error` with the span on the
  offending token → `compile_error!` at the exact attribute position. No `expect`/`unwrap`
  anywhere in `lsp_macros` (rule `macro-proc-error-spans`; also enforced by the workspace
  lints).
- The struct declaration stays hand-written in the file; the attribute only appends the
  `impl Request` block.

## `lsp_dispatch!`

Function-like proc macro; input is plain rows, resolve rows tagged:

```rust
lsp_dispatch! {
    hover: hover @ HoverRequest,              // our trait method : async-lsp method : type
    rename_prepare: prepare_rename @ RenamePrepareRequest,   // names differ for 5 methods
    // ...
    resolve(completion_resolve: completion_resolve @ CompletionResolveRequest),
}
```

Inside `lsp_macros` the engine is a **shared skeleton, two generators** — plain functions
`parse_row`, `dispatch_method(row) -> TokenStream`, `dispatch_resolve_method(row)` sharing
emit helpers (server/state proxies, `Box::pin`, conversion-document resolution,
staleness block); they diverge exactly where `implement_method!` and
`implement_resolve_method!` diverge today (URL path + staleness vs sole-document resolve
path). **Equivalence requirement:** the generated method bodies are line-for-line today's
macro expansion — same steps, same comments, same behavior; verified by the one-off
`cargo expand` diff (testing section).

## The `Server` trait (T4)

Each of the 48 request methods is a real declaration — doc comment and signature written
verbatim in the source — wrapped in a one-liner macro that appends the default body:

```rust
lsp_method! {
    /// Handles `textDocument/hover` requests from the client.
    ///
    /// Returns hover contents for the position in `params`, or `None` ...
    fn hover(&self, _state: ServerState, _params: async_lsp::lsp_types::HoverParams)
        -> impl Future<Output = ServerResult<Option<async_lsp::lsp_types::Hover>>> + Send;
}
lsp_resolve_method! {
    /// Handles `completionItem/resolve` requests from the client. ...
    fn completion_resolve(&self, _state: ServerState, item: async_lsp::lsp_types::CompletionItem)
        -> impl Future<Output = ServerResult<async_lsp::lsp_types::CompletionItem>> + Send;
}
```

- The macros re-emit the written item unchanged (docs, attributes, signature — every
  token) and append the default: `method_not_implemented(stringify!(name))` for the 42
  normal methods, `async move { Ok(item) }` for the 6 resolve methods. `macro-no-rewrite-
  item` satisfied by construction: nothing written is altered.
- Docs move **byte-identical** from registry rows to `///` (rendered rustdoc unchanged).
- Config methods and the 12 notification hooks stay as today (their empty bodies are
  below the dupes threshold; no wrapping needed).
- **Trait-position probe (first task of plan 2):** verify rustc accepts a function-like
  proc-macro invocation in trait item position (`syn::ItemTrait` has `TraitItem::Macro`,
  so macro calls in trait bodies parse; the proc-macro variant specifically is
  [Unverified] until probed). On failure, the documented fallback applies: plain
  hand-written methods plus exactly one deliberate-parallel-family entry in
  `.dupes-ignore.toml` (decision 4). A `proc-macro = true` crate cannot export
  `macro_rules!`, so there is no declarative fallback from inside `lsp_macros`.
- **Dupes gate:** zero new entries in the T4 path — each invocation is one opaque node.
- **Rename mapping:** uniform rule — append `Request`. Bases:
  - generated (31): Hover, Declaration, Definition, References, DocumentLink, Rename,
    RenamePrepare, DocumentFormat, DocumentRangeFormat, Implementation, TypeDefinition,
    DocumentHighlight, OnTypeFormatting, FoldingRange, LinkedEditingRange, CodeLens,
    WillSaveWaitUntil, DocumentColor, ColorPresentation, CallHierarchyPrepare,
    TypeHierarchyPrepare, Moniker, WillCreateFiles, WillRenameFiles, WillDeleteFiles,
    InlayHint, DocumentSymbol, ExecuteCommand, SemanticTokensFull, SemanticTokensRange,
    SemanticTokensFullDelta;
  - custom (11): Completion, CodeAction, DocumentDiagnostics, SelectionRange,
    IncomingCalls, OutgoingCalls, Supertypes, Subtypes, InlineValue, Symbol,
    SignatureHelp;
  - resolve (6): CompletionResolve, CodeActionResolve, DocumentLinkResolve,
    CodeLensResolve, InlayHintResolve, WorkspaceSymbolResolve.

## Visibility pass and workspace plumbing

1. **49 `pub` → `pub(crate)`** in `src/requests/` (`Request` + the 48 structs) — the only
   real visibility looseness found (research §2).
2. **`Request` is `pub(crate)` for good** in W1: macros are internal plumbing, emitted
   `crate::` paths resolve in-crate. The serde `#[doc(hidden)]` route for downstream macro
   exposure is deliberately deferred; revisit trigger: macros ever becoming
   downstream-visible.
3. **`[workspace.lints]`** — the current `[lints]` table moves to workspace level; both
   members inherit via `lints.workspace = true`. `lsp_macros` is born under every lint
   (expect/unwrap denies force span-accurate errors by construction).
4. **`[workspace.dependencies]`** — shared version pins (async-lsp, tokio, thiserror, …);
   syn/quote stay local to `lsp_macros` (sole consumer).
5. **`macros/Cargo.toml` parity:** `rust-version = "1.88"`, publish/description,
   `lints.workspace = true`.
6. **English-only fixes:** the Ukrainian workspace comment in the root `Cargo.toml`;
   the placeholder `macros/src/lib.rs` is replaced wholesale (already planned).
7. **arch-lint:** module map does not move in W1; scopes reviewed after the migration
   (`registry.rs` disappears; `deny-scope-dep` rules remain); behavioral rules in
   `tests/architecture.rs` unchanged.
8. **Normative docs updated with the cycle:** `.claude/rules/structure.md` (three-place
   pattern now = request file + trait method + dispatch row, no registry), `CLAUDE.md`
   architecture section, `README.md` if it names the registry; plus the one-sentence
   `oneshot` clarification (decision 9).

## Testing

- **The behavior-identical pin is the existing suite, unchanged:** `with_state/tests.rs`
  (1555 lines), wire tests, all `conversion_tests!` rows (call syntax preserved). All
  green before and after.
- **One-off expansion equivalence:** `cargo expand` on `with_state` before/after — the
  diff of generated dispatch methods must be empty modulo the struct rename.
- **New completeness pin (the design's only silent gap):** a trait method without a dispatch row
  answers `-32601` silently; everything else fails compilation. A parametrized wire test
  asserts each of the 48 methods answers ≠ `-32601` against an echo server.
- **`lsp_macros` unit tests:** the parse/emit functions are plain Rust
  (`parse_row`, `dispatch_method`, …) and are tested directly; error paths assert
  `syn::Error` spans/messages.
- **Dupes gate** re-run after migration; the target is **zero new entries** (decision 4).
  If any group surfaces — attribute wiring parallels, generated-hook families — each
  carries an avoidance analysis before an entry is even considered, and only a genuinely
  justified minimal set lands. Thresholds never change.
- **D6 form:** lean (b) — `conversion_tests!` stays a function-like proc macro with
  today's call syntax; rows remain ordinary module code. Variant (a)
  (`#[lsp_request_test]` stacked attribute) is reserved; the spike decides if it flips.

## Spike D3 (informational)

Immediately after the `lsp_macros` skeleton compiles: migrate `hover.rs` as the dogfood
file and check in the owner's editor (rust-analyzer): go-to-def on `params` type paths in
attribute args; go-to-def/find-usages on `outgoing` fn paths; completion inside attribute
args; fixture links inside `conversion_tests!` rows; error span quality on a deliberate
typo. Findings recorded in the task report; migration proceeds regardless (decision 7);
the D6 lean may flip on the findings.

## Sequencing

| plan | contents | done when |
|---|---|---|
| 1. Plumbing + hygiene + visibility | workspace lints/deps, `macros/Cargo.toml` parity, clean skeleton replacing the placeholder, 49 `pub`→`pub(crate)`, English fixes | battery green, both crates linted |
| 2. `lsp_macros` crate | **first task: the trait-position probe** (decision 4 fallback decides the trait form), then the five macros + shared skeleton as plain functions, parser/generator unit tests | dogfood `hover.rs` compiles and passes its conversion tests; **spike D3** runs right after |
| 3. Migration sweep (may slice 3a/3b) | 31 request files gain structs + attributes; 17 custom/resolve impls rewritten as attribute wiring over free fns; rename `Request` × 48; the trait via `lsp_method!`/`lsp_resolve_method!` with docs moved (or the fallback form per the probe); `lsp_dispatch!` table replaces five stamper invocations; registry + the 14 obsolete macro
definitions deleted (3 tables, 4 table-stampers, the aggregator, 2 engines, 3 helpers —
`conversion_tests!` moves instead); `requests/mod.rs` re-exports updated; normative docs + oneshot sentence; completeness wire test | full battery ×3 feature configs + dupes gate + `cargo expand` diff clean |

Visibility-first ordering (research §7): the macros emit paths against already-settled
visibility; each plan independently reviewable.

## Out of scope

Behavior changes; new LSP methods; registry-data changes beyond syntax (row content —
types, hooks, docs — is preserved); W2/W3 crate splits; downstream exposure of
`lsp_macros` (serde `#[doc(hidden)]` route deferred); a `tests/` directory; a `method`
string field; renames beyond the uniform `Request` suffix; unrelated debt cleanup.
