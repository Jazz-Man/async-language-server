# Macros & Structure — Plan 1: Plumbing, Hygiene, Visibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the two-member workspace to convention parity and close the visibility looseness: a clean `lsp_macros` skeleton replacing the invalid placeholder, `[workspace.lints]`/`[workspace.dependencies]` inheritance, manifest parity for `macros/Cargo.toml`, English-only manifest comments, and the 49-item `pub` → `pub(crate)` sweep confined to `src/requests/`.

**Architecture:** Pure plumbing — no behavior change, no public-API change, no macro migration yet. The skeleton lands first because lint inheritance (`expect_used = deny`, `unwrap_used = deny`) would fail on the current placeholder code; manifests change second; the visibility sweep is last and purely mechanical.

**Tech Stack:** Cargo workspaces (`[workspace.lints]`, `[workspace.dependencies]`, `lints.workspace = true`), edition 2024, pinned stable toolchain.

**Spec:** `docs/superpowers/specs/2026-09-04-macros-and-structure-design.md` — decisions 3 (W1), and the "Visibility pass and workspace plumbing" section. Research backing: `docs/superpowers/research/2026-09-05-project-structure-research.md` §2, §6 Option A, §7.

## Global Constraints

- The owner commits; the agent never runs git write commands. Each task ends with a checkpoint listing the changed files for the owner's commit — no `git add`/`git commit` steps anywhere.
- Zero behavior change. Zero public-API change except the owner-approved removal of the `tracing` feature (spec decision 10, breaking for consumers naming it) — the owner's commit message names it.
- The verification battery (from `.claude/rules/tech.md`, now covering BOTH workspace members): `cargo build --workspace --all-targets`, `cargo test --workspace`, `cargo test --workspace --no-default-features`, `cargo test --workspace --all-features`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- Toolchain pinned by `rust-toolchain.toml` (stable + rustfmt + clippy); never bypass with `rustup run` or `+nightly`.
- All written artifacts (code comments, manifest comments, docs) in English only.
- No `#[allow]`, `--cap-lints`, or lint suppression. When anything fails, invoke `superpowers:systematic-debugging` and `no-workarounds` skills and fix the root cause.
- `Cargo.lock` is committed and must not change semantically in this plan: `[workspace.dependencies]` moves pins, it does not relax them. If the lock diff shows a version movement, stop and investigate — do not proceed.
- MSRV: `rust-version = "1.88"` on both members.
- No `use … as Name` import aliases: an alias exists only to resolve a genuine name collision — two same-named types that must coexist in scope (the BaseCar rule, owner 2026-09-05). `as _` trait imports are not aliases. New/rewritten code uses real names.

## File Structure

| file | responsibility | action |
|---|---|---|
| `macros/src/lib.rs` | proc-macro crate root (skeleton now; five macros land in Plan 2) | rewrite wholesale |
| `macros/Cargo.toml` | proc-macro manifest reaching parity | modify |
| `Cargo.toml` | workspace + main manifest: lints hoisted, deps consolidated, English comment | modify |
| `src/requests/mod.rs` | `pub trait Request` → `pub(crate)`; stamper's `pub struct $req;` → `pub(crate) struct $req;` | modify 2 lines |
| `src/requests/<17 files>` | hand-written request structs → `pub(crate)` | modify 1 line each |
| `src/**` (8 files) + `Cargo.toml` + 5 doc files | `tracing` made permanent: cfg sites deleted, feature entry removed, feature docs rewritten | Task 3 |

The 17 hand-written files (custom + resolve): `code_action.rs`, `code_action_resolve.rs`, `code_lens_resolve.rs`, `completion.rs`, `completion_resolve.rs`, `document_diagnostics.rs`, `document_link_resolve.rs`, `inlay_hint_resolve.rs`, `inline_value.rs`, `incoming_calls.rs`, `outgoing_calls.rs`, `selection_range.rs`, `signature_help.rs`, `subtypes.rs`, `supertypes.rs`, `symbol.rs`, `workspace_symbol_resolve.rs`.

---

### Task 1: Clean `lsp_macros` skeleton

**Files:**
- Rewrite: `macros/src/lib.rs` (wholesale — the current content is an invalid copy-paste with `expect`/`unwrap` panics and Ukrainian comments)
- Modify: `macros/Cargo.toml` (parity fields only; the lints line lands in Task 2)

