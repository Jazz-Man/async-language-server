# Testing implementation design

Date: 2026-08-31. Input: the approved research decision document
`docs/superpowers/research/2026-08-31-testing-strategy.md` (four researcher
reports, 10/10 claims adversarially confirmed) plus the owner's decisions
recorded in the 2026-08-31 brainstorming session. Owner-approved choices that
override research defaults are marked **[owner]**.

## Goal

Upgrade the crate's test infrastructure in four phases plus one documentation
phase: extract the requests test harness in-domain, convert `RangeExt`
panics into typed errors and tighten public signatures, build the wire-tier
integration client (W2 duplex + W3 TCP) with the full 15-test catalog, fill
the unit-tier coverage gaps, and close with a steering rule that documents
the whole testing pipeline.

## Global constraints

- **Type first, test second [owner].** For every gap, the first question is
  "can this invalid state be removed by a type?". A test is written only for
  behavior no type can express. No tests for quantity or coverage statistics.
  The typing criterion: a type must remove a representable invalid state or
  separate a genuinely confusable pair; typing for fashion is ceremony and
  gets rejected.
- The full CI battery gates every phase: `cargo build --all-targets`;
  `cargo test` in default, `--no-default-features`, `--all-features`;
  `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`;
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`.
- Pinned toolchain, edition 2024, lint levels per `Cargo.toml` — no new
  `allow` entries, no lint suppression.
- No new dependencies (timeouts use the existing `futures` crate).
- English artifacts; the agent performs no git writes — the owner commits.
- Breaking changes to the public surface are allowed (fork, git-pinned
  consumers) and must be stated in the commit message.

## Phase 1 — pure motion (zero interaction with later phases)

1. Create `src/requests/testing.rs`: the six private helpers from
   `src/requests/tests.rs` (lines 22–60) moved verbatim, now
   `#[cfg(test)] pub(crate)` — `TestServer`, `url`, `p`, `r`,
   `open_document`, `state_with_documents`. In
   `src/requests/mod.rs`: `#[cfg(test)] mod tests;` →
   `#[cfg(test)] mod testing;`.
2. Distribute the 9 tests from `src/requests/tests.rs` as inline
   `#[cfg(test)] mod tests` blocks per the research migration map (§3.3):
   `definition.rs` (1), `rename.rs` (2), `completion.rs` (1),
   `completion_resolve.rs` (3), `code_action.rs` (1),
   `document_diagnostics.rs` (1). Bodies unchanged; imports via
   `crate::requests::testing`. Delete `src/requests/tests.rs`.
3. Delete the 9 doctest-duplicated unit tests **[owner — no tests for
   quantity]**: `converts_utf8_columns_to_utf16`
   (`src/text_utils/conversions.rs:88`, identical to the
   `position_to_encoding` doctest) and the 8 `bytes_tests.rs` cases
   (`basic_sub_delimited`, `sub_delimited_delimiter_at_start`,
   `sub_delimited_delimiter_at_end`, `sub_delimited_no_delimiter`,
   `sub_delimited_empty_text`, `basic_sub_delimited_tri`,
   `sub_delimited_tri_partial`, `sub_delimited_tri_no_delimiters`),
   byte-for-byte duplicates of the `sub_delimited`/`sub_delimited_tri`
   doctests. Leave a one-line pointer comment in `bytes_tests.rs`; do not
   touch the lsp/ts twins (nothing duplicates them).

Load-bearing invariant of the moved fixture: document text `"🙂abc"` with
negotiated `Encoding::UTF16` — U+1F642 is 4 UTF-8 bytes / 2 UTF-16 units, so
byte offset 4 == UTF-16 offset 2; every moved test asserts on that identity.
Any "equivalent-looking" ASCII rewrite is a defect.

## Phase 2 — typing pass

### 2.1 `RangeError` in `src/error.rs`

All `RangeExt` panics become one typed error, defined next to `ServerError`
in `src/error.rs` **[owner — single location for errors]**:

```rust
/// Failures of [`RangeExt`](crate::text_utils::RangeExt) operations.
///
/// A leaf-utility error without protocol semantics: it never crosses the
/// wire itself and is mapped by the caller at their boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RangeError {
    #[error("position lies beyond the end of the range")]
    PositionOutOfRange,
    #[error("subrange start lies after its end")]
    StartAfterEnd,
    #[error("shrink requires a single-line range")]
    NotSingleLine,
    #[error("delimiter {delimiter:?} is not a single-byte UTF-8 character")]
    DelimiterNotSingleByte { delimiter: char },
    #[error("text length {text_len} does not match range length {range_len}")]
    TextRangeMismatch { text_len: usize, range_len: usize },
}
```

