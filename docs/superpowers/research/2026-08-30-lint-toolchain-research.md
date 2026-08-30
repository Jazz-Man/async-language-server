# Lint toolchain research — decision document

Date: 2026-08-30. Repository state examined: `feature/abstraction` working tree
(`Cargo.toml` with arch-lint 0.5.0 dev-dep and the `[lints]` sections, the
commented-out check in `tests/architecture.rs`, `arch-lint.toml` with
`preset = "strict"`, `.github/workflows/rust.yml`, `rust-toolchain.toml`
pinning stable — currently rustc/clippy 1.98.0).

Question: which static-strictness tools belong in this crate's terminal + CI
battery, and what happens to the incumbent `arch-lint` 0.5.0 (currently
disabled pending config work, per its own doc comment and the owner's plan to
re-enable after the structure cycle).

Method: four research reports (R1 arch-lint assessment, R2 alternatives, R3
clippy deepening, R4 baselines and adjacent tools), 150 claims total. Claim
verdicts: 104 confirmed, 7 refuted (corrected in this document), 38 left
unverified (the verifier hit its capacity cap; those facts appear below only
with an explicit "unverified" label), 1 with no verdict recorded. The
synthesizer additionally verified this session: the arch-lint README's
declarative TOML syntax (field names quoted below), rustc's lint-levels
documentation for `#[expect]`/`unfulfilled_lint_expectations`/`reason`, the
cargo-machete crates.io record, and the repository files listed in Sources.

## 1. Comparison table