**Interfaces:**
- Consumes: nothing.
- Produces: a compiling, documented, lint-passing proc-macro crate root that Plan 2 fills with `lsp_request`, `lsp_dispatch`, `lsp_method`, `lsp_resolve_method`, `conversion_tests`. No public items yet, so `missing_docs` has nothing to fire on.

- [ ] **Step 1: Replace `macros/src/lib.rs` wholesale**

```rust
//! Procedural macros for `async-language-server`.
//!
//! Workspace-internal build plumbing — not part of the crate's public
//! surface. The macros arrive in the next plan: `#[lsp_request]`
//! (per-file request registration), `lsp_dispatch!` (dispatch entries for
//! the async-lsp `LanguageServer` impl), `lsp_method!` /
//! `lsp_resolve_method!` (`Server`-trait default bodies), and
//! `conversion_tests!` (W0 conversion-test stamping).
//!
//! Crate rules: input errors are reported as span-accurate
//! [`syn::Error`]s on the offending token — never `panic!`, `expect`, or
//! `unwrap` (`macro-proc-error-spans`); emitted code references call-site
//! paths, which resolve inside the main crate only.
```

(No items — an empty proc-macro crate is valid and compiles.)

- [ ] **Step 2: Bring `macros/Cargo.toml` to parity (fields only)**

```toml
[package]
name = "lsp_macros"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
license = "MIT"
publish = false
description = "Procedural macros for async-language-server (workspace-internal)"

[lib]
proc-macro = true

[dependencies]
quote = "1.0.47"
syn = { version = "3.0.4", features = ["full"] }
```

(No `[lints]` yet — that line lands in Task 2 together with `[workspace.lints]`, because `lints.workspace = true` errors while no workspace lints table exists. `syn`/`quote` stay local to this member: it is their sole consumer.)

- [ ] **Step 3: Verify the crate builds and lints clean under default clippy**

Run: `cargo build -p lsp_macros && cargo clippy -p lsp_macros --all-targets -- -D warnings`
Expected: both succeed; empty crate has nothing for clippy to flag.

- [ ] **Step 4: Verify rustdoc**

Run: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p lsp_macros`
Expected: success.

- [ ] **Step 5: Checkpoint (owner commits)**

Changed files: `macros/src/lib.rs`, `macros/Cargo.toml`.

---

### Task 2: Workspace lints, workspace dependencies, manifest inheritance

**Files:**
- Modify: `Cargo.toml` (root): hoist `[lints.*]` → `[workspace.lints.*]`, add `[workspace.dependencies]`, add `[lints] workspace = true`, fix the Ukrainian comment.
- Modify: `macros/Cargo.toml`: add `[lints] workspace = true`.

**Interfaces:**
- Consumes: Task 1's lint-clean skeleton (the hoisted `expect_used = deny` / `unwrap_used = deny` would fail on the old placeholder).
- Produces: both members under the identical lint table; shared dependency pins. Plan 2's macros compile under these lints from the first line.

- [ ] **Step 1: Rewrite the root `Cargo.toml` to this exact content**