Rationale (rust-skills consultation, session 2026-08-31):
`err-canonical-struct` — "split unrelated situations into separate error
types; do not create one crate-wide catch-all merely to standardize a name";
`err-custom-type` — domain-specific enums per failure family;
`err-edge-mapping` — `ServerError` is edge-wired
(`From<ServerError> → ResponseError`); a text utility must not carry it;
`err-thiserror-lib` — closed matchable domain → public enum. Display strings
lowercase, no trailing punctuation, carry discriminating values
(`err-lowercase-msg`).

Signatures (all three impls — bytes, lsp, tree_sitter — and the trait):

```rust
fn split_at(self, text: &str, at: Self::Position) -> Result<(Self, Self), RangeError>;
fn split_off_left(self, text: &str, at: Self::Position) -> Result<Self, RangeError>;
fn split_off_right(self, text: &str, at: Self::Position) -> Result<Self, RangeError>;
fn shrink(self, amount_left: usize, amount_right: usize) -> Result<Self, RangeError>;
fn sub(self, text: &str, from: Self::Position, to: Self::Position) -> Result<Self, RangeError>;
fn sub_delimited(self, text: &str, delimiter: char) -> Result<(Option<Self>, Option<Self>), RangeError>;
fn sub_delimited_tri(self, text: &str, delim0: char, delim1: char)
    -> Result<(Option<Self>, Option<Self>, Option<Self>), RangeError>;
```

- The undocumented-by-code precondition "text must be the exact range"
  becomes the `TextRangeMismatch` check.
- `RangeError` is re-exported from `text_utils` (use path
  `async_language_server::text_utils::RangeError`) so `RangeExt` consumers
  need no `error` import.
- The 56 range_ext trio tests get a mechanical pass: call sites become
  `.expect("valid range")` (allowed in tests via clippy.toml). Doctests show
  the honest fallible form.
- `position_to_encoding` (`src/text_utils/conversions.rs`): parameters
  `impl Into<Encoding>` → `Encoding` by value if `Encoding: Copy`, else
  `&Encoding`. The `unreachable!()` at conversions.rs:70 stays — it is an
  internal invariant guarded by the same-encoding early return, unreachable
  from external input; add the one-line contract comment.

### 2.2 Public-signature audit

One deliberate sweep over public signatures with the global criterion.
Pre-verdicts (already code-checked):

- **Encoded-vs-UTF-8 position separation — NO.** `lsp_types::Position` is a
  foreign serde type embedded inside request param structs; phantom types or
  newtypes mean rebuilding every param type. The invariant is held by
  structure (conversion centralized in `Request` hooks) and pinned
  behaviorally by wire test #2.
- **`DocumentVersion` newtype — NO.** Removes no representable invalid
  state (any `i32` is spec-legal).
- **`Into<Encoding>` → `Encoding` — YES** (§2.1).
- Anything further: found list goes to the owner for approval during
  implementation; default answer is no.

### 2.3 Rule amendment **[owner]**

`.claude/rules/error-handling.md`, section "The typed error", first bullet
replaced: keep `ServerError` as the single enum *of the server/protocol
domain*; leaf utilities without protocol semantics get their own narrow
type in `src/error.rs`; do not fold unrelated situations into `ServerError`
merely to standardize the name. Same change reflected in the module's
`//!` docs if they restate the rule.

Also verify `arch-lint.toml` scope globs: `src/error.rs` must not make
`text_utils → crate::error` a layer violation (error.rs is expected to sit
outside every scope; if the layer police should rank it below text-utils,
add the one declarative line).

## Phase 3 — wire tier (async-lsp integration per research §4)

### 3.1 Production change: extract the stack helper

`src/server/serve.rs:60-74` becomes a `pub(crate) async fn` run-over-streams
(server, client socket, read, write) used by both `serve()` and the W2
tests — eliminating stack drift between tested and shipped code.
Behavior-preserving; the only production-code change of Phases 3–4.

### 3.2 W2 — white-box, `src/server/tests.rs`

New file, wired `#[cfg(test)] mod tests;` in `src/server/mod.rs`.

- Server side: the real middleware stack through the 3.1 helper over one
  end of `tokio::io::duplex` (buffer ≥ 64 KiB), via a tokio→futures adapter
  (~50 lines, modeled on `src/transport.rs:96-147`) — needed only here
  because `run_buffered` speaks futures traits.
- Client side: **no async-lsp at all** — a raw JSON-RPC client (~60 lines:
  `Content-Length: N\r\n\r\n` framing + JSON;
  `write_message`/`read_message`/`request`/`notify`) over
  `AsyncReadExt`/`AsyncWriteExt`. Exact wire bytes visible; isolated from
  async-lsp client-path bugs.
- Scaffolding: echo `TestServer` plus channel-gated handlers for the
  staleness and concurrency tests. Free of tree-sitter API so all three
  feature configurations compile the same tests (the tokio io-util/net
  features are present in every configuration).
