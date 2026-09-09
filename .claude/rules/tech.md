# Technology

## Toolchain

Rust, edition 2024, MSRV 1.88 (`rust-version` in `Cargo.toml`). The toolchain
is pinned by `rust-toolchain.toml` to the `stable` channel with `rustfmt` and
`clippy` components — do not bypass the pin with `rustup run` or `+nightly`.
Edition 2024 means let-chains (`if let ... && ...`) are used freely; prefer
them over nested matches.

## Feature gates

One feature, on by default (`[features]` in `Cargo.toml`):

- `tree-sitter` — adds `tree_sitter_utils`, the grammar field on
  `DocumentMatcher`, and syntax-tree access on `Document`.

`tracing` is a permanent, non-optional dependency (owner decision 2026-09-05,
spec decision 10): no consumer disables it, and `TracingLayer` is always in
the middleware stack. Code under `#[cfg(feature = "tree-sitter")]` must also
compile without it; when a change touches the gated path, verify at least
`make test-no-default-features` in addition to `make test`.

## Verification battery

Before considering work done, run the same battery CI runs
(`.github/workflows/ci.yml`, on push/PR to `main` and via `workflow_dispatch`):

```bash
make battery
```

The mapping — which target runs which command; the only place raw commands
remain (`make help` is authoritative):

- `make fmt` — `cargo fmt --check`
- `make clippy` — `cargo clippy --workspace --all-targets -- -D warnings`
- `make doc` — `RUSTDOCFLAGS="--enable-index-page -Zunstable-options -D warnings" cargo +nightly doc --workspace --no-deps` (nightly with the pages-job flags: the target mirrors the deployed docs, deliberately diverging from the checks job's stable `cargo doc` build)
- `make test` — `cargo nextest run --workspace --all-features`, then `cargo test --doc --workspace --all-features`
- `make test-no-default-features` — `cargo nextest run --workspace --no-default-features`, then `cargo test --doc --workspace --no-default-features`
- `make dylint` — `cargo dylint --all -- --all-targets`

The dylint pass is a nightly-pinned lint pass (`nightly-2026-05-28`, shared
by both suites); the first run downloads the toolchain and builds the
drivers, cached afterwards via `~/.dylint_drivers`/`~/.rustup/toolchains`.

The standalone `cargo build --workspace --all-targets` step is gone:
nothing is published, and `clippy --all-targets` compiles every target.
The `default` configuration is currently identical to `--all-features`
(the crate has one feature, on by default) and returns as a third leg the
day a non-default feature lands.

`cargo nextest run <filter>` runs a single test. A failing check is a signal
about the code, not about the check: when anything fails, follow the global
`no-workarounds` and `superpowers:systematic-debugging` process — invoke both
skills, find the root cause, then fix it. Never make a check pass with
`#[allow]`, `--cap-lints`, or similar suppression.

Outside the per-task battery (and outside CI), `make dupes` runs on
demand or periodically: it guards against exact and near AST duplication
creeping back after the dupes refactor. `dupes.toml` sets the analysis knobs
(`min_nodes`, `max_exact_duplicates`, `max_near_duplicates`) and `.dupes-ignore.toml` carries one reasoned
entry per deliberate leftover — together they encode the invariants, so a
non-ignored group means new duplication, not a threshold to loosen.
Criterion benches run on demand (`make bench`),
not in CI: they exist for measuring the batch diagnostics pipeline when
working on it, not as a gate. Mutation testing runs on demand too
(`make mutants`, with `FILE=src/foo.rs` scoping the sweep to one file):
exit code 2 reports survivors — a diagnostic sweep, not a gate — and the
battery never runs it. Nothing heavy runs concurrently with
`make mutants`: its per-scenario timeout is auto-derived from the baseline
run, and a competing build poisons it.

## Lints

Lint levels live in `Cargo.toml`, not in source attributes:

- `[workspace.lints.clippy]`: `all`, `cargo`, and `pedantic` are `deny`, with
  a short inherited allow list (`module_inception`,
  `module_name_repetitions`, `multiple_crate_versions`, `similar_names`,
  `unnecessary_wraps`).
- `[workspace.lints.rust]`: `missing_docs = "deny"`.

Clippy remains the primary strictness lever. A nightly-pinned dylint pass
(`make dylint`) adds the stock Trail of Bits suites
(tag-pinned) and the third-party `perfectionist` style suite (rc-pinned);
`dylint.toml` carries the reasoned rule disables. arch-lint still owns the
layer rules and its own rule set inside `make test`. Perfectionist config
keys are silently ignored when names change: at every tag bump, verify the
`[perfectionist]` disables/ignores still name real rules.

Write code that passes at these levels. The allow entries are inherited from
upstream and count as debt: do not add new entries, and treat removing one as
its own deliberate task once the code it covers is fixed — not a drive-by
edit inside unrelated work. Nothing here relaxes the global `no-workarounds`
rule; suppression is not a way forward in this fork.

## Documentation

Public items need `///` docs (`missing_docs` is enforced):

- `# Examples` doctests on doctest-friendly API.

Error-documentation duties (`# Errors`, `# Panics` sections) are governed by
`error-handling.md`.

Doctests run in every feature configuration CI runs. Keep them free of
tree-sitter-gated API so they compile under `--no-default-features`, and use
`no_run` fences for anything that opens a transport (see `serve` in
`src/server/serve.rs`).

## Tests

Tests live inline as `#[cfg(test)] mod tests` at the bottom of each `src/`
module, or in a sibling `tests.rs` file for the larger modules
(`#[cfg(test)] mod tests;`) — not in a separate tests directory. They create
real temporary workspaces on disk with millisecond-unique names under
`std::env::temp_dir()`; follow the pattern in the
`src/server/with_state/tests.rs` tests.

## Dependencies

The load-bearing ones: `async-lsp` (the `LanguageServer` trait, `MainLoop`,
`ClientSocket`), `tower` (`ServiceBuilder` middleware stack), `ropey`
(document `Rope`), `dashmap` (interior-mutable state), `globset` + `ignore`
(matching and workspace walking), `tokio` (io-std/io-util/net/rt features).
`Cargo.lock` is committed. Dependabot handles routine weekly bumps; make
upgrades deliberately and re-run the battery. Do not upgrade `async-lsp`
casually — this crate tracks its trait surface closely.

---
_Use the pinned toolchain, respect both feature gates, and run the full
battery — the crate is verified exactly the way CI verifies it, and failures
are investigated, never suppressed._
