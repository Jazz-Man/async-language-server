# Single source of verification commands — the contract referenced by CLAUDE.md
# and .claude/rules/tech.md. Edit freely; keep targets .PHONY and bodies plain.

# Route cargo through rtk locally (token-optimized output, transparent
# pass-through for commands rtk does not filter); plain cargo under CI, where
# CI=true is set by the runner and rtk is not installed.
ifeq ($(CI),)
CARGO_BIN := rtk cargo
else
CARGO_BIN := cargo
endif

# Toolchains (dated pins; upgrades are deliberate, never casual).
MIRI_TOOLCHAIN ?= nightly-2026-09-08

TARGET := x86_64-apple-darwin

.DEFAULT_GOAL := help
.PHONY: help fmt fmt-fix clippy doc test test-no-default-features dylint battery \
        dupes deny bench mutants miri miri-setup

## help: list available targets
help:
	@rtk grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## //'

## fmt: check formatting (gate)
fmt:
	@$(CARGO_BIN) fmt --check

## fmt-fix: apply formatting
fmt-fix:
	@$(CARGO_BIN) fmt

## clippy: lint every target, warnings are errors (gate)
clippy:
	@$(CARGO_BIN) clippy --workspace --all-targets -- -D warnings

## doc: build docs, doc warnings are errors (gate)
doc:
	@RUSTDOCFLAGS="--enable-index-page -Zunstable-options -D warnings" $(CARGO_BIN) +nightly doc --workspace --no-deps

## test: all-features leg — nextest binaries, then doctests (gate)
test:
	@$(CARGO_BIN) nextest run --workspace --all-features
	@$(CARGO_BIN) test --doc --workspace --all-features

## test-no-default-features: no-default-features leg — nextest, then doctests (gate)
test-no-default-features:
	@$(CARGO_BIN) nextest run --workspace --no-default-features
	@$(CARGO_BIN) test --doc --workspace --no-default-features

## dylint: external lint suites via their pinned nightlies (gate)
dylint:
	@$(CARGO_BIN) dylint --all -- --all-targets

## battery: the full pre-done gate (CI parity)
battery: fmt clippy doc test test-no-default-features dylint

## dupes: duplication gate (on demand)
dupes:
	@$(CARGO_BIN) dupes check

## deny: dependency policies — advisories, duplicate bans, licenses, sources (on demand; deny-level findings gate)
deny:
	@$(CARGO_BIN) deny check

## bench: oneshot diagnostics benchmark (on demand)
bench:
	@$(CARGO_BIN) bench --bench oneshot_diagnostics

## mutants: mutation-testing sweep (on demand, heavy; run it alone — a concurrent build poisons its auto-derived per-scenario timeout). Optional FILE=src/foo.rs scopes the sweep to one file. Exit code 2 means survivors were found: this is a diagnostic sweep, not a gate — the battery never runs it.
mutants:
	@$(CARGO_BIN) mutants $(if $(FILE),-f $(FILE))

## miri: UB interpreter over the no-default-features leg, cross-interpreted for
## x86_64 (the ropey/str_indices NEON path is not interpretable on an aarch64 host);
## on demand, slow — never in the battery
# architecture_rules_hold is excluded by name: its workspace-wide fs walk does
# not terminate under the interpreter (observed >1h) — infeasible, not failing.
miri:
	@$(CARGO_BIN) +$(MIRI_TOOLCHAIN) miri nextest run --target $(TARGET) --no-default-features -E 'not(test(architecture_rules_hold))'

## miri-setup: one-time toolchain preparation for make miri
miri-setup:
	@rustup component add miri --toolchain $(MIRI_TOOLCHAIN)
	@rustup target add $(TARGET) --toolchain $(MIRI_TOOLCHAIN)
	@$(CARGO_BIN) +$(MIRI_TOOLCHAIN) miri setup --target $(TARGET)
