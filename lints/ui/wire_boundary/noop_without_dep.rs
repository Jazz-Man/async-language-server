// Proves: a fixture with no construction site stays silent under the lint.
// Does NOT prove the portability no-op (the crates-absence branch of the
// gate): as a strict-lints example target, async-lsp rides in through the
// shared dev-dependencies, so `async_lsp` is in the crate graph here and
// silence comes from having nothing to flag — the same mechanism as
// `ok_read`. A genuinely dep-less fixture crate is follow-up material.
fn main() {
    let response_error = 3;
    let _ = response_error;
}