- Determinism: channel gates, never sleeps; bounded timeouts
  (`futures::FutureExt::timeout`, no new dependency) on every cross-task
  await; `processId: None` in the test client's `initialize` keeps
  `ClientProcessMonitorLayer` inert; expected-EOF asserted at shutdown.

### 3.3 W3 — black-box, `tests/lsp_wire.rs`

Real TCP (precedent: `tests/architecture.rs`): `TcpListener` on
`127.0.0.1:0`, `serve(Transport::Socket(port), …)`, `accept()` hands the
stream to the same raw client.

### 3.4 Catalog — full 15 **[owner]**

W2 carries #1–11, #14, #15; W3 carries #12–13 (numbering per research
§4.3): negotiation end-to-end; UTF-16↔UTF-8 through real serialization;
pre-initialize gating `-32002`; double-initialize/post-shutdown `-32600`;
unknown method `-32601` as ONE parametrized test over the whole future
method surface; incremental didChange; staleness `CONTENT_MODIFIED` retry;
handler panic → structured error; concurrency bound 8; shutdown/exit clean
termination; server→client requests (`workspace/configuration`,
`client/registerCapability` — the test must answer them to observe the
effect); `$/cancelRequest` `-32800`; framing robustness smoke; TCP connect
failure → `Err(ServerError::TcpConnect)`; `serve()` happy path resolving
`Ok(())`. The two async-lsp-pinning items (#14, #15) are included as the
owner's explicit choice over the research's "skip" recommendation.

## Phase 4 — unit-tier gap fills (after typing, once, against final signatures)

| Gap | Test |
|---|---|
| UTF-32 arms + preference order | all six `Encoding::UTF32` conversion pairs; `POSITION_ENCODING_PREFERRED_ORDER` with UTF-8 first and UTF-32 second (today only the UTF-16 fallback is pinned) |
| `From<ServerError>` arms | `Lsp`, `InvalidFilePath`, `TcpConnect` → INTERNAL_ERROR (only `Other` is pinned today) |
| `handle_document_save` | text replacement from `params.text`, disk fallback, remove-on-failure, re-match |
| `DocumentMatcher` | `find` semantics, `with_lang_grammar` association, return halves of the invalid-glob/invalid-language warn paths |
| `Document::query` failure | invalid query → `None` |
| `split_off_left/right` boundaries | boundary and multiline cases across the trio; after Phase 2 these naturally pin both halves — `Ok` arithmetic and `Err(RangeError::…)` |
| `RangeError` variant triggers | one test per error path of the new contract: `DelimiterNotSingleByte`, `TextRangeMismatch`, `StartAfterEnd` — the variants not already exercised by the boundary row above |

Staleness (research gap 1) is closed by wire test #7 — no unit duplicate.
**Tracing log emission (gap 9) is out of scope**: it needs subscriber-capture
infrastructure while the return halves of the same paths are pinned above.
`serve()` coverage (gap 7) is Phase 3. Gap 8 (panics) is dissolved by
Phase 2, not tested.

## Phase 5 — testing steering document **[owner]**

After all phases land, write `.claude/rules/testing.md` — the full-pipeline
steering: the type-first criterion and the no-tests-for-statistics principle;
the three tiers (W0 inline/unit, W2 wire white-box, W3 wire black-box) and
what each is for; where each kind of test lives; the harness inventory
(`requests::testing`, the W2 raw client, W3 TCP setup); conventions (inline
`mod tests` / sibling `tests.rs`, millisecond-unique temp workspaces, no
sleeps, bounded timeouts, `processId: None`, feature-configuration
awareness); and the recipe for testing a new `Server` method through the
three-place pattern. Written last because it documents what was built.

## Error handling summary

`RangeError` per §2.1 (no `source()` slots — plain data variants, nothing to
chain); `ServerError` unchanged; the single wire boundary
(`From<ServerError> → ResponseError`) untouched. Downstream code meets
`RangeError` at its own boundary — absorbable via `ServerError::Other`
through `?` or mapped explicitly.

## Success criteria

- Battery green in all three feature configurations after every phase.
- Zero panics reachable through `RangeExt` on any input.
- All 15 wire tests deterministic (no wall-clock sleeps; bounded timeouts).
- No test exists whose assertion is subsumed by a type or lint.
- `.claude/rules/error-handling.md` matches the code; `.claude/rules/testing.md`
  exists and matches the built pipeline.

## Out of scope

Typed two-MainLoop client twin (rejected in research §4.2); tracing
subscriber capture; `publishDiagnostics` wire tests (crate never sends it);
the monolith splits of `workspace/diagnostics.rs` and
`oneshot/workspace_diagnostics.rs` (separate roadmap item); any dependency
additions.
