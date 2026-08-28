# Technology

## Toolchain

Rust, edition 2024, MSRV 1.88 (`rust-version` in `Cargo.toml`). The toolchain
is pinned by `rust-toolchain.toml` to the `stable` channel with `rustfmt` and
`clippy` components — do not bypass the pin with `rustup run` or `+nightly`.
Edition 2024 means let-chains (`if let ... && ...`) are used freely; prefer
them over nested matches.

## Feature gates

Two features, both on by default (`[features]` in `Cargo.toml`):

- `tracing` — adds `TracingLayer` to the middleware stack in `src/serve.rs`
  and `debug!`/`info!` calls in handlers.
- `tree-sitter` — adds `tree_sitter_utils`, the grammar field on
  `DocumentMatcher`, and syntax-tree access on `Document`.

Code under `#[cfg(feature = "tree-sitter")]` must also compile without it.
When a change touches a gated path, verify at least
`cargo test --no-default-features --features tree-sitter` in addition to the
default configuration.

## Verification battery

Before considering work done, run the same battery CI runs
(`.github/workflows/rust.yml`, on push/PR to `main`):

```bash
cargo build --all-targets
cargo test                          # default features
cargo test --no-default-features
cargo test --all-features
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

`cargo test <test_name>` runs a single test. A failing check is a signal
about the code, not about the check: when anything fails, follow the global
`no-workarounds` and `superpowers:systematic-debugging` process — invoke both
skills, find the root cause, then fix it. Never make a check pass with
`#[allow]`, `--cap-lints`, or similar suppression.

## Lints

Lint levels live in `Cargo.toml`, not in source attributes:

- `[lints.clippy]`: `all` is `deny`; `cargo` and `pedantic` are `warn`, with
  a short inherited allow list (`module_inception`,
  `module_name_repetitions`, `multiple_crate_versions`, `similar_names`,
  `unnecessary_wraps`).
- `[lints.rust]`: `missing_docs = "warn"`.

Write code that passes at these levels. The allow entries are inherited from
upstream and count as debt: do not add new entries, and treat removing one as
its own deliberate task once the code it covers is fixed — not a drive-by
edit inside unrelated work. Nothing here relaxes the global `no-workarounds`
rule; suppression is not a way forward in this fork.

## Documentation

Public items need `///` docs (`missing_docs` is enforced):

- `# Errors` on every fallible public function.
- `# Panics` wherever a panic path exists.
- `# Examples` doctests on doctest-friendly API.

Doctests run in all three feature configurations. Keep them free of
tree-sitter-gated API so they compile under `--no-default-features`, and use
`no_run` fences for anything that opens a transport (see `serve` in
`src/serve.rs`).

## Tests

Tests live inline as `#[cfg(test)] mod tests` at the bottom of each `src/`
module — not in a separate tests directory. They create real temporary
workspaces on disk with millisecond-unique names under `std::env::temp_dir()`;
follow the pattern in the `server_with_state.rs` tests.

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
