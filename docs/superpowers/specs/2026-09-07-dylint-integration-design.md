# DyLint integration — `strict-lints` suite — design

**Date:** 2026-09-07
**Status:** approved design, pre-implementation
**Inputs:** owner directives 2026-09-07 (full DyLint integration with custom linters portable across the owner's Rust projects; arch-lint replaced within this cycle; stock width 21 curated; dylint runs in CI and the local battery), `docs/superpowers/research/2026-08-30-lint-toolchain-research.md` (its dylint verdict is reversed here — see Provenance), `.claude/rules/{error-handling,testing,tech,structure}.md`, dylint documentation at master `89aa879` (README, `docs/how_dylint_works.md`, `examples/README.md`, `examples/general/{Cargo.toml,rust-toolchain.toml}` — fetched 2026-09-07)
**Branch:** `feature/DyLint`

## Goal

One lint stack for this crate and the owner's other Rust projects: clippy (primary strictness lever, unchanged) plus a portable dylint suite `strict-lints` carrying the rules neither clippy nor prose review can hold mechanically — the error-model boundary, error-enum discipline, layer dependencies, sync-IO and test-determinism invariants. arch-lint, the riskiest dependency in the current stack, is fully retired into the suite. Everything a new project needs to adopt the same strictness is three lines of dylint config.

## Decisions

- **D1 — dylint adopted; the 2026-08-30 "Out" verdict is reversed.** That research rejected dylint for this crate alone (custom lints against unstable rustc internals; lint libraries pin nightly toolchains). Two things changed: the portability requirement — the suite amortizes over the owner's projects, not one crate — and the owner's explicit acceptance of the nightly coupling as a **separate lint pass that never touches the stable verification battery** (fmt/clippy/test/doc stay on pinned stable).
- **D2 — placement: `lints/` inside this repo.** An isolated cargo workspace (not a member of the root workspace) holding the suite crate. This repo consumes it by path; any other project consumes it by `{ git = "…/async-language-server", tag = "vX", pattern = "lints" }`. Extracting to a dedicated repo later is a config-only change for consumers.
- **D3 — the suite is nine custom lints plus 21 stock Trail of Bits lints.** Custom (table in §2); stock: all 8 `examples/general`, all 10 `examples/supplementary`, and 3 curated from `examples/restriction` (`assert_eq_arg_misordering`, `collapsible_unwrap`, `misleading_variable_name`), pinned by tag. Fires are triaged empirically at plan time — the adoption pattern of the 2026-08-30 clippy restriction block: fix or drop, never loosen.
- **D4 — arch-lint is fully retired in this cycle.** Parity mapping in §5. The layers engine ports to `layer_boundaries`; `NoSyncIo` ports to `no_sync_io`; `RequireThiserror` ports into the error pass; `NoSilentResultDrop` moves to clippy (`let_underscore_must_use`); AL003/AL006/AL007 are dropped with recorded rationale.
- **D5 — toolchain: `lints/rust-toolchain.toml` pins `nightly-2026-05-28`** with `rustc-dev` + `llvm-tools-preview` — the pin inside dylint tag `v6.0.4`'s `examples/general/rust-toolchain.toml` (master pins a newer nightly; the tag governs, per verification item 2) — both suites then share one driver toolchain in CI. dylint checks this crate with the library's toolchain (verified in `docs/how_dylint_works.md`), so the pinned stable battery is untouched; the pin upgrades only deliberately, together with the ToB tag.
- **D6 — levels land measured, suppressions stay unnecessary.** Deny lints arrive at zero measured fires on the current tree; warn lints (`bool_params_public`, `panics_doc_debt`) are advisory and non-gating, so no `#[allow]`/`cfg_attr` ever appears in `src/` and the zero-allow invariant keeps holding unmodified.
- **D7 — configuration: root `dylint.toml`**, not `[workspace.metadata.dylint]` inside `Cargo.toml` — keeps the main manifest clean and groups every dylint concern (library discovery + per-library config) in one file. The root `Cargo.toml` gains only the ToB-recommended `unexpected_cfgs` `check-cfg` entry (without it, dylint's `--cfg=dylint_lib` injection warns on every run) and the clippy `let_underscore_must_use` restriction lint.
- **D8 — CI and battery.** A new `dylint` job in `ci.yml` (parallel to `checks`/`test`): install `cargo-dylint` + `dylint-link`, run `cargo dylint --all`, with the caches dylint's README recommends (`~/.cargo/bin`, `~/.dylint_drivers`, `~/.rustup/toolchains`, `target/dylint`). `tech.md`'s battery gains `cargo dylint --all`; `CLAUDE.md` gains one command line.
- **D9 — the suite tests itself the standard way.** `ui_test` fixtures (`.rs` + `.stderr` snapshots, positive and negative cases) per lint; the integration gate is `cargo dylint --all` green on this repo. The suite's own fmt + clippy run in the dylint job, mirroring the ToB examples' own lint setup.
- **D10 — the MD rules slim to pointers where enforcement moved into lints.** Final task of the cycle, after the suite is green: in `.claude/rules/`, every sentence a lint (strict-lints or clippy) fully subsumes shrinks to a one-line pointer ("enforced by `strict-lints` / clippy"); partially covered sentences keep their uncovered remainder; uncovered text stays word-for-word. §6 is the audit map. The pointer form (not deletion) keeps the preventive half: rules files are read by agents *before* code is written, lints fire *after*.

## Design

### 1. `lints/` workspace layout

```
lints/                          # isolated cargo workspace; NOT a root member
  Cargo.toml                    # cdylib "strict-lints"; dylint-linting; clippy_utils (pinned git rev)
  rust-toolchain.toml           # nightly-2026-07-09 + [rustc-dev, llvm-tools-preview]
  .cargo/config.toml            # linker = dylint-link (the ToB examples' pattern)
  src/lib.rs                    # dylint_library! + register_lints (nine lints)
  src/wire_boundary.rs
  src/error_display.rs          # lowercase + punctuation + transparent/#[from] ban
  src/error_no_string.rs
  src/require_thiserror.rs
  src/no_sleep_in_tests.rs
  src/bool_params.rs
  src/panics_doc_debt.rs
  src/layer_boundaries.rs       # scopes + deny matrix, config-driven
  src/no_sync_io.rs
  tests/ui.rs                   # ui_test harness
  ui/                           # per-lint .rs + .stderr fixtures
  README.md                     # consumption snippet + per-lint docs (English)
```

One crate, one file per lint — the ToB examples split each lint into its own package only to demo independent toolchains; a single suite on one pinned nightly does not need that. Portability rule for every lint: it must no-op when its target symbols are absent (no async-lsp dependency → `wire_boundary` silent; no thiserror derives → the error pass silent), so the suite carries cleanly into non-LSP projects.

### 2. The nine custom lints

| Lint | Level | Fires on | Config (`dylint.toml`) |
|---|---|---|---|
| `wire_boundary` | deny | Construction of `async_lsp::ResponseError` (struct literal, associated-fn call) and `impl From<X> for ResponseError` outside the blessed adapter. Reads (e.g. oneshot's `From` in the other direction) are not flagged — the rule is about construction | `allowed_paths` (here: `src/server/with_state/**`, `src/workspace/diagnostics.rs`) |
| `error_display_lowercase` | deny | `#[error("…")]` whose first alpha character is uppercase, or whose literal ends in `.`/`!`/`?`; and `#[error(transparent)]` combined with `#[from]` — the exact pattern that forwards `source()` into an inner error with no source of its own (the rule's own empirical finding) | — |
| `error_no_string_catch_all` | deny | An error-enum variant whose only content is `String`/`&str` (the `Unknown(String)` shape that stringifies and loses the chain). Two-field variants like `Rpc { code, message: String }` pass — a wire message is data, not a stringified cause | — |
| `require_thiserror` | deny | An error-typed enum (ported semantics: enums named `*Error`) without `derive(thiserror::Error)` | name pattern (optional) |
| `no_sleep_in_tests` | deny | `std::thread::sleep` / `tokio::time::sleep` calls in test code (testing.md determinism: channel gates and bounded waits, never sleeps) | — |
| `no_sync_io` | deny | Blocking-IO call set (`std::fs::*`, `Path::exists`/`metadata`, sync `std::io` reads) outside blessed paths — port of AL002. Granularity delta, accepted: blessing is file-glob, slightly wider than arch-lint's per-function comment allows | `allowed_paths` |
| `layer_boundaries` | deny | Any path (use tree or inline qualified path) resolving from a file in scope A to a crate-local item in scope B where the pair is denied — port of `[[scopes]]` + the six `[[deny-scope-dep]]` rules, resolution via def-ids rather than syn import text | `scopes` (name → path globs), `deny` (from → to list + message) |
| `bool_params_public` | warn | `bool` parameters on public functions (boolean blindness; candidates for enum/newtype). Skips impls of foreign traits and test code. Warn-level: heuristic, possible false positives | — |
| `panics_doc_debt` | warn | Public functions whose docs carry a `# Panics` section — surfacing each documented panic contract as type-first debt, not a gate | — |

### 3. Stock adoption (21, pinned)

`examples/general` (8) + `examples/supplementary` (10) + the three restriction lints, loaded by pattern from the pinned tag. Triage at plan time: every fire is fixed or the lint dropped with a recorded reason; thresholds are never loosened to hide a fire.

### 4. Root configuration

```toml
# dylint.toml (crate root)
[workspace.metadata.dylint]
libraries = [
    { path = "lints" },
    { git = "https://github.com/trailofbits/dylint", tag = "<verified tag>", pattern = [
        "examples/general",
        "examples/supplementary",
        "examples/restriction/assert_eq_arg_misordering",
        "examples/restriction/collapsible_unwrap",
        "examples/restriction/misleading_variable_name",
    ] },
]

[strict-lints.wire_boundary]
allowed_paths = ["src/server/with_state/**", "src/workspace/diagnostics.rs"]

[strict-lints.no_sync_io]
allowed_paths = ["src/server/state/documents.rs", "src/server/state/workspace.rs", "src/workspace/walker.rs", "benches/**"]

[strict-lints.layer_boundaries]
# scopes + deny matrix ported from arch-lint.toml (same seven scopes, same six rules)
```

Root `Cargo.toml` additions: the `unexpected_cfgs` `check-cfg` entry under `[workspace.lints.rust]`; `let_underscore_must_use` under the restriction block. The consumption snippet other projects use is documented in `lints/README.md`:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/Jazz-Man/async-language-server", tag = "vX.Y.Z", pattern = "lints" },
]
```

### 5. arch-lint retirement (parity map)

| Today (`tests/architecture.rs` + `arch-lint.toml`) | Destination |
|---|---|
| `[[scopes]]` + 6× `[[deny-scope-dep]]` | `layer_boundaries` (config-driven, def-id resolution) |
| `NoSyncIo` (AL002) | `no_sync_io` (file-glob blessing — the six per-function comment allows collapse into `allowed_paths`; slightly wider, accepted) |
| `RequireThiserror` (AL005) | `require_thiserror` |
| `NoSilentResultDrop` | clippy `let_underscore_must_use` in `[lints]` |
| `NoErrorSwallowing` (AL003) | **dropped** — its semantics ("only logging") cannot be told apart from the trace-and-continue pattern error-handling.md explicitly blesses for streams of fallible entries; the axis stays with the MD rule and review |
| `RequireTracing` (AL006) | **dropped** — zero possible fires: the `log` crate is not a dependency; trivial to add later if ever needed |
| `TracingEnvInit` (AL007) | **dropped** — zero fires: a library does not initialize subscribers; downstream concern |
| `no-unwrap-expect`, `handler-complexity` | already excluded today; clippy owns both (2026-08-30 research) |

Removed with it: `arch-lint.toml`, `tests/architecture.rs`, the `arch-lint = "=0.5.0"` workspace/dev dependency (Cargo.lock updates), and the inert `// arch-lint: allow(…)` comments in `src/` and `benches/`. Consequence, recorded plainly: layer and sync-IO checking moves from stable `cargo test` into the nightly dylint pass — battery-level parity holds because dylint is in the battery, and `cargo test` gets faster (the whole-repo syn analysis disappears).

### 6. What this mechanizes from the MD rules (honest boundary)

Covered by the suite: the ResponseError construction boundary (error-handling.md "one boundary"), the stringify-catch-all shape, Display casing/punctuation, transparent+`#[from]`, thiserror derive, sync-IO discipline, layer dependencies (structure.md), sleep-free deterministic tests (testing.md). Covered by clippy after this cycle: dropped Results (`let_underscore_must_use`) alongside the existing unwrap/expect and `# Errors`/`# Panics` doc lints. Not mechanically checkable and left in the MD rules deliberately: whether a variant preserves its `#[source]` when wrapping, whether a `#[from]` conversion loses context, whether a call is fire-and-forget that must trace, whether untrusted values flow through fallible paths, whether Display carries discriminating values, `anyhow` absence (a dependency-policy concern, cargo-deny territory if ever), and the single-enum/leaf-error organization.

### 7. CI and battery

`ci.yml` gains a `dylint` job (checkout → rust-setup → `cargo install cargo-dylint dylint-link` → `cargo dylint --all`) with the README-recommended caches; `checks`, `test`, `release`, `pages` are untouched. `tech.md` battery list gains `cargo dylint --all` with a note about the one-time nightly setup; `CLAUDE.md` Commands gains the same line. Nothing in the stable battery changes.

### 8. Testing the suite

`ui_test` fixtures per lint: at minimum one file that fires and one that stays silent, each with a `.stderr` snapshot. The suite's own code quality: `cargo fmt --check` + clippy (nursery/pedantic warn, mirroring the ToB examples) inside the dylint job. Integration acceptance: `cargo dylint --all` exits green on this repository in both CI and the local battery.

### 9. Documentation sync

`lints/README.md` (consumption snippet, per-lint docs, levels, the portability rule); `tech.md` battery + a lint-stack paragraph (clippy primary, dylint suite for what clippy cannot express, arch-lint gone); `CLAUDE.md` one command line. The prose rules in `.claude/rules/` stay as files — they serve agents and carry the axes no lint can check — but per D10 they are slimmed: covered duties become pointers, uncovered text remains. The trim respects granularity: a sentence is trimmed only where the lint's coverage truly subsumes it (e.g. the Display casing sentence → pointer; the "preserve `#[source]` chains" sentence keeps its text because `error_no_string_catch_all` covers only the catch-all shape, and `no_sync_io`'s file-glob blessing is wider than the per-function wording it replaces). The spec's §6 table is the recorded boundary between the machine layer and the prose layer.

### 10. Open verification items (plan-time, before or during the first task)

1. Whether `cargo dylint` installs a missing pinned toolchain itself or fails — decides whether the CI job needs an explicit `rustup toolchain install nightly-2026-07-09 -c rustc-dev,llvm-tools-preview` step, and what the battery note in `tech.md` says about first-run setup.
2. The exact dylint release tag to pin (v6.0.4 is the current crates.io release per the 2026-08-30 research; confirm the tag exists on GitHub) and the `rust-toolchain.toml` pin inside that tag's `examples/general` — our `lints/` pin matches it (D5); if the tag pins a different nightly than master's `nightly-2026-07-09`, follow the tag.
3. `dylint-linting` and the `ui_test` utility: published on crates.io or consumed as git dependencies (the ToB examples use path deps into their monorepo).
4. The `dylint.toml` layout when metadata and per-library config coexist in one file (§4 assumes the README's `[workspace.metadata.dylint]` table plus sibling `[strict-lints.*]` tables).
5. Whether `examples/supplementary` is one umbrella package like `examples/general` (affects only the pattern string).
6. The exact clippy lint covering `let _ =` on must-use values (`let_underscore_must_use` per current clippy docs — confirm at the pinned 1.98) and whether it is restriction-group.
7. Gating semantics for deny: our lints register `Deny` themselves so a plain `cargo dylint --all` gates; confirm no extra flags are needed.

## Out of scope

- **cargo-deny** (advisories/bans/licenses/sources) — the one applicable security axis; registered as a deliberate follow-up cycle, per the still-unlanded step 5 of the 2026-08-30 research. dylint ships no stock security suite (verified against its `examples/` catalog; Trail of Bits' own Testing Handbook recommends Clippy favorites plus audit-specific custom lints), so no security lints join this cycle.
- Migrating clippy-expressible rules into the suite (the division of labor is fixed: per-project clippy config, dylint only beyond clippy).
- `cargo modules` / `cargo machete` CI additions — the rest of the unlanded 2026-08-30 step 5.
- The declarative `[[restrict-use]]`/`[[require-use]]` arch-lint forms (unused in `arch-lint.toml` today); `layer_boundaries` is designed so they can be added later.
- The `clippy::panic`/`clippy::unreachable` adoptions — conscious follow-ups from the 2026-08-30 research, unrelated to this cycle.
- VS Code/rust-analyzer integration — documented as a snippet in `lints/README.md` only.
- `rust-review` (Trail of Bits' Claude Code security-review plugin) — the owner's personal tooling decision, outside project scope.

## Acceptance

- `cargo dylint --all` green locally and in the new CI job.
- Nine custom lints, each with ui fixtures (positive + negative) and a no-op path for projects lacking the target symbols.
- Stock 21 triaged with every fire fixed or its lint dropped and the reason recorded.
- arch-lint fully gone (files, dev-dep, inert comments) with `cargo test --workspace` green without it.
- `grep -rn 'allow(' src examples` still returns nothing; no `cfg_attr` dylint suppressions anywhere in `src/`.
- Battery and docs synced (`tech.md`, `CLAUDE.md`, `lints/README.md`); the stable battery configuration is byte-identical apart from the two additive Cargo.toml entries (check-cfg, one restriction lint).
- `.claude/rules/` slimmed per D10: every lint-covered duty carries a pointer, every uncovered sentence is unchanged — audited against the §6 table.
- Full battery green in both feature configurations, plus the dylint pass.

## Provenance

This spec reverses the dylint verdict of `docs/superpowers/research/2026-08-30-lint-toolchain-research.md` (§1 comparison table: "Out: writing a boundary lint here is effort comparable to a custom psalm plugin, and lint libraries pin nightly rustc internals"). The reversal is deliberate and scoped: that research optimized for this crate alone under a stable-only battery; the portability requirement across the owner's projects amortizes the lint-writing effort, and the nightly coupling is accepted as a lint pass explicitly separated from the stable battery. The research's other holdings stand unchanged: clippy stays the primary strictness lever, arch-lint's layer engine had no stable alternative (which is why it survives here as `layer_boundaries`), and no baseline mechanism exists in this landscape (the adopted equivalent remains self-verifying `#[expect(..., reason)]` — unused by this design, which needs no suppressions at all).

Owner decisions 2026-09-07 (in session): placement in-repo `lints/` (vs a dedicated repo); the six initial custom lints; all three type-first proxies added; both error-handling extensions (transparent+`#[from]`, `From`-impl boundary) added; CI + battery wiring (not on-demand); stock width 21 curated; arch-lint replaced within v1 rather than deferred; the rules files slimmed to pointers in the cycle's final task (D10); cargo-deny registered as a follow-up cycle, not folded in (dylint verified to carry no stock security lints — `examples/` catalog plus the Trail of Bits Testing Handbook blog post of 2026-07-13).
