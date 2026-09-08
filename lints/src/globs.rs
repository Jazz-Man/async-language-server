//! Shared helpers behind the configurable lints: compiling glob patterns
//! from the linted workspace's `dylint.toml` and matching source files
//! against them.
//!
//! Paths reach the matchers relative to the workspace root: cargo invokes
//! the driver from there and passes source paths relative to it. Absolute
//! paths fall back to stripping the current directory.

extern crate rustc_lint;
extern crate rustc_span;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rustc_lint::{LateContext, LintContext};
use rustc_span::{FileName, Span};

/// Compiles glob patterns into a glob set; empty input yields an empty set,
/// which matches nothing. `config_key` names the configured field for
/// diagnostics. `**` crosses `/`, and `*` does not: `literal_separator` is
/// on, so patterns like `src/module/**` stay within the directory.
///
/// # Panics
/// Panics when a pattern is not a valid glob — a malformed config must fail
/// loudly, not silently narrow the lint.
pub fn glob_set(config_key: &str, patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match GlobBuilder::new(pattern).literal_separator(true).build() {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(error) => panic!("invalid glob in `{config_key}`: {error}"),
        }
    }
    builder
        .build()
        .expect("valid glob set from validated globs")
}

/// Whether `file` is a real file whose workspace-relative path matches `set`.
pub fn matches_file(set: &GlobSet, file: &FileName) -> bool {
    if set.is_empty() {
        return false;
    }
    let FileName::Real(real) = file else {
        return false;
    };
    let Some(path) = real.local_path() else {
        return false;
    };
    set.is_match(workspace_relative(path))
}

/// Whether `span`'s source file matches `set`.
pub fn span_matches_file(cx: &LateContext<'_>, set: &GlobSet, span: Span) -> bool {
    matches_file(set, &cx.sess().source_map().span_to_filename(span))
}

fn workspace_relative(path: &std::path::Path) -> &std::path::Path {
    if path.is_absolute()
        && let Ok(cwd) = std::env::current_dir()
        && let Ok(relative) = path.strip_prefix(&cwd)
    {
        return relative;
    }
    path
}