```toml
[package]
name = "async-language-server"
version = "0.0.0"
edition = "2024"
license = "MIT"
publish = false
description = "A higher-level abstraction on top of async-lsp for building language servers with less boilerplate"
repository = "https://github.com/Jazz-Man/async-language-server"
readme = "README.md"
rust-version = "1.88"

[lib]
name = "async_language_server"
path = "src/lib.rs"

[features]
default = ["tracing", "tree-sitter"]
tracing = ["dep:tracing", "async-lsp/tracing"]
tree-sitter = ["dep:tree-sitter"]

[workspace]
members = [
    "macros",
    ".", # current directory (the main crate)
]
resolver = "2"

[workspace.dependencies]
# Shared version pins for workspace members. syn/quote stay local to
# lsp_macros (sole consumer).
async-lsp = { version = "0.2.4", default-features = false, features = ["client-monitor", "omni-trait"] }
dashmap = "6.2.1"
futures = "0.3.34"
globset = "0.4.20"
ignore = "0.4.33"
ropey = "1.6.1"
serde_json = "1.0.151"
thiserror = "2.0.20"
tokio = "1.53.1"
tower = "0.5.3"
tracing = "0.1.44"
tree-sitter = "0.26.13"
# Pinned exactly: 0.4 -> 0.5 silently changed what the "strict" preset means,
# and a `cargo update` must not shift preset semantics.
arch-lint = "=0.5.0"
tree-sitter-json = "0.24.8"

[dependencies]
async-lsp = { workspace = true }
dashmap = { workspace = true }
futures = { workspace = true }
globset = { workspace = true }
ignore = { workspace = true }
ropey = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true, features = ["io-std", "rt"] }
tower = { workspace = true }

tracing = { workspace = true, optional = true }
tree-sitter = { workspace = true, optional = true }

[dev-dependencies]
arch-lint = { workspace = true }
tokio = { workspace = true, features = ["rt", "rt-multi-thread", "macros", "time", "sync", "fs", "io-util"] }
tree-sitter-json = { workspace = true }

[[example]]
name = "tree_sitter"
required-features = ["tree-sitter"]

[lints]
workspace = true

[workspace.lints.clippy]
all = { level = "deny", priority = -3 }
# warn -> deny: CI already promotes warnings to errors via `-D warnings`;
# these levels make local runs and the manifest explicit.
cargo = { level = "deny", priority = -2 }
pedantic = { level = "deny", priority = -1 }

module_inception = { level = "allow", priority = 1 }
module_name_repetitions = { level = "allow", priority = 1 }
multiple_crate_versions = { level = "allow", priority = 1 }
similar_names = { level = "allow", priority = 1 }
unnecessary_wraps = { level = "allow", priority = 1 }

# Restriction lints, measured at zero fires on this codebase.
print_stdout = "warn"                # stdout is the protocol channel over stdio
print_stderr = "warn"
dbg_macro = "warn"
todo = "warn"
unimplemented = "warn"
mem_forget = "warn"
exit = "warn"                        # a library must not kill the process
undocumented_unsafe_blocks = "warn"  # crate is 100% safe code today
multiple_unsafe_ops_per_block = "warn"
allow_attributes_without_reason = "warn"  # encodes the suppression policy
unwrap_in_result = "warn"
panic_in_result_fn = "warn"
verbose_file_reads = "warn"
clone_on_ref_ptr = "warn"
integer_division = "warn"
error_impl_error = "warn"
self_named_module_files = "warn"     # locks in the mod.rs layout

# Fires today, adopted as explicit work items (see clippy.toml thresholds).
str_to_string = "warn"
cognitive_complexity = "warn"

# Takes over arch-lint's no-unwrap-expect rule, test-aware via clippy.toml.
expect_used = "deny"
unwrap_used = "deny"

[workspace.lints.rust]
missing_docs = "deny"
unsafe_code = "deny"
```

