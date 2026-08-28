# Product

`async-language-server` is a Rust library crate (no binary target) that wraps
`async-lsp` for writing small language servers with less boilerplate: tokio
stdio/TCP transports, ropey-based incremental document sync, automatic
position-encoding negotiation (UTF-8/16/32), optional tree-sitter integration,
and workspace-wide diagnostics.

## What this repository is

This is the owner's **fork** of an upstream framework, not greenfield code.
It exists to serve the owner's downstream servers (for example, a Markdown
LSP built on this crate), with substantially improved code quality along the
way. Upstream patterns are a starting point, not an authority: inherited
quirks, suppressions, and shortcuts count as debt to pay down deliberately,
in their own changes, when the owner asks.

Global rules in `~/.claude/rules/` apply everywhere and take precedence over
anything written here; nothing in this file relaxes them.

## Distribution model

Version 0.0.0, `publish = false` in `Cargo.toml`, not intended for
crates.io. Consumers use it as a git dependency or a fork and pin revisions
or tags, so breaking changes to `async_language_server::server::*` land
directly on downstream servers — there is no semver safety net.

- Keep the public surface small: the `server` module (re-exports in
  `src/lib.rs`), `oneshot`, `text_utils`, `tree_sitter_utils`
  (feature-gated), and the `lsp_types` re-export at the crate root.
- When a change breaks that surface, say so in the commit message — a fork
  maintainer reads commit messages, not changelogs.
- Prefer additive changes (new `Server` methods with default implementations)
  over signature changes to existing ones.

## What belongs here

This crate is plumbing, not policy. Transports, document synchronization,
encoding conversion, matcher configuration, workspace scanning, and
tree-sitter bookkeeping belong here. Language-specific behavior — Markdown
link checking, JSON validation rules, per-language formatting rules — does
not; it belongs in downstream servers implementing the `Server` trait
(`src/server_trait.rs`). When a request only makes sense for one language,
raise that once, then build what was asked.

Add generality only when a downstream server needs it now, not in
anticipation of hypothetical users. The audience is the owner; "would a
stranger want this" is not a requirement here.

## Design center

Every capability must stay expressible as: implement `Server`, override a few
async methods, register capabilities and matchers, run `serve()`. When a new
feature complicates that path for a minimal server, redesign the feature, not
the example. `examples/minimal.rs` is the smallest path (diagnostics only);
`examples/tree_sitter.rs` is the full path with a grammar attached.

## Documentation stance

`README.md` is the crate-level documentation
(`#![doc = include_str!("../README.md")]` in `src/lib.rs`), so README edits
change the rendered docs — keep it in sync with actual behavior. All written
artifacts (docs, specs, code and doc comments, commit messages) are in
English.

---
_This rule anchors decisions to what this crate is: the owner's fork of a
framework, improved for their own language servers, consumed by git pin or
fork._
