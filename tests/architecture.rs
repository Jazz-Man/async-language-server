//! Architecture checks via `arch-lint`, wired programmatically.
//!
//! The `check!()` macro path applies preset defaults only: behavior knobs
//! (complexity thresholds, `allow_in_tests`, ...) configured in `[rules.*]`
//! TOML sections are parsed but never consulted. Building the analyzer here
//! makes the wiring explicit and every knob real, keeps `arch-lint` a
//! dev-dependency, and still loads the declarative layer rules from
//! `arch-lint.toml` (scopes and `deny-scope-dep`).
//!
//! Rule choices:
//! - `no-unwrap-expect` stays off: clippy's `expect_used`/`unwrap_used`
//!   (deny, test-aware via clippy.toml) own that axis, with self-verifying
//!   `#[expect(..., reason)]` suppressions.
//! - `handler-complexity` is excluded: it only measures functions named
//!   `handle_*`/`process_*`/`on_*`/`update*`, while clippy's
//!   `cognitive_complexity` and `too_many_lines` thresholds (clippy.toml)
//!   already cover every function in the crate.
//! - Per-site suppressions use the comment form with a mandatory reason.

use arch_lint::declarative::load_rules_from_toml;
use arch_lint::rules::{
    NoErrorSwallowing, NoSilentResultDrop, NoSyncIo, RequireThiserror, RequireTracing,
    TracingEnvInit,
};
use arch_lint::{Analyzer, RuleBox, Severity};
use std::path::Path;

#[test]
fn architecture_rules_hold() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let rules: Vec<RuleBox> = vec![
        Box::new(NoSyncIo::new()),
        Box::new(NoErrorSwallowing::new()),
        Box::new(NoSilentResultDrop::new()),
        Box::new(RequireThiserror::new()),
        Box::new(RequireTracing::new()),
        Box::new(TracingEnvInit::new()),
    ];

    // Only build output is excluded: arch-lint scans `src/`, `examples/`,
    // `benches/`, `macros/`, and this file. The lint suites in `dylint.toml`
    // are external git dependencies — there is no nested workspace in this
    // tree for the scan to fence off.
    let mut builder = Analyzer::builder().root(root).exclude("**/target/**");
    for rule in rules {
        builder = builder.rule_box(rule);
    }

    // arch-lint: allow(no-sync-io) reason="the analyzer setup reads its own config synchronously; this is test code"
    let config = std::fs::read_to_string(root.join("arch-lint.toml"))
        .expect("arch-lint.toml is committed at the crate root");
    for rule in load_rules_from_toml(&config).expect("arch-lint.toml parses") {
        builder = builder.rule_box(rule);
    }

    let analyzer = builder.build().expect("analyzer builds");
    let analysis = analyzer.analyze().expect("analysis completes");

    assert!(
        !analysis.has_violations_at(Severity::Error),
        "{}",
        analysis.format_test_report(Severity::Error),
    );
}