(The content is the current manifest verbatim except for: `[lints]` table hoisted to `[workspace.lints]` with levels untouched, `[workspace.dependencies]` added with the exact pins that are in use today, `[lints] workspace = true` added, and the workspace-members comment translated to English. Nothing else changes — the `[features]`, `[[example]]`, and dependency feature sets are byte-identical to today's.)

- [ ] **Step 2: Add the lints line to `macros/Cargo.toml`**

Append at the end of `macros/Cargo.toml`:

```toml

[lints]
workspace = true
```

- [ ] **Step 3: Verify the whole workspace lints clean**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: success, both members. If a lint fires inside `macros/`, fix the code (span-accurate errors instead of panics) — never suppress.

- [ ] **Step 4: Verify builds and the full default-feature test suite**

Run: `cargo build --all-targets && cargo test`
Expected: success; the same test count as before this plan (nothing behavioral changed).

- [ ] **Step 5: Verify dependency resolution did not shift**

Run: `cargo tree -p async-language-server -e normal --depth 1`
Expected: the same dependency set and versions as on the main branch (spot-check `async-lsp v0.2.4`, `ropey v1.6.1`, `tokio v1.53.1`). `Cargo.lock` should show no version movement in the owner's diff — if it does, stop and investigate (Global Constraints).

- [ ] **Step 6: Checkpoint (owner commits)**

Changed files: `Cargo.toml`, `macros/Cargo.toml` (and `Cargo.lock` only if cargo rewrote formatting — a version change is a stop-signal, see Step 5).

---

### Task 3: `tracing` becomes permanent

**Files:**
- Modify: `src/server/serve.rs` (unconditional import + layer — the exact edits below)
- Modify: 7 more files carrying `#[cfg(feature = "tracing")]` lines: `src/server/with_state/mod.rs` (10 sites), `src/server/state/documents.rs` (3), `src/server/with_state/initialize.rs` (2), `src/workspace/diagnostics.rs` (2), `src/documents/matcher.rs` (2), `src/requests/conversion.rs` (1), `src/workspace/walker.rs` (1)
- Modify: `Cargo.toml` (feature removal + dependency flip — AFTER the code edits, so no `cfg` ever references a removed feature under `-D warnings`)
- Modify: `.claude/rules/tech.md`, `.claude/rules/structure.md`, `.claude/rules/error-handling.md`, `CLAUDE.md`, `README.md`

**Interfaces:**
- Consumes: Task 2's workspace manifests (`[workspace.dependencies]` already carries `tracing`).
- Produces: tracing compiled in unconditionally; `TracingLayer` always in the middleware stack; the feature matrix reduced to `tree-sitter` (`--no-default-features` = "no tree-sitter"). Plan 2's macro code and Plan 3's migration never see a tracing cfg.

- [ ] **Step 1: Unconditional import and layer in `serve.rs`**

The import block (currently lines 7–16 with the gated import at 15–16) becomes — `TracingLayer` joins the main `use async_lsp::{...}` list, the separate gated import disappears:

```rust
use async_lsp::{
    client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    panic::CatchUnwindLayer, router::Router, server::LifecycleLayer, tracing::TracingLayer,
};
```

The stack construction (currently the gated shadowing at lines 84–88) becomes one unconditional chain:

```rust
        let builder = ServiceBuilder::new()
            .layer(LifecycleLayer::default())
            .layer(TracingLayer::default());
```

- [ ] **Step 2: Delete every remaining `#[cfg(feature = "tracing")]` attribute line**

The exhaustive site list (grep-verified at plan time; line numbers are advisory, the attribute text is the match):

```
src/server/with_state/mod.rs:17,312,320,334,342,352,362,369,377,388
src/server/state/documents.rs:244,355,388
src/server/with_state/initialize.rs:11,85
src/workspace/diagnostics.rs:289,349
src/documents/matcher.rs:130,143
src/requests/conversion.rs:950
src/workspace/walker.rs:69
```

For statement-level sites (a `debug!`/`warn!`/`info!` call): delete only the attribute line, keep the call. For the import sites (`use tracing::debug;` in `with_state/mod.rs:17` and `initialize.rs:11`): delete the attribute line, keep the import (fold into the file's existing `use` group where one is adjacent).

- [ ] **Step 3: Remove the feature from `Cargo.toml`**

Three deltas to the manifest Task 2 produced:

```toml
[features]
default = ["tree-sitter"]
tree-sitter = ["dep:tree-sitter"]
```

(the `tracing = ["dep:tracing", "async-lsp/tracing"]` line is deleted)

`[workspace.dependencies]` — the async-lsp entry gains the tracing feature permanently:

```toml
async-lsp = { version = "0.2.4", default-features = false, features = ["client-monitor", "omni-trait", "tracing"] }
```

`[dependencies]` — `tracing` stops being optional:

```toml
tracing = { workspace = true }
tree-sitter = { workspace = true, optional = true }
```

- [ ] **Step 4: Rewrite the five documentation sites**

`.claude/rules/structure.md` (the `serve()` paragraph, currently naming `` `TracingLayer` (`tracing` feature) ``): drop the parenthetical — the line reads `LifecycleLayer`, `TracingLayer`, `ConcurrencyLayer(8)`, `CatchUnwindLayer`, `ClientProcessMonitorLayer` with no feature note.

`.claude/rules/error-handling.md` ("No swallowed failures", first bullet + its example): the bullet starts "Fire-and-forget client requests log their failure through `tracing`," (drop "under the `tracing` feature"), and the example loses its `#[cfg(feature = "tracing")]` line:

```rust
tracing::warn!("request failed: {error}");
```

`CLAUDE.md` (Commands, feature-gates bullet):

```markdown
- Feature gates matter: the default feature is `tree-sitter` (`tracing` is permanent, not a feature). Changes touching `#[cfg(feature = "tree-sitter")]` paths should also be checked with `cargo test --no-default-features`
```

`.claude/rules/tech.md` — replace the "Feature gates" section wholesale:

```markdown
## Feature gates

One feature, on by default (`[features]` in `Cargo.toml`):

- `tree-sitter` — adds `tree_sitter_utils`, the grammar field on
  `DocumentMatcher`, and syntax-tree access on `Document`.

`tracing` is a permanent, non-optional dependency (owner decision 2026-09-05,
spec decision 10): no consumer disables it, and `TracingLayer` is always in
the middleware stack. Code under `#[cfg(feature = "tree-sitter")]` must also
compile without it; when a change touches the gated path, verify at least
`cargo test --no-default-features` in addition to the default configuration.
```

`README.md` — replace the "Feature flags" section:

```markdown
## Feature flags

One feature, default on: `tree-sitter` (per-document grammars,
`tree_sitter_utils`). Tracing is always compiled in — middleware spans and
handler logging need no flag.
```

- [ ] **Step 5: Verify no gated sites remain**

Run: `grep -rn 'cfg(feature = "tracing")' src/ examples/`
Expected: empty output.

- [ ] **Step 6: Run the full battery**

```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo test --workspace --no-default-features
cargo test --workspace --all-features
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```
Expected: all green in all three configurations; `--no-default-features` now builds the crate without tree-sitter only.

- [ ] **Step 7: Checkpoint (owner commits)**

Changed files: `Cargo.toml`, the 8 `src/` files above, `.claude/rules/tech.md`, `.claude/rules/structure.md`, `.claude/rules/error-handling.md`, `CLAUDE.md`, `README.md`. **The owner's commit message names the breaking change** (feature removal), per product.md.

---

### Task 4: The visibility sweep — 49 `pub` → `pub(crate)` in `src/requests/`

**Files:**
- Modify: `src/requests/mod.rs` (two lines: the trait declaration and the stamper's struct emission)
- Modify: the 17 hand-written request files listed in File Structure (one line each)

**Interfaces:**
- Consumes: nothing from Tasks 1–3 (independent of the manifest and tracing work).
- Produces: `Request` and all 48 request structs as `pub(crate)` — the visibility Plan 2's macro-emitted `crate::requests::Request` paths and Plan 3's re-exports target. No public path ever named these types (research §2: the module is private; references live only in `crate::requests::*`, `crate::server::with_state`, and `crate::workspace`'s `DocumentDiagnostics` hook).

- [ ] **Step 1: Change the trait and the stamper in `src/requests/mod.rs`**

Line with the trait declaration (currently `pub trait Request {`):

```rust
pub(crate) trait Request {
```

Line inside `macro_rules! registry_request_impls` (currently `pub struct $req;`):

```rust
            pub(crate) struct $req;
```

- [ ] **Step 2: Change the 17 hand-written struct declarations**

In each of the 17 files, the declaration `pub struct X;` becomes `pub(crate) struct X;` — e.g. in `completion.rs`:

```rust
pub(crate) struct Completion;
```

The full set of struct names: `CodeAction`, `CodeActionResolve`, `CodeLensResolve`, `Completion`, `CompletionResolve`, `DocumentDiagnostics`, `DocumentLinkResolve`, `InlayHintResolve`, `InlineValue`, `IncomingCalls`, `OutgoingCalls`, `SelectionRange`, `SignatureHelp`, `Subtypes`, `Supertypes`, `Symbol`, `WorkspaceSymbolResolve`.

- [ ] **Step 3: Verify nothing `pub` remains in `src/requests/`**

Run: `grep -rnE "^pub (trait|struct|enum|fn|mod|use|const|type)" src/requests/`
Expected: empty output. (The pattern deliberately matches only bare `pub `, not `pub(crate)`.)

- [ ] **Step 4: Run the full battery over both members**

Run:
```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo test --workspace --no-default-features
cargo test --workspace --all-features
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```
Expected: all green, same test counts as the pre-plan baseline in each configuration. A compile error pointing at some `pub` use elsewhere is a signal to inspect that site — convert it to `pub(crate)` if it is internal (expected), and stop to investigate if it turns out to be reachable from a public path (would contradict the research; surface it rather than patch it).

- [ ] **Step 5: Checkpoint (owner commits)**

Changed files: `src/requests/mod.rs` plus the 17 files above. This is the plan's final checkpoint — the battery in Step 4 doubles as the plan-level done criterion.

---

## Self-Review (performed at plan writing)

- **Spec coverage:** spec "Visibility pass and workspace plumbing" items 1–6 map to Tasks 4, 2, 2, 2, 2, 1+2; decision 10 (`tracing` permanent) maps to Task 3; items 7–8 (arch-lint review, the remaining normative-docs updates) belong to Plan 3 by the spec's sequencing. ✓
- **Placeholder scan:** no TBD/TODO; every step carries exact content or an exact command with expected output. ✓
- **Type consistency:** no new types introduced; the only identifiers named (`Request`, the 17 struct names) are verbatim from the current tree. ✓
