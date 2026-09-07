//! UI tests for the `strict-lints` suite.
//!
//! Each fixture under `ui/` is a standalone example crate compiled with the
//! library loaded by the dylint driver; the `.stderr` file next to a fixture
//! is its diagnostic snapshot. Fixtures that expect no diagnostics carry no
//! `.stderr`.
//!
//! Fixtures are registered as example targets (`[[example]]` in
//! `Cargo.toml`) because they depend on `thiserror`: `dylint_testing` can only
//! resolve fixture dependencies through cargo-built examples (`ui_test_examples`,
//! per its docs), not through plain `ui/` source files.
//!
//! Snapshot regeneration: `dylint_testing` 6.0.4 does not expose compiletest's
//! `--bless`, so `BLESS=1` has no effect here. Instead, run `cargo test`, find
//! the "Actual stderr saved to ..." path printed for each mismatching fixture,
//! and copy that file over the fixture's `.stderr` (delete the `.stderr` if a
//! fixture became clean).

#[test]
fn ui() {
    dylint_testing::ui_test_examples(env!("CARGO_PKG_NAME"));
}
