# DyLint Integration — `strict-lints` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the portable `strict-lints` dylint suite (9 custom lints + 21 stock Trail of Bits lints), retire arch-lint entirely, and wire the dylint pass into CI and the local battery, per `docs/superpowers/specs/2026-09-07-dylint-integration-design.md`.

**Architecture:** An isolated `lints/` cargo workspace (own pinned nightly toolchain) holds one cdylib crate `strict-lints` with nine `LateLintPass` lints; the repo root gets a `dylint.toml` naming both the local suite and the pinned ToB stock suites. arch-lint's layer engine, sync-IO rule, and thiserror rule port into the suite; its remaining duties move to clippy or are dropped per the spec's parity map. The stable battery is untouched apart from two additive Cargo.toml entries.

**Tech Stack:** dylint 6.0.4 (tag `v6.0.4`), `dylint-linting` + `dylint-testing` 6.0.4 from crates.io, `clippy_utils` pinned to git rev `9fca3bc9fc2bc83c60bde26d18ed68f11564b228`, toolchain `nightly-2026-05-28` (+ `rustc-dev`, `llvm-tools-preview`), rustc `LateLintPass` HIR APIs.

## Revision 2026-09-08 — arch-lint retained (owner decision, supersedes conflicting task text)

- **Task 4 revised:** scope is `no_sync_io` ONLY (kept as a *portable* lint — arch-lint covers this repo, the suite covers the owner's other projects). `layer_boundaries` is CANCELLED and stripped from the tree. Task 4's original text remains only as detection-spec reference for no_sync_io.
- **Task 6 SKIPPED entirely:** no arch-lint demolition (`arch-lint.toml`, `tests/architecture.rs`, the dev-dep, and the inert comment-allows all stay), and no `let_underscore_must_use` clippy addition (arch-lint's `NoSilentResultDrop` keeps owning that axis).
- **Task 5 unchanged** (warn trio — not arch-lint-related).
- **Task 7 unchanged** (CI `dylint` job, `cargo dylint --all -- --all-targets` from repo root).
- **Task 8 adjusted:** the D10 trim covers only duties owned by strict-lints (T2/T3/T4-revised/T5 lints) and clippy; `structure.md`'s layer text stays arch-lint-owned. The suite is **8 custom lints**. Keep: `lints/README.md`, battery sync, `CLAUDE.md` line, and the spec §4 three-`allowed_paths` correction for wire_boundary.
- **Task 9 adjusted:** acceptance audit drops all layer/sync-IO-replacement items; the canary step covers every deny lint (`error_display_lowercase`, `error_no_string_catch_all`, `require_thiserror`, `wire_boundary`, `no_sync_io`, `no_sleep_in_tests`); arch-lint's green run stays part of the `cargo test` battery.
- **Note:** `require_thiserror` (Task 2, already committed and reviewed) is technically an AL005 port; the duplication with arch-lint is accepted for landed work. Deleting it is a separate owner decision, not part of this revision.

## Revision 2026-09-08 (later same day) — custom suite deleted, perfectionist adopted

Second owner reversal: the `strict-lints` suite is deleted entirely — Tasks 2–5 landed and were removed in the same cycle (the suite tree deleted in 974faf4 "Replace strict-lints with perfectionist suite", the CI drop in d6e7dad "Drop strict-lints suite from dylint CI"). The third-party `perfectionist` suite replaces it at pin `0.0.0-rc.22` in `dylint.toml` (three reasoned disables, one ignore knob); the stock ToB suites stay at tag `v6.0.4`.

- **Task 8 = docs sync + narrowed MD trim** (per the task brief; no `lints/README.md` — the suite no longer exists): battery line + lint-stack paragraph in `tech.md`, one `CLAUDE.md` command line, this revision record and the spec's second addendum, and the D10 trim narrowed to clippy-covered duties.
- **Task 9 = final battery + acceptance audit with the re-worded gate:** the five stable battery lines plus `cargo dylint --all -- --all-targets` green; every suite-specific audit item (custom-lint list, ui fixtures, `cd lints` self-check) is gone.

## Global Constraints

- **No git writes by agents.** Every task ends with a *Checkpoint* step listing the task's file group; the owner commits. Never run `git add`/`git commit`/`git push`.
- **Zero suppressions:** no `#[allow]`, no `#[cfg_attr(... allow ...)]` anywhere in `src/`, `examples/`, or fixtures outside the suite's own tree. `grep -rn 'allow(' src examples` must stay empty. Deny lints land at zero measured fires; warn lints are advisory.
- **Exact pins** (verbatim): dylint git tag `v6.0.4`; `lints/rust-toolchain.toml` → `channel = "nightly-2026-05-28"`, `components = ["llvm-tools-preview", "rustc-dev"]`; `clippy_utils = { git = "https://github.com/rust-lang/rust-clippy", rev = "9fca3bc9fc2bc83c60bde26d18ed68f11564b228" }`; `dylint-linting = "6.0.4"`; `dylint-testing = "6.0.4"` (dev-dep).
- **The dylint run command** is `cargo dylint --all -- --all-targets` (the `--all-targets` forward is what lets lints see test code; source: dylint README's VS Code/CI examples).
- **The stable battery is unchanged** except two additive entries in root `Cargo.toml`: the `unexpected_cfgs` `check-cfg` line and `let_underscore_must_use = "warn"` (Task 6).
- **English only** in all written artifacts (code comments, docs, fixtures, commit messages belong to the owner).
- **The suite is portable:** every lint no-ops when its target symbols/crates are absent. No lint may hard-depend on this repository's layout except through `dylint.toml` config values.
- **The rustc-internal API is unstable.** The canonical skeleton is pinned by the toolchain above; if a compiler-internals API differs on `nightly-2026-05-28`, adapt to what compiles on that toolchain — never `unsafe`, never a suppression, never a different nightly.
- **House style for lint code:** mirror the ToB examples — `#![feature(rustc_private)]`, `extern crate` declarations, `impl_late_lint!` docs block (`### What it does`, `### Why is this bad?`, `### Example` with rust fences), `clippy_utils` helpers over hand-rolled traversal.

---

### Task 1: Scaffold `lints/` workspace + adopt the 21 stock lints

**Files:**
- Create: `lints/Cargo.toml`, `lints/rust-toolchain.toml`, `lints/.cargo/config.toml`, `lints/src/lib.rs`
- Create: `dylint.toml` (repo root)
- Modify: `Cargo.toml` (root; add the `unexpected_cfgs` entry)

**Interfaces:**
- Produces: a loadable (empty) `strict-lints` library at path `lints`; root `dylint.toml` with the stock entries; a green `cargo dylint --all -- --all-targets` run; a recorded stock-fire triage list (input to Task 8's README levels).

- [ ] **Step 1: Install the dylint tooling**

Run: `cargo install cargo-dylint dylint-link --locked`
Expected: both binaries install (a few minutes; cached in `~/.cargo/bin`).

- [ ] **Step 2: Create the toolchain and linker config**

`lints/rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly-2026-05-28"
components = ["llvm-tools-preview", "rustc-dev"]
```

`lints/.cargo/config.toml` (the ToB examples' universal-target linker pattern):

```toml
[target.'cfg(all())']
linker = "dylint-link"
```

- [ ] **Step 3: Create the suite crate (empty, loadable)**

`lints/Cargo.toml`:

```toml
[package]
name = "strict-lints"
version = "0.1.0"
edition = "2024"
publish = false
description = "Portable strictness lints shared across the owner's Rust projects"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
clippy_utils = { git = "https://github.com/rust-lang/rust-clippy", rev = "9fca3bc9fc2bc83c60bde26d18ed68f11564b228" }
dylint-linting = "6.0.4"
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
dylint-testing = "6.0.4"

[workspace]

[lints.clippy]
nursery = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
option_if_let_else = "allow"

[lints.rust.unexpected_cfgs]
level = "deny"
check-cfg = ["cfg(dylint_lib, values(any()))"]

[package.metadata.rust-analyzer]
rustc_private = true
```

`lints/src/lib.rs` (empty library — no lints yet; `impl_late_lint!` arrives in Task 2):

```rust
//! `strict-lints`: portable dylint lints shared across the owner's Rust
//! projects. See `lints/README.md` for adoption.

dylint_linting::dylint_library!();

/// Registers this library's lints with the compiler (empty until Task 2).
pub fn register_lints(_sess: &rustc_session::Session, _lint_store: &mut rustc_lint::LintStore) {}
```

Note: `[workspace]` (empty table) detaches the directory from the root workspace; the root `Cargo.toml` `[workspace] members` list is left untouched — an empty-table workspace is not a member candidate.

Verify it builds: `cd lints && cargo build` (rustup downloads `nightly-2026-05-28` + components on first use; if it errors instead of downloading, run `rustup toolchain install nightly-2026-05-28 -c rustc-dev,llvm-tools-preview` and retry).
Expected: build succeeds.

- [ ] **Step 4: Wire the root dylint.toml (stock only for now) and the check-cfg entry**

`dylint.toml` (repo root):

```toml
# Dylint configuration: library discovery + per-library config.
# Docs: https://github.com/trailofbits/dylint#workspace-metadata

[workspace.metadata.dylint]
libraries = [
    { path = "lints" },
    { git = "https://github.com/trailofbits/dylint", tag = "v6.0.4", pattern = [
        "examples/general",
        "examples/supplementary",
        "examples/restriction/assert_eq_arg_misordering",
        "examples/restriction/collapsible_unwrap",
        "examples/restriction/misleading_variable_name",
    ] },
]
```

Root `Cargo.toml`, inside the existing `[workspace.lints.rust]` section (after `unsafe_code`):

```toml
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(dylint_lib, values(any()))"] }
```

- [ ] **Step 5: First full run + stock triage**

Run from the repo root: `cargo dylint --all -- --all-targets`
Expected: dylint builds the ToB suites at tag `v6.0.4` (first run is slow — driver + toolchains), then checks the workspace. Record every stock-lint fire.

Triage policy (spec D3): fix the fire in our code, or drop that lint from the `pattern` list, recording the reason as a `#` comment next to the pattern entry. Never silence a fire with an allow.

- [ ] **Step 6: Confirm the stable battery still passes**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --all-features`
Expected: both green (nothing in `src/` changed).

- [ ] **Step 7: Checkpoint**

File group: `lints/Cargo.toml`, `lints/rust-toolchain.toml`, `lints/.cargo/config.toml`, `lints/src/lib.rs`, `dylint.toml`, root `Cargo.toml`. Owner commits.

---

### Task 2: Suite skeleton + the error pass (3 lints)

**Files:**
- Create: `lints/src/error_display.rs`, `lints/src/error_no_string.rs`, `lints/src/require_thiserror.rs`, `lints/tests/ui.rs`
- Create fixtures: `lints/ui/error_display/{bad_case,bad_punct,ok_lowercase,transparent_from}.rs`, `lints/ui/error_no_string/{bad_tuple,bad_named,ok_two_fields}.rs`, `lints/ui/require_thiserror/{bad_plain,ok_derived}.rs`
- Modify: `lints/Cargo.toml` (add `thiserror` dev-dep), `lints/src/lib.rs` (register 3 lints)

**Interfaces:**
- Produces: the canonical lint skeleton (`impl_late_lint!` + `LateLintPass` + ui harness) that Tasks 3–5 copy; shared helper `error_enum<'tcx>(item: &'tcx Item) -> Option<&'tcx EnumDef<'tcx>>` in `lints/src/lib.rs`… (helper stays private to each file — three small copies beat a premature abstraction; copy the variant-walk into each lint).
- Lint names registered (exact): `ERROR_DISPLAY_LOWERCASE` / lint id `error_display_lowercase`; `ERROR_NO_STRING_CATCH_ALL` / `error_no_string_catch_all`; `REQUIRE_THISERROR` / `require_thiserror`.

- [ ] **Step 1: Dev-dependency for fixtures**

Add to `lints/Cargo.toml` `[dev-dependencies]`:

```toml
thiserror = "2"
```

- [ ] **Step 2: Write the ui harness and the failing fixtures**

`lints/tests/ui.rs`:

```rust
//! UI tests for the `strict-lints` suite. Each fixture is a small crate
//! compiled with the suite loaded; `.stderr` files are the snapshots.
//! Regenerate snapshots with: `BLESS=1 cargo test` (run inside `lints/`).

#[test]
fn ui() {
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui").run();
}
```

If `dylint_testing::ui::Test` differs on 6.0.4, read the actual API in `~/.cargo/registry/src/*/dylint-testing-6.0.4/src/ui.rs` and adapt the call — the contract is: fixtures under `lints/ui/`, snapshots compared, `BLESS=1` regenerates.

Fixture `lints/ui/error_display/bad_case.rs` (uppercase first letter):

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("Failed to read the document")]
    Read,
}

fn main() {}
```

Fixture `lints/ui/error_display/bad_punct.rs` (trailing punctuation):

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("failed to read the document.")]
    Read,
}

fn main() {}
```

Fixture `lints/ui/error_display/ok_lowercase.rs` (stays silent):

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("failed to read the document")]
    Read,
    #[error("document {path} is invalid")]
    Invalid { path: std::path::PathBuf },
}

fn main() {}
```

Fixture `lints/ui/error_display/transparent_from.rs` (transparent combined with `#[from]`):

```rust
#[derive(thiserror::Error)]
enum E {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn main() {}
```

Fixture `lints/ui/error_no_string/bad_tuple.rs`:

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("unknown failure")]
    Unknown(String),
}

fn main() {}
```

Fixture `lints/ui/error_no_string/bad_named.rs`:

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("unknown failure")]
    Unknown { message: String },
}

fn main() {}
```

Fixture `lints/ui/error_no_string/ok_two_fields.rs` (a wire message is data, not a stringified cause):

```rust
#[derive(thiserror::Error)]
enum E {
    #[error("json-rpc error {code}: {message}")]
    Rpc { code: i32, message: String },
}

fn main() {}
```

Fixture `lints/ui/require_thiserror/bad_plain.rs`:

```rust
enum NotDerivedError {
    A,
}

fn main() {}
```

Fixture `lints/ui/require_thiserror/ok_derived.rs`:

```rust
#[derive(thiserror::Error)]
enum DerivedError {
    #[error("e")]
    A,
}

fn main() {}
```

- [ ] **Step 3: Run the harness to verify it fails (no lints registered yet)**

Run: `cd lints && cargo test`
Expected: the harness compiles; fixtures produce no diagnostics and no `.stderr` exists — with the lints unregistered the `bad_*` fixtures cannot fire. (This step proves the harness runs; the red/green cycle is Step 4→5.)

- [ ] **Step 4: Implement the three lints**

Canonical skeleton (adapted from `examples/general/abs_home_path` @ v6.0.4 — this exact shape, per file):

```rust
#![feature(rustc_private)] // at crate top, i.e. in lib.rs, NOT per lint file

// per lint file:
extern crate rustc_ast;
extern crate rustc_hir;
extern crate rustc_lint;

use clippy_utils::diagnostics::span_lint_and_help;
use rustc_hir::{Item, ItemKind};
use rustc_lint::{LateContext, LateLintPass};
```

`lints/src/lib.rs` gains (crate root):

```rust
#![feature(rustc_private)]
#![warn(unused_extern_crates)]
```

`lints/src/error_display.rs` — fires on `check_item` for `ItemKind::Enum`:

```rust
dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Checks `#[error("...")]` messages on error enums: the first alphabetic
    /// character must be lowercase and the literal must not end in `.`, `!`, or `?`.
    /// Also denies `#[error(transparent)]` combined with `#[from]`.
    ///
    /// ### Why is this bad?
    /// Uppercase starts and trailing punctuation break the house Display
    /// convention; `transparent` + `#[from]` forwards the `source()` call into
    /// an inner error that usually has no source of its own, silently breaking
    /// the error chain.
    ///
    /// ### Example
    /// ```rust
    /// # #[derive(thiserror::Error)]
    /// enum E {
    ///     #[error("Failed to read.")]
    ///     Read,
    /// }
    /// ```
    /// Use instead:
    /// ```rust
    /// # #[derive(thiserror::Error)]
    /// enum E {
    ///     #[error("failed to read the document")]
    ///     Read,
    /// }
    /// ```
    pub ERROR_DISPLAY_LOWERCASE,
    Deny,
    "`#[error(...)]` messages must start lowercase, carry no trailing punctuation, and must not be `transparent` on a `#[from]` variant",
    ErrorDisplayLowercase
}

#[derive(Default)]
pub struct ErrorDisplayLowercase;

impl<'tcx> LateLintPass<'tcx> for ErrorDisplayLowercase {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        let ItemKind::Enum(enum_def, _) = &item.kind else {
            return;
        };
        for variant in enum_def.variants {
            // Walk `variant.attrs` as syntax trees: find the attribute whose
            // path is `error`. Inspect its args:
            //  - a single string literal -> message check:
            //      * first char satisfying char::is_alphabetic must be lowercase
            //      * last char must not be one of '.', '!', '?'
            //  - the bare word `transparent` -> fire iff any field of the
            //    variant carries a `from` attribute.
            // Emit with span_lint_and_help(cx, ERROR_DISPLAY_LOWERCASE, span, msg, |diag| ...)
        }
    }
}
```

The comment block above is the algorithm; write the real body against `rustc_ast::ast::MetaItemKind`/`NestedMetaItem` on `nightly-2026-05-28` (the attribute args parse as `MetaList` of `Lit`/`List`). Porting reference for attribute walking: any lint in `~/.cargo/registry/src/*/dylint-linting-6.0.4` and the clippy_utils source at the pinned rev. Do not weaken the checks to make compilation easier.

`lints/src/error_no_string.rs` — same enum walk; fire when a variant's fields are all `String`/`&str` (single unnamed field `Unknown(String)`, or named fields where every field is `String`/`&str` and at least one exists). Type detection: the HIR `Ty` is a `Path` whose final segment is `String`, or `Kind::Ptr`/`Rptr` to `str`-segment path. Message: "error variant stringifies its failure; carry a typed source instead". Level `Deny`.

`lints/src/require_thiserror.rs` — `check_item` for `ItemKind::Enum` whose `item.ident.as_str().ends_with("Error")`: fire unless one of the item's `derive` attributes contains a path whose final segment is `Error`. Documented detection caveat in the lint docs: the check is on derive-path text (`thiserror::Error` or `use thiserror::Error; derive(Error)` both end in `Error`). Level `Deny`.

Register all three in `lints/src/lib.rs`:

```rust
dylint_linting::dylint_library!();

pub fn register_lints(_sess: &rustc_session::Session, lint_store: &mut rustc_lint::LintStore) {
    lint_store.register_lints(&[
        error_display::ERROR_DISPLAY_LOWERCASE,
        error_no_string::ERROR_NO_STRING_CATCH_ALL,
        require_thiserror::REQUIRE_THISERROR,
    ]);
    lint_store.register_late_pass(|_| {
        Box::new(error_display::ErrorDisplayLowercase::default())
    });
    lint_store.register_late_pass(|_| {
        Box::new(error_no_string::ErrorNoStringCatchAll::default())
    });
    lint_store.register_late_pass(|_| Box::new(require_thiserror::RequireThiserror::default()));
}

mod error_display;
mod error_no_string;
mod require_thiserror;
```

(If `impl_late_lint!` on 6.0.4 already registers the pass itself, follow the macro's real contract from its source; the outcome requirement is: `cargo dylint list --path lints` shows the three lints.)

- [ ] **Step 5: Generate snapshots and go green**

Run: `cd lints && BLESS=1 cargo test && cargo test`
Expected: `.stderr` snapshots generated for the six `bad_*`/`transparent_from` fixtures, none for the `ok_*` ones; second run green. Review each `.stderr` by eye — it must contain the lint id and a sensible message.

- [ ] **Step 6: The suite runs green on this repo**

Run from root: `cargo dylint --all -- --all-targets`
Expected: the three new lints report zero fires on this codebase (its error enum already complies — verify, don't assume: if a fire appears, it is a real finding; fix the code, not the lint).

- [ ] **Step 7: Checkpoint**

File group: `lints/src/{lib.rs,error_display.rs,error_no_string.rs,require_thiserror.rs}`, `lints/tests/ui.rs`, `lints/ui/**`, `lints/Cargo.toml`. Owner commits.

---

### Task 3: `wire_boundary` (first configurable lint)

**Files:**
- Create: `lints/src/wire_boundary.rs`
- Create fixtures: `lints/ui/wire_boundary/{bad_construct,bad_from_impl,ok_read}.rs`, plus `lints/.cargo/dylint.toml`… — no: fixture-scoped config is not supported; instead the no-op case is `lints/ui/wire_boundary/noop_without_dep.rs` (a fixture with no `async-lsp` dependency at all — see Step 2).
- Modify: `lints/Cargo.toml` (`async-lsp` dev-dep for fixtures), `lints/src/lib.rs` (register), `dylint.toml` (config table)

**Interfaces:**
- Produces: the configurable-lint pattern Tasks 4 reuses — a `#[derive(serde::Deserialize)]` config struct read from the linted workspace's `dylint.toml` via `dylint_linting`'s config module.
- Config contract (consumed by the root `dylint.toml` here, and by any adopting project):

```toml
[strict-lints.wire_boundary]
allowed_paths = ["src/server/with_state/**", "src/workspace/diagnostics.rs"]
```

- [ ] **Step 1: Dev-dependency**

```toml
async-lsp = "0.2"
```

in `lints/Cargo.toml` `[dev-dependencies]` (fixtures import `async_lsp::ResponseError`; version matches the root workspace's 0.2.4 line).

- [ ] **Step 2: Fixtures**

`lints/ui/wire_boundary/bad_construct.rs`:

```rust
use async_lsp::ResponseError;

fn main() {
    let _e = ResponseError {
        code: 0,
        message: "boom".into(),
        data: None,
    };
}
```

`lints/ui/wire_boundary/bad_from_impl.rs`:

```rust
use async_lsp::ResponseError;

#[derive(Debug)]
struct DomainError;

impl From<DomainError> for ResponseError {
    fn from(_: DomainError) -> Self {
        ResponseError {
            code: 0,
            message: "boom".into(),
            data: None,
        }
    }
}

fn main() {}
```

`lints/ui/wire_boundary/ok_read.rs` (reading a wire error is fine — the rule is about construction):

```rust
use async_lsp::ResponseError;

fn describe(e: &ResponseError) -> String {
    format!("{}: {}", e.code, e.message)
}

fn main() {}
```

`lints/ui/wire_boundary/noop_without_dep.rs` (no `async_lsp` import; the whole crate has no async-lsp target — this fixture must stay silent, proving the portability no-op):

```rust
fn main() {
    let response_error = 3;
    let _ = response_error;
}
```

- [ ] **Step 3: Implement the lint**

`lints/src/wire_boundary.rs`:

```rust
// Skeleton; detection algorithm in comments is the contract.

dylint_linting::impl_late_lint! {
    /// ### What it does
    /// Denies constructing the protocol error type (`ResponseError`) and
    /// implementing `From<X> for ResponseError` outside configured adapter
    /// paths. Reading a `ResponseError` is not flagged.
    ///
    /// ### Why is this bad?
    /// Domain code must speak typed domain errors; exactly one boundary
    /// converts them to the wire form. A second construction site is a second
    /// boundary and erases the domain error model.
    ///
    /// ### Configuration
    /// `allowed_paths` in the linted workspace's `dylint.toml`, e.g.
    /// ```toml
    /// [strict-lints.wire_boundary]
    /// allowed_paths = ["src/adapter/**"]
    /// ```
    pub WIRE_BOUNDARY,
    Deny,
    "constructing `ResponseError` outside the wire adapter breaks the one-boundary rule",
    WireBoundary
}
```

Detection algorithm:
1. Early no-op unless some crate in `cx.tcx.crates()` (plus `LOCAL_CRATE`) has name `async_lsp` or `lsp_types` — `cx.tcx.crate_name(cnum).as_str()`.
2. `check_item`: for `ItemKind::Impl` whose self type resolves (`QPath::Resolved` last segment) to a type named `ResponseError` and whose trait is `From`, fire unless the item's source file matches an `allowed_paths` glob.
3. `check_expr`: for `ExprKind::Call`/`ExprKind::Struct` whose path (`QPath::Resolved`) resolves (`cx.qpath_res`) to a `DefKind::Ctor(Struct)`/associated fn of a type named `ResponseError`, fire unless the enclosing file matches.
4. File matching: `cx.sess().source_map().span_to_filename(span)` → string → strip the workspace prefix → glob match with the `globset`-style matching from the config. Add `globset = "0.4"` to `[dependencies]` for this.
5. Config: a `#[derive(Default, serde::Deserialize)]` struct `Config { allowed_paths: Vec<String> }` read from the linted workspace's `dylint.toml` via dylint_linting's config module (read `~/.cargo/registry/src/*/dylint-linting-6.0.4/src/config.rs` for the exact entry point on 6.0.4 — it keys off `env!("CARGO_PKG_NAME")`). Empty/missing config = no paths allowed.

Register in `lib.rs` (`WIRE_BOUNDARY` + pass) and add the config table to root `dylint.toml` exactly as in the Interfaces block.

- [ ] **Step 4: Snapshots + green**

Run: `cd lints && BLESS=1 cargo test && cargo test`
Expected: `.stderr` for `bad_construct` and `bad_from_impl`; silence for `ok_read` and `noop_without_dep`.

- [ ] **Step 5: Repo run**

Run from root: `cargo dylint --all -- --all-targets`
Expected: zero `wire_boundary` fires (all construction sites live in the two blessed paths — verify by searching `ResponseError {` and `ResponseError::` in `src/`; every hit must sit under an allowed path or be a read).

- [ ] **Step 6: Checkpoint**

File group: `lints/src/{lib.rs,wire_boundary.rs}`, `lints/ui/wire_boundary/**`, `lints/Cargo.toml`, `dylint.toml`. Owner commits.

---

### Task 4: The arch-lint ports — `no_sync_io` + `layer_boundaries`

**Files:**
- Create: `lints/src/no_sync_io.rs`, `lints/src/layer_boundaries.rs`
- Create fixtures: `lints/ui/no_sync_io/{bad_fs_read,default_config_fires,ok_async_alt}.rs`; `lints/ui/layer_boundaries/{bad_use,bad_inline_path,ok_within_scope,ok_unscoped}.rs`
- Modify: `lints/src/lib.rs` (register both), `dylint.toml` (both config tables)

**Interfaces:**
- Produces: config contracts consumed by the root `dylint.toml` (and adoptable anywhere):

```toml
[strict-lints.no_sync_io]
allowed_paths = ["src/server/state/documents.rs", "src/server/state/workspace.rs", "src/workspace/walker.rs", "benches/**"]

[strict-lints.layer_boundaries]
[[strict-lints.layer_boundaries.scopes]]
name = "text-utils"
paths = ["src/text_utils/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "documents"
paths = ["src/documents/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "workspace"
paths = ["src/workspace/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "lsp-requests"
paths = ["src/lsp_requests/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "server"
paths = ["src/server/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "oneshot"
paths = ["src/oneshot/**"]

[[strict-lints.layer_boundaries.scopes]]
name = "lsp-macros"
paths = ["macros/src/**"]

[[strict-lints.layer_boundaries.deny]]
from = "text-utils"
to = ["documents", "lsp-requests", "workspace", "server", "oneshot"]
message = "text_utils is a leaf; conversion machinery must not leak into it."

[[strict-lints.layer_boundaries.deny]]
from = "documents"
to = ["lsp-requests", "workspace", "oneshot"]
message = "documents must not depend on request handling, workspace scanning, or the clientless entry point."

[[strict-lints.layer_boundaries.deny]]
from = "lsp-requests"
to = ["documents", "workspace", "oneshot"]
message = "encoding conversion must not reach into sibling domains or the clientless entry point."

[[strict-lints.layer_boundaries.deny]]
from = "lsp-macros"
to = ["text-utils", "documents", "lsp-requests", "workspace", "server", "oneshot"]
message = "the proc-macro crate generates library code but must not depend on the library it serves."

[[strict-lints.layer_boundaries.deny]]
from = "workspace"
to = ["documents", "oneshot"]
message = "workspace scanning stays below the document store and the clientless entry point."

[[strict-lints.layer_boundaries.deny]]
from = "server"
to = ["oneshot"]
message = "server plumbing must not depend on the clientless oneshot entry point."
```

(Config semantics identical to `arch-lint.toml`, which this replaces; the seven scopes and six deny rules are ported verbatim from it — the source file exists until Task 6 removes it, read it there.)

- [ ] **Step 1: Fixtures**

`lints/ui/no_sync_io/bad_fs_read.rs`:

```rust
use std::path::Path;

fn main() {
    let _ = std::fs::read_to_string("x.txt");
    let _ = Path::new("x.txt").exists();
}
```

`lints/ui/no_sync_io/ok_allowed_file.rs` — same content as `bad_fs_read.rs`; silence comes from config. Since fixture crates cannot carry per-file dylint config, instead cover the allowed-path branch by testing the config plumbing in Step 3 against a scratch workspace (see Step 4), and use this fixture for the *default* (empty config) behavior. Rename it `default_config_fires.rs` with the same content.

`lints/ui/no_sync_io/ok_async_alt.rs`:

```rust
fn main() {
    // async alternatives are not sync IO; nothing to flag
    let _ = "tokio::fs::read is not in the deny set";
}
```

`lints/ui/layer_boundaries/bad_use.rs` — fixture layout: the ui harness compiles each fixture as its own crate root, so scopes need paths relative to the fixture. Use directory-relative scopes in the *test* config (Step 3 wires a `layer_boundaries` test config under `lints/`), or simpler: make the fixtures self-contained by using module paths inside one file — a `use` of a module declared later in the same crate. Concretely:

```rust
mod outer {
    pub mod a {
        pub fn f() {}
    }
    pub mod b {
        // flagged when a test config denies b -> a
        use crate::outer::a;

        pub fn g() {
            a::f();
        }
    }
}

fn main() {}
```

with a sibling `.stderr` snapshot. Mirror the same structure for `bad_inline_path.rs` (no `use`; call `crate::outer::a::f()` fully qualified from `b`), `ok_within_scope.rs` (`b` calls its own `b::h`), `ok_unscoped.rs` (a path into a file matching no scope glob).

- [ ] **Step 2: Implement `no_sync_io`**

`check_expr` on call expressions: resolve the callee's `DefId` (`cx.typeck_results().type_dependent_def_id` or the `QPath::Resolved` of `ExprKind::Call`), then `cx.tcx.def_path_str(did)`. Fire when:
- `def_path_str` starts with `std::fs::`, or
- it is one of `std::path::Path::exists`, `std::path::Path::metadata`, `std::path::Path::is_file`, `std::path::Path::is_dir`, or
- it starts with `std::io::{Read,Write,BufRead}::` read/write trait methods on files — keep v1 to the two bullets above; the trait-method bullet is YAGNI.

Blessing: the enclosing file (source_map path, workspace-prefixed-stripped) matching an `allowed_paths` glob from the `Config { allowed_paths: Vec<String> }` (same config pattern as Task 3). Level `Deny`. Skip `is_in_test` contexts (`clippy_utils::is_in_test(cx.tcx, expr.hir_id)`), matching arch-lint's `allow_in_tests` behavior for benches… benches are NOT `is_in_test` — keep benches blessed via `allowed_paths = […, "benches/**"]` as in the Interfaces block.

- [ ] **Step 3: Implement `layer_boundaries`**

Config structs:

```rust
#[derive(Default, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    scopes: Vec<Scope>,
    #[serde(default)]
    deny: Vec<Deny>,
}

#[derive(serde::Deserialize)]
pub struct Scope {
    name: String,
    paths: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct Deny {
    from: String,
    to: Vec<String>,
    #[serde(default)]
    message: String,
}
```

Pass state: precompiled `globset::GlobSet` per scope (scope name → matcher), plus a `HashMap<Deny>` keyed by `from`. Source-file → scope resolution: `cx.sess().source_map().span_to_filename` → normalized path → first matching scope.

`check_item` for `ItemKind::Use`: resolve each `UseTree` leaf via `cx.qpath_res`-equivalent (`import_res` on the item's `HirId` via `cx.tcx.hir_attrs`… use the resolved `Res` from `tcx` for the use item — `rustc_hir` `ItemKind::Use` gives the path; resolve through `cx.qpath_res(cx.tcx.hir, ...)` or `tcx` `resolutions`; the pinned clippy_utils has path-resolution helpers — prefer them). If `Res::Def(def_id)` and `def_id.is_local()`: target scope = scope of the def's `span` file; source scope = scope of the use's file; fire when `deny[source].contains(target)`, message from the rule.

`check_expr` + `check_ty` for inline qualified paths (`QPath::Resolved` whose leading segments name crate-local modules): same source/target scope comparison. This is arch-lint's "check inline qualified paths" parity.

Level `Deny`. Fixture runs need a config: put a test config in `lints/dylint.toml` — **no**: dylint.toml at the *linted* workspace. The ui harness lints fixtures under the suite's own package; give the SUITE's workspace a `lints/dylint.toml` carrying the fixture-scoped scopes (`ui/layer_boundaries/**` layout) and the real repo config stays in the root `dylint.toml`. Write that file with fixture-relative scopes:

```toml
# Fixture-scoped config: the ui harness lints `lints/` itself, so this file
# drives layer_boundaries during `cargo test` inside lints/.
[strict-lints.layer_boundaries]
[[strict-lints.layer_boundaries.scopes]]
name = "a-scope"
paths = ["src/outer/a/**", "**/outer/a*"]

[[strict-lints.layer_boundaries.scopes]]
name = "b-scope"
paths = ["src/outer/b/**", "**/outer/b*"]

[[strict-lints.layer_boundaries.deny]]
from = "b-scope"
to = ["a-scope"]
message = "fixture: b must not depend on a."
```

(Glob sets must match the fixture's temporary layout — verify against how `dylint_testing` stages fixtures on 6.0.4 and adjust the globs so the intent holds; the intent is exactly: b→a fires, b→b and unscoped stay silent.)

- [ ] **Step 4: Snapshots, green, and a scratch-workspace proof of `allowed_paths`**

Run: `cd lints && BLESS=1 cargo test && cargo test`
Expected: `.stderr` for `default_config_fires`, `bad_use`, `bad_inline_path`; silence for the others.

Then prove the config plumbing end-to-end: create a throwaway cargo crate in `/tmp/` with a `dylint.toml` blessing its own file for `no_sync_io`, run `cargo dylint --lib-path <path to built suite> --path /tmp/scratch` (or `--path lints`-style invocation per the 6.0.4 CLI help), expect silence; remove the blessing, expect a fire. Delete the scratch crate after.

- [ ] **Step 5: Repo run**

Run from root: `cargo dylint --all -- --all-targets`
Expected: zero `no_sync_io` fires outside the blessed files (the six former arch-lint allow-sites now live in `allowed_paths`), zero `layer_boundaries` fires (the layer rules hold today — they hold in arch-lint's green test). Any fire is a real finding or a wrong glob; fix the code or the glob, never silence.

- [ ] **Step 6: Checkpoint**

File group: `lints/src/{lib.rs,no_sync_io.rs,layer_boundaries.rs}`, `lints/ui/{no_sync_io,layer_boundaries}/**`, `lints/dylint.toml`, `dylint.toml`. Owner commits.

---

### Task 5: The warn trio — `no_sleep_in_tests`, `bool_params_public`, `panics_doc_debt`

**Files:**
- Create: `lints/src/no_sleep_in_tests.rs`, `lints/src/bool_params.rs`, `lints/src/panics_doc_debt.rs`
- Create fixtures: `lints/ui/no_sleep_in_tests/{bad_thread,bad_tokio,ok_outside_test}.rs`; `lints/ui/bool_params_public/{bad_pub_fn,ok_trait_impl}.rs`; `lints/ui/panics_doc_debt/bad_doc.rs`
- Modify: `lints/Cargo.toml` (`tokio` dev-dep), `lints/src/lib.rs` (register three)

**Interfaces:**
- Produces: two warn-level advisory lints + one deny lint; all fully portable (no config).

- [ ] **Step 1: Dev-dependency**

```toml
tokio = "1"
```

- [ ] **Step 2: Fixtures**

`lints/ui/no_sleep_in_tests/bad_thread.rs`:

```rust
#[test]
fn waits() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
```

`lints/ui/no_sleep_in_tests/bad_tokio.rs`:

```rust
#[tokio::test]
async fn waits() {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}
```

`lints/ui/no_sleep_in_tests/ok_outside_test.rs`:

```rust
fn main() {
    // not test code: the lint deliberately does not fire here
    let sleep_is_allowed_here = true;
    assert!(sleep_is_allowed_here);
}
```

The two `bad_*` fixtures compile as test crates: the ui harness call gains `.rustc_flags(["--test"])` (as ToB's `abs_home_path` does). Split into a second harness test in `lints/tests/ui.rs`:

```rust
#[test]
fn ui_test_target() {
    dylint_testing::ui::Test::src_base(env!("CARGO_PKG_NAME"), "ui_test_target")
        .rustc_flags(["--test"])
        .run();
}
```

with the two bad fixtures moved under `lints/ui_test_target/no_sleep_in_tests/`.

`lints/ui/bool_params_public/bad_pub_fn.rs`:

```rust
pub fn configure(verbose: bool) {}

fn main() {}
```

`lints/ui/bool_params_public/ok_trait_impl.rs`:

```rust
pub trait Conf {
    fn configure(&self, verbose: bool);
}

pub struct S;
impl Conf for S {
    fn configure(&self, verbose: bool) {
        let _ = verbose;
    }
}

fn main() {}
```

(`fn main` above has no `bool` param — the trait impl signature is exempt because it must match the trait.)

`lints/ui/panics_doc_debt/bad_doc.rs`:

```rust
/// Does a thing.
///
/// # Panics
///
/// Panics if `x` is zero.
pub fn thing(x: u32) -> u32 {
    1 / x
}

fn main() {}
```

- [ ] **Step 3: Implement the three lints**

`no_sleep_in_tests` (Deny): `check_expr` on calls whose callee def-id matches `clippy_utils::match_def_path(cx, did, &["std", "thread", "sleep"])` or `&["tokio", "time", "sleep"]` (for `tokio::time::sleep` the callee resolves into the `tokio` crate — match via `match_def_path`); guard with `clippy_utils::is_in_test(cx.tcx, expr.hir_id)`. Doc section notes the determinism rule: channel gates and bounded waits, never sleeps.

`bool_params_public` (Warn): `check_fn`; fire when the fn is public (`cx.tcx.visibility(owner_did).is_public()`), not inside a trait impl (walk to the parent `ItemKind::Impl` and skip when `of_trait` is `Some`), not in a test context, and any non-self parameter's HIR type is `TyKind::Ptr`/path resolving to `bool`. Message suggests an enum or newtype.

`panics_doc_debt` (Warn): `check_fn` on public fns; read `cx.tcx.hir_attrs(body_id.hir_id.owner.into())` — simpler: `cx.tcx.hir.get_attrs(hir_id, sym::doc)` — scan the rendered doc for a `# Panics` section; fire with "documented panic contract is type-first debt: a type may be able to remove the invalid state".

- [ ] **Step 4: Snapshots + green**

Run: `cd lints && BLESS=1 cargo test && cargo test`
Expected: `.stderr` for the four bad fixtures; silence elsewhere.

- [ ] **Step 5: Repo run**

Run from root: `cargo dylint --all -- --all-targets`
Expected: `no_sleep_in_tests` zero fires (the test suite uses channel gates and `WIRE_TIMEOUT` bounds — verify with `grep -rn 'sleep' src benches`; any hit in test context is a real finding). `bool_params_public` and `panics_doc_debt` are warnings — list their fires in the task report (advisory; fixes optional in this cycle, findings recorded for Task 8's README notes).

- [ ] **Step 6: Checkpoint**

File group: `lints/src/{lib.rs,no_sleep_in_tests.rs,bool_params.rs,panics_doc_debt.rs}`, `lints/tests/ui.rs`, `lints/ui/**`, `lints/ui_test_target/**`, `lints/Cargo.toml`. Owner commits.

---

### Task 6: arch-lint demolition + clippy parity

**Files:**
- Delete: `arch-lint.toml`, `tests/architecture.rs`
- Modify: `Cargo.toml` (remove `arch-lint` from `[workspace.dependencies]` and `[dev-dependencies]`; add `let_underscore_must_use`), `Cargo.lock` (regenerates), `benches/oneshot_diagnostics.rs` + `src/server/state/{documents,workspace}.rs` + `src/workspace/walker.rs` (remove inert `// arch-lint: allow(...)` comments)

**Interfaces:**
- Consumes: Tasks 4–5 (the replacing lints are green on this repo).
- Produces: a workspace with exactly two lint stacks — clippy + strict-lints.

- [ ] **Step 1: Remove the dependency and the gate**

In root `Cargo.toml`: delete the `arch-lint = "=0.5.0"` line from `[workspace.dependencies]` and the `arch_lint = { workspace = true }`-equivalent line from `[dev-dependencies]` (it appears as `arch-lint = { workspace = true }`; remove it). Delete `arch-lint.toml` and `tests/architecture.rs`. Run `cargo check --workspace` to regenerate `Cargo.lock` (arch-lint + its core/macros/rules crates drop out).

- [ ] **Step 2: Sweep the inert suppressions**

Run: `grep -rn "arch-lint" src benches examples 2>/dev/null`
Delete every `// arch-lint: allow(...) reason="..."` comment found (expected sites: `src/server/state/documents.rs`, `src/server/state/workspace.rs`, `src/workspace/walker.rs`, `benches/oneshot_diagnostics.rs` — the grep is the oracle). No code changes — the comments are inert.

- [ ] **Step 3: clippy takes over `NoSilentResultDrop`**

In root `Cargo.toml`, in the restriction block (after `cognitive_complexity = "warn"`), add:

```toml
let_underscore_must_use = "warn"       # NoSilentResultDrop parity; `let _ =` on a must-use is a swallowed failure
```

Run: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | head -50`
Expected: either clean, or a bounded list of `let_underscore_must_use` fires. Each fire gets judged: fix it (handle the result, or name it `_x` only where the failure is impossible by construction — prefer an explicit fix) or record it in the task report as an accepted warn (it does not gate). No `#[allow]`.

- [ ] **Step 4: Full test battery without arch-lint**

Run: `cargo test --workspace --all-features && cargo test --workspace --no-default-features && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all green; `cargo test` no longer contains the architecture test (the `tests/` directory disappears entirely — `tests/architecture.rs` was its only file; remove the empty dir).

- [ ] **Step 5: dylint still green**

Run from root: `cargo dylint --all -- --all-targets`
Expected: green (layer/sync-io coverage now lives here).

- [ ] **Step 6: Checkpoint**

File group: deleted `arch-lint.toml` + `tests/architecture.rs`; modified `Cargo.toml`, `Cargo.lock`, `benches/oneshot_diagnostics.rs`, `src/server/state/documents.rs`, `src/server/state/workspace.rs`, `src/workspace/walker.rs`. Owner commits. The commit message should name the behavioral move (layer checks: `cargo test` → dylint pass) — the owner writes it.

---

### Task 7: CI — the `dylint` job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: everything above (a suite that runs green locally).
- Produces: a `dylint` job running on every push/PR/dispatch.

- [ ] **Step 1: Add the job**

In `.github/workflows/ci.yml`, after the `test` job (before `release`):

```yaml
  dylint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: ./.github/actions/rust-setup
      - name: Install dylint
        run: cargo install cargo-dylint dylint-link --locked
      - name: Cache dylint artifacts
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin
            ~/.dylint_drivers
            ~/.rustup/toolchains
            lints/target
            target/dylint
          key: dylint-${{ runner.os }}-${{ hashFiles('lints/Cargo.toml', 'lints/rust-toolchain.toml', 'dylint.toml') }}
      - name: Lint (strict-lints + stock suites)
        run: cargo dylint --all -- --all-targets
      - name: Suite self-check (fmt + clippy)
        run: |
          cd lints
          cargo fmt --check
          cargo clippy --all-targets -- -D warnings
```

Notes: the cache key rolls when the suite manifest, toolchain pin, or library list changes; `~/.rustup/toolchains` makes the nightly + components a one-time download. The `rust-setup` composite keeps the stable battery caches separate.

- [ ] **Step 2: Validate the workflow syntax**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo OK`
Expected: `OK` (YAML parses; GitHub's schema is exercised on the next push).

- [ ] **Step 3: Checkpoint**

File group: `.github/workflows/ci.yml`. Owner commits. (First real validation happens on the owner's next push — flag in the report that the first CI run of this job should be watched.)

---

### Task 8: Docs — `lints/README.md`, battery sync, MD trim-to-pointers

**Files:**
- Create: `lints/README.md`
- Modify: `.claude/rules/tech.md`, `.claude/rules/testing.md`, `.claude/rules/error-handling.md`, `.claude/rules/structure.md`, `CLAUDE.md`

**Interfaces:**
- Consumes: the spec's §6 coverage table (the trim map), the triage records from Tasks 1–5.
- Produces: documentation matching the shipped state.

- [ ] **Step 1: `lints/README.md`**

Sections (English): purpose; the nine lints with level + one-line behavior each (including the stock-21 list with the tag pin and any Task 1 triage drops); adoption snippet for other projects:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/Jazz-Man/async-language-server", tag = "v0.1.0", pattern = "lints" },
]
```

(with the note: replace `v0.1.0` with the tag you pin); per-lint config tables (`wire_boundary`, `no_sync_io`, `layer_boundaries` with the full scope/deny example); the portability rule (lints no-op without their target crates); first-run setup (`cargo install cargo-dylint dylint-link --locked`; the nightly toolchain downloads on first `cargo dylint`); VS Code/rust-analyzer `check.overrideCommand` snippet (dylint README's version); how to run the suite's own tests (`cd lints && cargo test`, `BLESS=1` to regenerate snapshots).

- [ ] **Step 2: Battery sync**

`.claude/rules/tech.md`: add `cargo dylint --all -- --all-targets` to the verification battery block (after the clippy line), with a one-line note: "nightly-2026-05-28 + rustc-dev download on first run; cached afterwards (dylint job caches ~/.dylint_drivers, toolchains)". In the same file, extend the lint-stack description (clippy primary; `lints/` suite for what clippy cannot express; arch-lint is gone — its layer rules live in `strict-lints.layer_boundaries`).

`CLAUDE.md`: one line in Commands: `cargo dylint --all -- --all-targets` — runs the strict-lints suite + stock dylint lints (nightly-pinned lint pass, separate from the stable battery).

- [ ] **Step 3: MD trim-to-pointers (spec D10)**

Working map: the spec's §6 table. For each covered duty, replace the sentence with a pointer; leave everything else word-for-word:
- `error-handling.md`: "Display messages start lowercase, carry no trailing punctuation" → pointer to `strict-lints.error_display_lowercase` (keep the "include the discriminating values" sentence — uncovered); the stringify sentence → pointer to `error_no_string_catch_all` keeping the uncovered remainder; the boundary section's construction ban → pointer to `wire_boundary` (keep the carve-out rationale — it is architecture context); `# Errors`/`# Panics` duty line → note "enforced by clippy pedantic" (keep the how-to-write guidance).
- `testing.md`: the sleeps sentence in Determinism → pointer to `no_sleep_in_tests`; the unwrap/expect-in-tests fact → "clippy `unwrap_used`/`expect_used` with clippy.toml allowances".
- `structure.md`: the layer bullet list stays (architecture context for agents); add one line: "the dependency directions are enforced by `strict-lints.layer_boundaries` (dylint.toml carries the scopes)".
- `tech.md`: covered in Step 2.
Rule: a sentence is trimmed only where the lint's coverage truly subsumes it; partial coverage keeps the uncovered remainder.

- [ ] **Step 4: Verify docs compile claims**

Run: `cd lints && cargo test` and from root `cargo dylint --all -- --all-targets` once more.
Expected: green (the README's commands are real commands from this tree).

- [ ] **Step 5: Checkpoint**

File group: `lints/README.md`, `.claude/rules/{tech,testing,error-handling,structure}.md`, `CLAUDE.md`. Owner commits.

---

### Task 9: Final battery + acceptance audit

**Files:** none created; audit only (fixes land where the audit finds gaps).

**Interfaces:** Consumes the full cycle.

- [ ] **Step 1: Full battery, both feature configurations**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo dylint --all -- --all-targets
cd lints && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: every line green.

- [ ] **Step 2: Acceptance audit against the spec**

Verify each item, by command:
1. `cd lints && cargo dylint list --path .` — nine lints listed.
2. `ls lints/ui lints/ui_test_target` — fixtures exist for every lint, positive and negative.
3. `grep -rn 'allow(' src examples` — empty.
4. `grep -rn 'arch-lint\|arch_lint' . --exclude-dir=target --exclude-dir=.git` — only historical mentions under `docs/superpowers/` remain.
5. `grep -c 'transparent' src/error.rs` etc. — spot-check that the deny lints genuinely evaluate this repo (the Task 2/3/5 repo runs are the evidence).
6. The adoption snippet in `lints/README.md` parses as TOML and points at `pattern = "lints"`.
7. Root `Cargo.toml` diff vs the branch point: exactly the two additive entries + the `let_underscore_must_use` line + arch-lint removals.

Expected: all pass; any gap goes back to the owning task's files and is fixed before the checkpoint.

- [ ] **Step 3: Checkpoint**

File group: whatever the audit required (ideally empty). Owner commits; on the last task, continue straight to the whole-branch review per the house workflow.

---

## Self-Review Notes

- Spec coverage: D1 (reversal recorded — spec only), D2/D3 (Tasks 1–5), D4 (Tasks 4, 6), D5 (Task 1 pins), D6 (levels in each lint; zero-allow in constraints + audits), D7 (Tasks 1, 3, 4 config), D8 (Tasks 7, 8), D9 (ui fixtures Tasks 2–5; suite self-check Task 7), D10 (Task 8). Stock 21 → Task 1; error-pass extensions (transparent+from, From-impl) → Tasks 2, 3. Out-of-scope items have no tasks, correctly.
- Known adaptation points (honest, not placeholders): the exact `dylint_testing::ui::Test` API, the `dylint_linting` config-module entry point, and rustc-internal attribute/path APIs are pinned by the locked toolchain + crate versions and verified against the vendored sources at implementation time; the detection algorithms, fixtures, config contracts, and levels are fully specified above.
- No git commands anywhere: checkpoints replace commits; the owner drives git.
