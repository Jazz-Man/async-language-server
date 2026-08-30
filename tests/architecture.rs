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

use std::path::Path;

use arch_lint::{
    Analyzer, RuleBox, Severity,
    declarative::load_rules_from_toml,
    rules::{
        NoErrorSwallowing, NoSilentResultDrop, NoSyncIo, RequireThiserror, RequireTracing,
        TracingEnvInit,
    },
};

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
    let result = analyzer.analyze().expect("analysis completes");

    assert!(
        !result.has_violations_at(Severity::Error),
        "{}",
        result.format_test_report(Severity::Error)
    );
}