| Tool | Category | What it enforces | Maturity signals (verified 2026-08-30) | Config format | Baseline support | CI mode | Verdict for this crate |
|---|---|---|---|---|---|---|---|
| **arch-lint 0.5.0** (incumbent) | Architecture linter, syn AST | 8 built-in rules per preset (strict = AL001/002/003/004/005/006/007/013) plus declarative `[[scopes]]` / `[[deny-scope-dep]]` / `[[restrict-use]]` / `[[require-use]]` layer rules | Solo project (ynishi). 5 stars, 0 forks, 0 issues ever, 0 releases; 250 crates.io downloads, 1 reverse dep (author's own tool); 6-month commit gap 2026-02-20 → 2026-08-22; 0.5.0 released 2026-08-22 | `arch-lint.toml` (TOML). Documented surface partly dead through `check!()`: `[rules.*]` honored only for `enabled`/`severity`; `[analyzer] root` ignored; `exclude_files` does not exist; README's "strict = All rules" is false | None (zero occurrences of "baseline" in the repo) | `arch_lint::check!()` expands to a `#[test]`; rides existing `cargo test` | **Keep, narrowed**: `recommended` preset + declarative layer rules; version pinned. `strict` as currently configured yields 191 violations / 149 errors — mostly test-code `expect()` |
| **cargo-pup 0.1.8** | ArchUnit-style assertion tool | Module-path/import restrictions (`.*::api::.*` must not import `sqlx::*`), trait-implementor constraints, function-length rules | DataDog; 51 stars; created 2025-01-23; 0.1.8 published 2026-06-09; 9 versions, ~5.2k downloads | `pup.ron` (RON) or builder API via `cargo_pup_lint_config` dev-dep run in a plain `#[test]` | None documented (README + code search) | CLI exit code or the `#[test]` builder | **Runner-up — rejected on toolchain**: install requires nightly-2026-01-22 + rust-src/rustc-dev/llvm-tools-preview because it drives rustc internals; conflicts with the stable-pinned battery |
| **dylint 6.0.4** | Custom-lint framework | Nothing by itself; runs lint libraries you write against rustc internals (`LateLintPass`) | Trail of Bits; 645 stars; 91 versions; 6.0.4 published 2026-08-14 | `[workspace.metadata.dylint] libraries = [...]` or `dylint.toml` | None documented | `cargo dylint --all` | **Out**: writing a boundary lint here is effort comparable to a custom psalm plugin, and lint libraries pin nightly rustc internals (example suite pins nightly-2026-05-28) |
| **cargo-gears-lints 0.0.4** | dylint-based architecture suite (corrects R2's "no such suite exists") | Fixed dependency-direction rules (`no_serde_in_domain`, `dtos_not_referenced_outside_api`, `no_infra_in_domain`, ...) | Created 2026-05-22; v0.0.4 published 2026-08-18; 4 stars, ~2.1k downloads | None — encodes one project's conventions, not configurable general-purpose boundaries | None documented (unverified) | via dylint | **Out**: v0.0.x, project-specific rule set, inherits dylint's nightly coupling |
| **cargo-modules 0.27.0** | Structure visualizer with two enforceable checks | `dependencies --acyclic` (module-graph cycle detection with the cycle path in the error); `orphans --deny` (unlinked source files) | regexident; 1258 stars; maintained since 2016; 0.27.0 published 2026-08-03; pushed 2026-08-26. Declares `rust-version = 1.95.0` (installing toolchain is 1.98 — fine) | CLI flags only; no rule syntax, cannot express "A must not import B" | None | Two `cargo` commands, non-zero exit | **Add as complement**: cheap, stable, covers the DAG and orphan invariants; not a layer-rule engine |
| **clippy 1.98.0** (pinned stable) | General linter | 132 restriction lints (all allow-by-default), complexity thresholds, group levels | First-party, ships with the toolchain | `[lints]` in Cargo.toml + `clippy.toml` | None native; per-item `#[expect(lint)]` (self-verifying via `unfulfilled_lint_expectations`) and `#[allow(lint, reason = "...")]` — all lint attributes accept `reason` | Already in battery (`cargo clippy --all-targets -- -D warnings`) | **Primary strictness lever — expand** (section 4, step 1) |
| **cargo-deny 0.20.2** | Dependency-graph linter | Advisories, bans (deny/allow crates, duplicate versions), licenses, sources — crate-level, not intra-crate modules | EmbarkStudios; 2412 stars; 0.20.2 released 2026-07-09; first-party `cargo-deny-action` (190 stars) | `deny.toml`; allows carry reasons | n/a (different axis) | Action or `cargo deny check` | **Add**: dependency-hygiene axis, reason-carrying allows match the repo's suppression policy |
| cargo-workspace-lints 0.1.4 | Workspace lint inheritance | Only that every workspace package declares `lints.workspace = true` (details unverified — verifier cap) | 0 stars, single commit (unverified) | Cargo.toml | n/a | cargo command | **Out**: this repo is a single package, and it is not an architecture tool |
| rust-guardian 0.1.1 | Placeholder-code detection | TODO/unimplemented-style detection (details unverified — verifier cap) | 1 star, ~446 downloads (unverified) | unverified | n/a | unverified | **Out**: not credible as boundary enforcement |

Landscape note, corrected: an April 2025 (not May 2025) r/rust thread asked
for ArchUnit-style tools for Rust and found none ready-to-use; a 2019 thread
asking for a dependency-cruiser equivalent was actually answered with
cargo-modules, not "nothing exists". The defensible absence finding is the
researchers': no mature, general-purpose, stable-toolchain module-boundary
rule engine was found beyond arch-lint — an absence claim from the searches
performed, not an exhaustive proof.

## 2. Adjacent-tools triage

One line each; "unverified" marks release data the verifier could not check
(cap reached) — none of these facts gate the recommendation.

- **cargo-deny 0.20.2** — IN: advisories/bans/licenses/sources in one
  config-driven check with reason-carrying allows; terminal + first-party
  Action.
- **cargo-modules 0.27.0** — IN: `dependencies --acyclic` and
  `orphans --deny` enforce the structural half of the layer rules on stable,
  zero config.
- **cargo-machete 0.9.2** — IN: unused-dependency detection on stable
  (verified this session: 0.9.2 published 2026-04-15, bnjbvr/cargo-machete,
  ~2.76M downloads); directly useful because `multiple_crate_versions` sits
  on the inherited clippy allow list.
- **cargo-shear** — alternative to machete, not both (release data
  unverified).
- **cargo-audit** — OUT: fully overlapped by cargo-deny's advisories check;
  running both is redundant (version data unverified).
- **cargo-llvm-cov** — IN later, only if coverage is wanted; stable and works
  on macOS dev machines and Linux CI (v0.9.0, 2026-08-16 — unverified).
- **cargo-tarpaulin** — OUT: on macOS it degenerates to the LLVM
  instrumentation cargo-llvm-cov drives natively (version/engine data
  unverified).
- **cargo-semver-checks** — NOT YET: earns a release-time job when real
  versions start; today the crate is 0.0.0, `publish = false`, git-pinned
  (version data unverified).
- **cargo-udeps** — OUT: needs nightly to run (unverified), which the
  stable-pinned battery excludes.
- **Miri** — OUT: nightly component; `grep -rn unsafe src/` returns zero
  matches, so there is nothing for it to check (the zero-unsafe fact is
  verified; Miri's install requirements unverified).
- **loom** — OUT: model-checks code using loom replacement types; this
  crate's concurrency lives in DashMap/tokio, which loom cannot see
  (unverified).
- **cargo-fuzz** — OUT of the default battery: nightly + Unix + C++; a
  possible separate opt-in job for LSP-JSON parsing later (unverified).
- **reviewdog** — DEFERRED: the only maintained diff-filter path
  (`-filter-mode=added` + `-fail-level`), and it is a ratchet, not a psalm
  baseline; revisit only if PR-scoped gating is wanted (release data
  unverified).
- **clippy-sarif + GitHub code scanning** — OUT: gating would live in
  GitHub's Security tab, not the terminal (unverified).
- **cargo-workspace-lints, rust-guardian** — OUT: wrong problem, and their
  specifics are unverified.

## 3. Ranked recommendation

**#1 — Keep arch-lint, narrowed, and combine it with an expanded clippy and
two stable additions.** Division of labor:

1. **clippy takes over everything the built-in arch-lint rules did**, better:
   `expect_used`/`unwrap_used` are test-aware via clippy.toml
   (`allow-expect-in-tests`, `allow-unwrap-in-tests` — verified keys, default
   false), support `#[expect(..., reason)]` suppressions that flag themselves
   when stale, and enforcement already rides CI's
   `cargo clippy --all-targets -- -D warnings`.
2. **arch-lint keeps the one thing nothing else on stable can do**: the
   declarative scope/dependency engine (`[[scopes]]`, `[[deny-scope-dep]]`,
   `[[restrict-use]]`, `[[require-use]]`, per-item
   `#[arch_lint::allow(rule, reason = "...")]`), which machine-enforces the
   layer split currently written only in prose in
   `.claude/rules/structure.md`. Clippy cannot express it: its
   `disallowed_methods`/`disallowed_types` ban named items, not
   module-to-module relations, and it has no module-cycle lint (rust-clippy
   issue #5782, open since 2020, E-hard, no linked PR).
3. **Add `cargo modules dependencies --acyclic` (and `orphans --deny`)** for
   the DAG invariant, and **cargo-deny** for the dependency axis.

Why not the alternatives:

- **cargo-pup (runner-up)** — the only true ArchUnit analog found
  (module-path regex rules, severity per rule, a builder API that runs in a
  plain `#[test]` — the closest match to the owner's psalm-at-max workflow),
  Datadog-backed and actively released. Lost solely on the toolchain: it must
  compile the project with nightly-2026-01-22 because it drives rustc
  internals. Revisit if a separate nightly CI job ever becomes acceptable.
- **dylint + cargo-gears-lints** — dylint itself is excellent (645 stars,
  Trail of Bits, v6.0.4 this month) but is a framework, not a rule engine:
  boundary rules mean implementing `LateLintPass` against unstable rustc
  internals, with lint libraries pinned to nightly toolchains. cargo-gears
  (which corrects R2's refuted "no maintained dylint architecture suite"
  claim) proves the path works but encodes one project's conventions at
  v0.0.x.
- **clippy-only (drop arch-lint)** — tempting because AL001/AL002/AL003
  overlap heavily with clippy restriction lints already enforceable at
  `-D warnings`. Rejected because the unique value — path-based layer rules
  inside `cargo test`, with reason-carrying suppressions — has no clippy
  equivalent, and the owner's stated goal is that prose rules verify nothing.
- **cargo-modules as replacement** — it enforces exactly two things
  (acyclicity, orphans); there is no rule syntax for layer relations.

Accepted risks of keeping arch-lint: bus factor of one, 5 stars, 250
downloads, a demonstrated six-month dormancy, stale CHANGELOG/README, and no
baseline mechanism — every finding must be fixed or annotated before the gate
goes green. Mitigations: pin `=0.5.0` exactly (0.4→0.5 silently changed what
"strict" means; a `cargo update` must not shift preset semantics), configure
only the config surface verified to work, and treat the tool as
early-adopted: the false positives this repo hits (sync notification
handlers, `unwrap_or`) are unreported upstream issues.

## 4. Concrete next steps

Timing: after the structure cycle completes — `tests/architecture.rs`
already documents that plan, and the scope globs below should be validated
against the final tree.

### Step 1 — Cargo.toml `[lints]` and a new `clippy.toml`

Validated as a combined package on a copy of this repo: exit 0 with 14
warnings (the two cognitive-complexity trips and five `str_to_string` sites,
doubled by lib + lib-test targets).

```toml
# Cargo.toml — [lints] additions. Existing entries unchanged.
[lints.clippy]
all = { level = "deny", priority = -3 }
# warn -> deny: CI already promotes warns to errors via `-D warnings`
# (verified: exit 101 on a pedantic warning), so this makes LOCAL runs and
# the manifest explicit; CI outcomes do not change.
cargo = { level = "deny", priority = -2 }
pedantic = { level = "deny", priority = -1 }

# inherited allow list unchanged (module_inception, module_name_repetitions,
# multiple_crate_versions, similar_names, unnecessary_wraps)

# --- restriction lints, all measured at ZERO fires on this codebase ---
print_stdout = "warn"                # stdout IS the protocol channel over stdio transport
print_stderr = "warn"
dbg_macro = "warn"
todo = "warn"
unimplemented = "warn"
mem_forget = "warn"
exit = "warn"                        # a library must not kill the process
undocumented_unsafe_blocks = "warn"  # crate is 100% safe code today
multiple_unsafe_ops_per_block = "warn"
allow_attributes_without_reason = "warn"  # encodes the repo's suppression policy
unwrap_in_result = "warn"
panic_in_result_fn = "warn"
verbose_file_reads = "warn"
clone_on_ref_ptr = "warn"
integer_division = "warn"
error_impl_error = "warn"
self_named_module_files = "warn"     # locks in the mod.rs layout

# --- fires today, adopted as explicit work items ---
str_to_string = "warn"               # 5 quick fixes: documents/document.rs:201,202,
                                     # documents/matcher.rs:149,162, oneshot/server.rs:102
cognitive_complexity = "warn"        # threshold below; trips exactly 2 functions (both 13)

# --- takes over arch-lint AL001, test-aware via clippy.toml ---
expect_used = "deny"
unwrap_used = "deny"                 # production code is already unwrap-free (0 fires)

[lints.rust]
missing_docs = "deny"   # passes today in all three feature configs, docs included
unsafe_code = "deny"    # zero `unsafe` in src/ — locks in the safe-only invariant
```

```toml
# clippy.toml (new file, crate root)
# Verified keys (clippy_config/src/conf.rs @ rust-1.98.0).
allow-unwrap-in-tests = true    # default false
allow-expect-in-tests = true    # default false
cognitive-complexity-threshold = 12  # trips the two 13s in requests/conversion.rs:236,309
too-many-lines-threshold = 75        # current max: 75 (a test); longest prod fn is 71
too-many-arguments-threshold = 6     # exactly one 6-arg fn exists; stays legal
# type-complexity-threshold stays at the default 250 (nothing reaches even 50
# at the default; the threshold-50 sweep itself is unverified).
```

Choices and caveats, stated plainly:

- `cognitive-complexity-threshold = 12` is a two-item ratchet
  (`modify_outgoing_workspace_edit`, `modify_incoming_workspace_edit`, both
  13). Prefer 15 if zero-trip adoption matters first. Upstream explicitly
  keeps `cognitive_complexity` in restriction and disclaims it as a
  measurement tool ("the true Cognitive Complexity ... is not something we
  calculate"), recommending `excessive_nesting`/`too_many_lines` instead —
  so this is not the PHPMD cyclomatic number. `excessive_nesting` at
  threshold 5 (9 trips) is a follow-up candidate, not a same-change item;
  its sweep numbers are unverified.
- Known clippy.toml gotcha: cargo does not fingerprint `clippy.toml`
  (open rust-clippy issue #9928) — touch sources after editing it.
- Not adopted now, listed as conscious follow-ups: `clippy::panic` (1 fire,
  `src/text_utils/encoding.rs:70`, documented `# Panics` with a fallible
  sibling — converting it aligns with "no panics on external input" but is a
  real design change), `clippy::unreachable` (3 invariant-style fires:
  `serve.rs:20`, `conversions.rs:70`, `transport.rs:73`), and
  `tests_outside_test_module` (all 22 fires are the
  `#[cfg(all(test, feature = "tree-sitter"))]` gate in
  `src/text_utils/range_ext/tree_sitter_tests.rs`, which the lint's
  `is_cfg_test` does not recognize — a confirmed false-positive class; skip
  until upstream handles compound cfgs).
- Corrected counts from the probes: `unwrap_used` fires 25 times (not 21),
  all in test code; `expect_used` totals 130 fires (not 94), of which
  exactly 2 are production.

The two blessed expects (the same two arch-lint flagged) get
self-verifying suppressions; `reason` on lint attributes is rustc-documented
and displayed in the lint message:

```rust
// src/server/state/documents.rs:201
#[expect(clippy::expect_used,
    reason = "invariant: a document carrying a tree always has its parser")]
```

```rust
// src/text_utils/range_ext/lsp.rs:176
#[expect(clippy::expect_used,
    reason = "invariant: delim0 was confirmed present by the preceding search")]
```

### Step 2 — arch-lint.toml rewrite

```toml
# arch-lint configuration
# Real documentation: https://github.com/ynishi/arch-lint
# (the previous header pointed at github.com/example/arch-lint — a placeholder)
#
# Verified against the published 0.5.0 source; do NOT configure by README alone:
#   - "strict = All rules" is false: 8 rules, and it sets allow_in_tests(false),
#     flagging every ordinary test .expect() — 191 violations / 149 errors here.
#   - [rules.*] is honored ONLY for `enabled` and `severity`; behavior options
#     (allow_in_tests, allow_patterns, max_lines, ...) are inert through check!().
#   - [analyzer] root is ignored through check!() (the runner passes the project root).
#   - `exclude_files` does not exist. `respect_gitignore` is never consulted.

preset = "recommended"   # the preset whose 8 findings this repo can annotate away
fail_on = "error"

[analyzer]
exclude = ["**/target/**"]

# clippy owns unwrap/expect discipline (expect_used/unwrap_used = deny with
# allow-*-in-tests and #[expect(..., reason)]), which is test-aware; AL001
# through check!() is not, and its per-rule options cannot make it so.
[rules.no-unwrap-expect]
enabled = false

# --- Layer rules mirroring .claude/rules/structure.md -----------------------
# Draft: validate against the tree by running `cargo test` — the check's own
# report is the fastest way to tune scope membership and directions.
# (deny-scope-dep semantics: `from` must not import `to`.)

[[scopes]]
name = "text_utils"
paths = ["src/text_utils/**"]

[[scopes]]
name = "documents"
paths = ["src/documents/**"]

[[scopes]]
name = "workspace"
paths = ["src/workspace/**"]

[[scopes]]
name = "requests"
paths = ["src/requests/**"]

[[scopes]]
name = "server"        # user layer (Server trait, serve, options) + plumbing
paths = ["src/server/**"]

[[scopes]]
name = "oneshot"       # clientless entry point
paths = ["src/oneshot/**"]

# Dependencies point downward only; leaves never reach back up.
[[deny-scope-dep]]
from = "text_utils"
to = ["requests", "documents", "workspace", "server", "oneshot"]
message = "text_utils is a leaf; conversion machinery must not leak into it."

[[deny-scope-dep]]
from = "documents"
to = ["requests", "server", "oneshot"]
message = "documents must not depend on request handling or server plumbing."

[[deny-scope-dep]]
from = "workspace"
to = ["requests", "server", "oneshot"]
message = "workspace scanning stays below the request layer."

[[deny-scope-dep]]
from = "requests"
to = ["server", "oneshot"]
message = "conversion helpers must not import server plumbing; the wrapper calls them, not vice versa."

[[deny-scope-dep]]
from = "server"
to = ["oneshot"]
message = "server plumbing must not depend on the clientless oneshot entry point."
```

The `[[restrict-use]]` form (verified field names: `name`, `scope`, `deny`
globs, `message`, optional `check-inline = false`) is available for rules
like banning `tokio::fs` in notification-handler modules if that ever needs
encoding; `[[require-use]]` (`files`, `prefer`, `over`) suits
prefer-tracing-over-log style rules. Top-level modules (`src/lib.rs`,
`src/error.rs`, `src/transport.rs`, `src/tree_sitter_utils.rs`) sit outside
all scopes by design in this draft.

### Step 3 — annotate the 8 current findings (recommended preset)

Confirmed locations from the reproduced run (6 AL002 errors + 2 AL001
errors; the AL001 pair is handled by clippy in step 1 if
`no-unwrap-expect` is disabled as above — otherwise use
`#[allow(clippy::expect_used, reason = "...")]`, which arch-lint also
recognizes):

```rust
// src/server/state/documents.rs:111,232,273 and
// src/server/state/workspace.rs:112,151 — on each enclosing function:
#[arch_lint::allow(no_sync_io,
    reason = "LSP notification handlers must stay synchronous per the spec; \
              the reload-from-disk fallback deliberately uses std::fs")]
```

```rust
// src/workspace/walker.rs:76 (.is_file()):
#[arch_lint::allow(no_sync_io,
    reason = "workspace walking is a synchronous batch scan over the ignore crate")]
```

Line-comment form when a whole function is too wide:
`// arch-lint: allow(no-sync-io) reason="..."` (reason is required for
error-severity rules — omitting it only produces a Warning, so write it).

The 2 AL003 warnings (`src/workspace/diagnostics.rs:290,350`) sit on
trace-and-continue paths the error-handling rule explicitly blesses; they do
not fail at `fail_on = "error"`, but annotating them with that citation
keeps the report clean. The 40 AL013 warnings (`unwrap_or`/`ok`/`let _ =`)
are idiomatic usage; leave them as visible, non-gating signal rather than
disabling the rule — do not set `fail_on = "warning"`.

### Step 4 — re-enable and pin

```rust
// tests/architecture.rs
arch_lint::check!(preset = "recommended"); // macro arg wins over the TOML
```

```toml
# Cargo.toml [dev-dependencies] — pin exactly: 0.4 -> 0.5 silently changed
# what "strict" means; a cargo update must not shift preset semantics.
arch-lint = "=0.5.0"
```

### Step 5 — CI additions (.github/workflows/rust.yml)

```yaml
    - name: Install extra lint tools
      run: cargo install cargo-modules cargo-deny cargo-machete
    - name: Module graph acyclicity
      run: cargo modules dependencies --lib --acyclic
    - name: Orphaned source files
      run: cargo modules orphans --deny
    - name: Dependency hygiene (advisories, bans, licenses, sources)
      run: cargo deny check
    - name: Unused dependencies
      run: cargo machete
```

Notes: cargo-modules 0.27.0 declares `rust-version = 1.95.0`; the pinned
stable 1.98 satisfies it (it constrains the installing toolchain, not this
crate's MSRV 1.88). cargo-deny needs a committed `deny.toml`
(`cargo deny init`); its allows require reasons, matching the repo's
suppression policy. The arch-lint gate needs no CI step — it rides
`cargo test` in all three feature configurations.

### Step 6 — what this does not give you

No psalm/phpstan-style baseline exists anywhere in this landscape — not in
clippy (its usage doc sanctions "a generous sprinkling of `#[allow(..)]`s",
and an issue search finds no baseline feature request among 6 incidental
matches), not in arch-lint, cargo-pup, cargo-modules, or cargo-deny. The
Rust-native equivalent adopted here is per-item `#[expect(..., reason)]`
entries that flag themselves for deletion via
`unfulfilled_lint_expectations` when they go stale. If PR-scoped ratcheting
is ever wanted, reviewdog with `-filter-mode=added` + `-fail-level` is the
only maintained path (release data unverified).

## 5. Sources

URLs actually consulted (confirmed claims plus this session's fetches; repo
files read directly).

arch-lint:

- https://crates.io/api/v1/crates/arch-lint
- https://crates.io/api/v1/crates/arch-lint/versions
- https://crates.io/api/v1/crates/arch-lint/reverse_dependencies
- https://api.github.com/repos/ynishi/arch-lint
- https://github.com/ynishi/arch-lint/issues?q=
- https://github.com/ynishi/arch-lint/commits/main
- https://github.com/ynishi/arch-lint/blob/main/README.md (declarative TOML
  syntax, suppression forms, presets table)
- https://github.com/ynishi/arch-lint/blob/main/CHANGELOG.md
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint/src/runner.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-macros/src/lib.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-rules/src/presets.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-rules/src/no_unwrap_expect.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-rules/src/no_sync_io.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-core/src/analyzer.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-core/src/config.rs
- https://github.com/ynishi/arch-lint/blob/main/crates/arch-lint-core/src/context.rs
- https://github.com/ynishi/arch-lint/commit/00ea1f771dc4b11996f4d524a7111702b3e40ebc
- https://docs.rs/arch-lint/latest/arch_lint/

cargo-pup:

- https://github.com/DataDog/cargo-pup (README sections: Installation,
  Examples, How It Works, Step 5)
- https://api.github.com/repos/DataDog/cargo-pup
- https://crates.io/api/v1/crates/cargo-pup

dylint and cargo-gears:

- https://github.com/trailofbits/dylint (README: workspace metadata, Writing
  lints, Caching in CI, Conditional compilation)
- https://github.com/trailofbits/dylint/blob/master/examples/general/rust-toolchain.toml
- https://github.com/trailofbits/dylint/tree/master/examples/general
- https://crates.io/api/v1/crates/dylint
- https://api.github.com/repos/trailofbits/dylint
- https://github.com/constructorfabric/cargo-gears

cargo-modules:

- https://api.github.com/repos/regexident/cargo-modules
- https://crates.io/api/v1/crates/cargo-modules
- https://crates.io/api/v1/crates/cargo-modules/versions
- https://github.com/regexident/cargo-modules/blob/main/README.md

clippy / rustc:

- https://github.com/rust-lang/rust-clippy/blob/master/clippy_lints/src/deprecated_lints.rs
- https://github.com/rust-lang/rust-clippy/blob/rust-1.98.0/clippy_config/src/conf.rs
- https://rust-lang.github.io/rust-clippy/rust-1.98.0/index.html
  (cognitive_complexity, redundant_pub_crate, self_named_module_files,
  print_stdout, tests_outside_test_module articles; group counts)
- https://rust-lang.github.io/rust-clippy/master/book/src/usage.md
- https://github.com/rust-lang/rust-clippy/issues/5782
- https://github.com/rust-lang/rust-clippy/issues/6541
- https://github.com/rust-lang/rust-clippy/issues?q=baseline (corrected
  basis for the no-baseline-request conclusion)
- https://doc.rust-lang.org/rustc/lints/levels.html (expect level,
  unfulfilled_lint_expectations, `reason` on lint attributes)
- Local pinned toolchain: rustc/clippy 1.98.0 (88d9e12ae 2026-08-18)

cargo-deny / cargo-machete:

- https://github.com/EmbarkStudios/cargo-deny/releases
- https://api.github.com/repos/EmbarkStudios/cargo-deny
- https://github.com/EmbarkStudios/cargo-deny-action
- https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html
- https://crates.io/api/v1/crates/cargo-machete

This repository (read this session):

- /Users/vasilsokolik/www/async-language-server/Cargo.toml
- /Users/vasilsokolik/www/async-language-server/tests/architecture.rs
- /Users/vasilsokolik/www/async-language-server/arch-lint.toml
- /Users/vasilsokolik/www/async-language-server/rust-toolchain.toml
- /Users/vasilsokolik/www/async-language-server/.github/workflows/rust.yml
- src/ tree (documents, oneshot, requests, server/{state,with_state},
  text_utils, workspace; top-level error.rs, lib.rs, transport.rs,
  tree_sitter_utils.rs)
