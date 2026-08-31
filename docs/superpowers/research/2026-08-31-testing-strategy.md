# Testing strategy research — decision document

Date: 2026-08-31. Repository state examined: `develop` at `1fc97a3` (clean
working tree): the 102-test suite in `src/` plus `tests/architecture.rs`,
`Cargo.toml`/`clippy.toml` lint configuration, `arch-lint.toml`, and vendored
async-lsp 0.2.4 sources
(`~/.cargo/registry/src/index.crates.io-…/async-lsp-0.2.4/`). No cargo runs
were performed during this research (shared target-dir lock); every claim is
source-read and cited, and each section lists the battery that must confirm it
at implementation time.

Question, in three parts: (1) after the move to typed `ServerError` errors and
the strict lint regime, which existing tests lost their value and should be
deleted, merged, or rewritten? (2) How should the `src/requests` test harness
be extracted so each new `Request` impl can be tested where it lives? (3)
Should the crate grow client-based integration tests — a test-only LSP client —
and if so, which architecture?

Method: four researcher reports (WS1 deep test revision, WS2 harness-extraction
design, WS3 async-lsp client-side facts, WS4 integration-test architecture).
10 headline claims were adversarially verified against source: **10 confirmed,
0 refuted, 0 unverified.** Two verifier nuances are folded in below as inline
corrections (the lsp.rs character-counting description in §2.4, and the
"serve() is the only entry point" overstatement in §4.2); one risk framing was
scaled back (§2.2). Facts that were not adversarially verified appear normally
with their citations; speculative points keep their labels.

## 1. Executive summary

**Recommendation: keep the suite almost intact, extract the requests harness
in-domain, and build a thin byte-level test LSP client — not a typed one.**

1. **Test revision — the strict regime deleted almost nothing.** Of 102 tests:
   93 keep, 8 merge-optional (doctest-duplicated), 1 clean deletion candidate,
   0 forced rewrites. The suite is overwhelmingly behavioral (range arithmetic,
   encoding conversion, protocol output shape, document state machine), and
   lints and types execute nothing. The one axis where static analysis
   replaced testing — arch-lint rules — is itself powered by a test
   (`tests/architecture.rs:32`). Also note `allow-unwrap-in-tests = true`
   (clippy.toml) means tests were never carrying unwrap-checking. The real
   work in this area is **additions**: the `CONTENT_MODIFIED` staleness
   contract and all UTF-32 conversion arms are completely untested (§2.3).
2. **Harness extraction — `src/requests/testing.rs`.** Move the six private
   helpers out of `src/requests/tests.rs` into a `#[cfg(test)] mod testing`
   with `pub(crate)` items (signatures verbatim), delete `tests.rs`, and
   distribute its 9 tests as inline `#[cfg(test)] mod tests` blocks in the six
   request files they exercise. Risks assessed low: no feature gates in
   `src/requests`, harness touches no tree-sitter API, arch-lint polices
   `cfg(test)` imports but requests→server is the blessed direction (§3).
3. **Integration testing — yes, but the small version.** A raw JSON-RPC client
   (~60-line framing helper + ~50-line tokio→futures adapter modeled on
   `src/transport.rs:96-147`) over `tokio::io::duplex`, driving the real
   middleware stack via a `pub(crate)` run-over-streams helper extracted from
   `serve()`, plus 2–3 black-box TCP tests through `serve()` itself. Skip the
   typed two-MainLoop twin: async-lsp 0.2.4 has no public channel-level
   interconnect, every wiring already pays full serde+framing, and the typed
   twin reuses the dependency-under-test on the client side, which worsens
   failure attribution (§4). One-time cost ~300–400 lines; ~15–30 lines per
   future `Server` method.

The three parts compose: the wire tier (part 3) is where gap 1 (staleness)
gets its best test, the harness (part 2) is what makes each new `Request` impl
cheap to test at the unit tier, and the revision (part 1) establishes that
neither replaces the existing behavioral suite.

## 2. Test revision (WS1)

### 2.1 Method and inventory

The originally referenced audit doc
(`docs/superpowers/research/2026-08-31-test-wiring-audit.md`) does not exist on
disk or in git history (`git log --all -- docs/superpowers/research/` shows
only the two 2026-08-30 commits). The inventory was rebuilt from source; every
test body was read and classified by behavior, not name:

| file | tests |
|---|---|
| src/text_utils/range_ext/tree_sitter_tests.rs | 22 |
| src/text_utils/range_ext/lsp_tests.rs | 19 |
| src/text_utils/range_ext/bytes_tests.rs | 15 |
| src/server/with_state/tests.rs | 12 |
| src/requests/tests.rs | 9 |
| src/server/state/tests.rs | 8 |
| src/error.rs | 5 |
| src/oneshot/workspace_diagnostics.rs | 4 |
| src/text_utils/conversions.rs | 2 |
| src/server/options.rs | 2 |
| src/documents/document.rs | 1 |
| src/text_utils/encoding.rs | 1 |
| src/tree_sitter_utils.rs | 1 |
| src/workspace/walker.rs | 1 |
| tests/architecture.rs | 1 |
| **total** | **102** |

The classification lens — what the type system and lints actually guarantee:
`ServerError` is a single `#[non_exhaustive]` thiserror enum (src/error.rs:26-62)
with one `From<ServerError> for ResponseError` boundary conversion
(src/error.rs:72-79); exhaustiveness of that match is compiler-checked, the
mapping *result* is not. Clippy denies `expect_used`/`unwrap_used` in
production code but allows both in tests (clippy.toml
`allow-unwrap-in-tests`/`allow-expect-in-tests = true`) — so no test exists to
"check for no unwraps" and none is needed. arch-lint rules
(`NoErrorSwallowing`, `NoSilentResultDrop`, `RequireThiserror`, `NoSyncIo`,
`RequireTracing`, `TracingEnvInit`, plus declarative scope rules) inspect code
shape; they never execute the code under test. A test is a deletion candidate
only if its assertion is subsumed by a static guarantee or duplicates other
coverage.

### 2.2 Verdict counts and deletion candidates

- **keep: 93**
- **merge (optional, doctest-duplicated): 8** — `bytes_tests.rs:48, 55, 82,
  89, 96, 103, 110, 118`
- **delete (duplication criterion met): 1** — `conversions.rs:88`
- **rewrite: 0**

**The one clean deletion: `converts_utf8_columns_to_utf16`
(src/text_utils/conversions.rs:87-95).** Its assertions — text `"a🙂b"`, col 5
→ col 3, `Encoding::UTF8 → Encoding::UTF16` — are identical to the doctest on
`position_to_encoding` (src/text_utils/conversions.rs:9-18, same string via
`\u{1f642}`, same input/output positions). The doctest runs in all three
feature configurations, verified from source rather than convention:
`Cargo.toml [lib]` has no `doctest = false`, `src/lib.rs:15` declares
`pub mod text_utils;` with no feature gate, and `.github/workflows/rust.yml`
runs `cargo test` in default, `--no-default-features`, and `--all-features`.
It is the only doctest-duplicated test in the file. One correction from
verification: deleting it does not leave the (UTF8, UTF16) arm with "zero
direct coverage" — `caps_lines_before_converting_columns` (conversions.rs:98)
also exercises that arm on `"first\n🙂"`. The deletion is safe either way; the
risk framing in the original report was overstated.

**The 8 merge-optional tests** (all in `src/text_utils/range_ext/bytes_tests.rs`):
`basic_sub_delimited`, `sub_delimited_delimiter_at_start`,
`sub_delimited_delimiter_at_end`, `sub_delimited_no_delimiter`,
`sub_delimited_empty_text`, `basic_sub_delimited_tri`,
`sub_delimited_tri_partial`, `sub_delimited_tri_no_delimiters`. Each asserts
byte-for-byte what the `sub_delimited`/`sub_delimited_tri` doctests already
assert (src/text_utils/range_ext/mod.rs:111-115 and 138-149): identical
ranges, delimiter positions, and asserted outputs; four of the eight pairs
differ only in same-length ASCII filler text. The doctests are a strict
superset (mod.rs:150 `tri-empty-text` has no unit-test mirror), so coverage
survives deletion — but per-file case symmetry with the lsp/ts twins is a
reading aid, and doctests are documentation that can be edited for brevity
without review treating it as coverage loss. Verdict: **merge-optional, low
priority; default keep.** If taken, delete the 8 unit copies, leave a pointer
comment in `bytes_tests.rs`, and do not touch the lsp/ts twins (no doctest
duplicates them).

### 2.3 Coverage gaps (highest value first)

These are additions the suite needs regardless of any deletion:

1. **`CONTENT_MODIFIED` staleness detection — untested.** The advertised
   retry contract (`implement_method!` step 4a, src/server/with_state/mod.rs:74-78)
   and the workspace-diagnostics variant (src/workspace/diagnostics.rs:422-428)
   have no test that triggers them: nothing mutates a document version *while
   a handler is in flight*. This is the crate's headline staleness guarantee.
   The deterministic recipe requires a `Server` impl gated on channels — best
   built once at the wire tier (§4.3, catalog item 7).
2. **UTF-32 conversion arms — untested.** The `position_to_encoding` arms at
   src/text_utils/conversions.rs:46-49, 56-59, 61-68 have zero direct tests;
   no test anywhere sets `Encoding::UTF32` as source or target with real text.
   The preference order (`POSITION_ENCODING_PREFERRED_ORDER`,
   src/server/with_state/mod.rs:28-34) is only tested for the UTF-16 fallback
   (with_state/tests.rs:217) — UTF-8 preferred and UTF-32 second are
   unasserted.
3. **`From<ServerError>` untested arms.** `Lsp`, `InvalidFilePath`,
   `TcpConnect` → INTERNAL_ERROR mapping (src/error.rs:76) untested; only
   `Other` is (error.rs:126).
4. **`handle_document_save` — untested** (src/server/state/documents.rs:265-296):
   text replacement from `params.text`, disk fallback, remove-on-failure,
   re-match. Zero references in any test file.
5. **`DocumentMatcher` has no own tests** (src/documents/matcher.rs, 0
   `#[test]`). Indirect coverage exists (`**/*.test` globs, lang strings), but
   `find` semantics, `with_lang_grammar` association, and the
   invalid-glob/invalid-language warn paths (matcher.rs:131, 144) are unpinned.
6. **`Document::query` failure path untested** (src/documents/document.rs:175):
   an invalid query returns `None` + `tracing::warn!` (document.rs:183) — the
   `None` half is testable without log capture.
7. **`serve()` untested** (src/server/serve.rs): transport wiring, middleware
   stack, client-process monitor; the doctest is `no_run`. Covered by §4.
8. **8 documented `# Panics` contracts, zero `#[should_panic` tests.** The
   `# Panics` sections in `src/text_utils/range_ext/` (out-of-range `at`,
   multiline `shrink`, `from > to`, multi-byte delimiter) are client-visible
   panics on untrusted input — the multi-byte-delimiter panic is reachable
   from a hostile client, which the error-handling rule says must not happen
   (verify whether those are invariants or should become `Result`s).
9. **Tracing fire-and-forget paths untested**: warn sites at walker.rs:70,
   diagnostics.rs:291, 351, documents.rs:245, matcher.rs:131, 144,
   document.rs:183. Their *return-value* halves are items 5–6; log emission
   itself needs a subscriber capture — flagged, not prescribed.
10. **`split_off_left`/`split_off_right` boundary behavior**: only happy-path
    `basic_*` tests exist in each of the three range_ext files; no
    boundary/multiline cases (contrast `split_at`).

### 2.4 range_ext trio consolidation evaluation (56 tests, 489 lines)

Facts: 15 case names appear in all three files; 4 more are shared by lsp+ts
only; 3 are tree-sitter-only. The three impls are *independent code* —
bytes.rs:4-101 (plain offsets, `_text` ignored), lsp.rs:3-191, tree_sitter.rs:3-293
(byte-column counting at tree_sitter.rs:28-40). Shared names are a shared
*spec matrix*, not shared coverage: an LSP `split_at` regression is invisible
to the bytes tests, because each module imports only its own range type.
One correction from verification: lsp.rs:115-116 uses
`.chars().count()`, which counts Unicode scalar values (code points), not
UTF-16 code units — they differ for astral characters. The crate's actual
UTF-16 arithmetic is ropey's `len_utf16_cu` (conversions.rs:41-67). The
load-bearing distinction is unaffected: lsp counts characters, tree-sitter
counts bytes, bytes.rs counts nothing.

Options, with failure-diagnostic granularity as the first-class criterion:

- **Leave as-is (recommended).** 56 independently named tests: a failure
  identifies method × type × case exactly, `cargo test
  split_at_boundaries` filters precisely, parallel execution is maximal.
- **Table-driven within each file** — saves ~15 lines/file; costs the
  per-case name in the test list, breaks `cargo test <case>` filtering, and
  rows after a failing assert go unreported. Net loss.
- **Macro-generated tests** — preserves runtime granularity but deduplicates
  only the `#[test] fn NAME()` boilerplate; type-specific fixtures and
  expected values are ~90% of each body. Also conflicts with "three similar
  lines beats a premature abstraction".
- **Cross-type generic table** — requires a test-only position-arithmetic
  converter: a parallel reimplementation of the code under test, whose bugs
  would mask impl bugs. Rejected (no generic `Position` constructor exists;
  `TsRange` carries byte offsets the others lack; `position_to_encoding` has
  no byte-column variant).

## 3. Harness extraction (WS2)

### 3.1 Current state

`src/requests/tests.rs` (286 lines) = 6 private harness helpers (tests.rs:22-60)
+ 9 tests (tests.rs:62-286). The harness is deliberately thin: an empty-impl
`TestServer` (no capabilities, no matchers — trait defaults return none,
src/server/server_trait.rs:49-59), `ServerState::with_options` with a closed
`ClientSocket` and default `ServerOptions`, `Encoding::UTF16`, and two
in-memory documents (`"abcdef"` / `"🙂abc"`) at `file:///tmp/...` URLs — zero
disk IO, no `initialize`. The `"🙂abc"` text is load-bearing: U+1F642 is 4
UTF-8 bytes / 2 UTF-16 units, so byte offset 4 == UTF-16 offset 2, and that
identity is what every moved test asserts. Usage census (verified):
`state_with_documents` 7 call sites, `r()` 20, `url()` 7, `open_document`
2 direct + 2 inside the fixture, `TestServer` 3; nothing outside the file
references any of it (sole declaration: src/requests/mod.rs:22-23, and all
items are private in a private child module). The 2026-08-30 structure spec
centralized these tests as a deliberate mechanical move with "the owner has a
separate test revision planned" — this is that revision, so distribution is
now in scope.

### 3.2 Target layout and API sketch

```
src/requests/
├── mod.rs                    EDITED: `#[cfg(test)] mod tests;` → `#[cfg(test)] mod testing;`
├── testing.rs                NEW — harness, ~70 lines (tests.rs:1-60 content,
│                                   minus unneeded imports, plus pub(crate))
├── definition.rs             + inline #[cfg(test)] mod tests (1 test)
├── rename.rs                 + inline #[cfg(test)] mod tests (2 tests)
├── completion.rs             + inline #[cfg(test)] mod tests (1 test)
├── completion_resolve.rs     + inline #[cfg(test)] mod tests (3 tests)
├── code_action.rs            + inline #[cfg(test)] mod tests (1 test)
├── document_diagnostics.rs   + inline #[cfg(test)] mod tests (1 test)
└── tests.rs                  DELETED
```

```rust
//! src/requests/testing.rs — test-only baseline for per-request conversion
//! tests. Declared #[cfg(test)] in mod.rs; never compiled into non-test builds.

pub(crate) struct TestServer;                                   // tests.rs:22-24
impl Server for TestServer {}
pub(crate) fn url(path: &str) -> Url;                           // tests.rs:26-28
pub(crate) const fn p(line: u32, character: u32) -> Position;   // tests.rs:30-32
pub(crate) const fn r(line: u32, start: u32, end: u32) -> Range;// tests.rs:34-39
pub(crate) fn open_document(state: &mut ServerState, uri: Url, text: impl Into<String>); // tests.rs:41-45
pub(crate) fn state_with_documents() -> (ServerState, Url, Url);// tests.rs:47-60
```

Signatures move **verbatim**, not redesigned (D3); ergonomics (a `Fixture`
struct, parameterized encoding) are follow-ups. Test modules import via
`use crate::requests::testing::{...}` — the absolute path, since
`super::super::testing` is unreadable from inside a request file's test mod.
An optional `sole_document_state(text)` helper would deduplicate the two
sole-document tests' setup blocks (2 call sites — judgment call; "three
similar lines beats a premature abstraction").

### 3.3 Migration mapping

| # | Test (current lines in src/requests/tests.rs) | Exercises | Destination |
|---|---|---|---|
| 1 | `definition_locations_are_converted_using_their_own_document` (62-77) | `<Definition as Request>::modify_response` | `definition.rs` |
| 2 | `workspace_edits_are_converted_using_their_own_document` (79-96) | `<Rename as Request>::modify_response` | `rename.rs` |
| 3 | `completion_additional_text_edits_are_converted` (98-117) | `<Completion as Request>::modify_response` | `completion.rs` |
| 4 | `code_action_context_diagnostics_are_converted` (119-142) | `<CodeAction as Request>::modify_params` | `code_action.rs` |
| 5 | `document_diagnostic_related_documents_are_converted_using_their_own_document` (144-180) | `<DocumentDiagnostics as Request>::modify_response` | `document_diagnostics.rs` |
| 6 | `rename_edits_fall_back_to_request_document_when_target_is_unknown` (182-200) | `<Rename as Request>::modify_response` (fallback) | `rename.rs` |
| 7 | `resolve_edits_convert_against_the_sole_tracked_document` (202-230) | `convert_completion_resolve` | `completion_resolve.rs` |
| 8 | `resolve_edits_pass_through_without_a_document` (232-252) | `convert_completion_resolve`, `None` doc | `completion_resolve.rs` |
| 9 | `resolve_echo_round_trip_is_identity` (254-286) | incoming + outgoing compose to identity | `completion_resolve.rs` |

No assertion or body text changes; only import paths. `self_named_module_files
= "warn"` (Cargo.toml) locks `testing.rs` to a leaf file — never create a
`testing/` directory beside it.

### 3.4 Risks

- **Feature-config matrix — LOW.** `src/requests/` contains exactly one `cfg`
  in the whole tree (the `#[cfg(test)]` on `mod tests;`). The harness touches
  only `async_lsp::ClientSocket`/lsp_types, `crate::server::{Server,
  ServerOptions, ServerState}`, and `crate::text_utils::Encoding` — none
  feature-gated. The only feature-sensitive path is `insert_document`'s
  tree-sitter branch, inert with no matcher-supplied grammar. Same behavior in
  all three configurations.
- **arch-lint — LOW.** arch-lint parses full ASTs without applying `cfg`, so
  test-only imports are policed (`allow_in_tests` is parsed but never
  consulted). `src/requests/testing.rs` sits in the `requests` scope
  (`src/requests/**` glob); its imports are requests → server (blessed
  direction) and requests → text-utils (not denied); request-file test mods
  importing `crate::requests::testing` resolve to a self-edge, and no deny
  rule targets `requests` from `requests`.
- **Conventions — LOW.** `missing_docs` cannot fire on `pub(crate)` cfg(test)
  items; `TestServer` already exists as a private struct in four other test
  modules with no conflict (a future crate-wide harness should pick a distinct
  name). The `let _ =` on `handle_document_open` drops a `ControlFlow`, not a
  `Result` — `NoSilentResultDrop` is green on that code today.
- **Behavioral — NONE INTENDED.** Any diff beyond imports/paths is a defect.
  The review-critical semantic: the fixture's `"🙂abc"` and UTF-16 encoding
  are load-bearing; an "equivalent-looking" ASCII rewrite would silently stop
  exercising the conversion.

A crate-wide `src/testing/mod.rs` was considered and rejected: it edits
`lib.rs`, adds a top-level module for a requests-only need, and a module
matched by no `[[scopes]]` glob silently escapes the layer rules. If a later
cycle generalizes, the seam is promoting `requests::testing`'s generic pieces
(url/p/r, temp-workspace, TestServer) and keeping `state_with_documents`
domain-local.

Verification (not run during design): `cargo test --lib requests::`, the full
battery in all three feature configurations, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings"
cargo doc --no-deps`, `cargo test --test architecture`.

## 4. Client-based integration testing (WS3 + WS4)

### 4.1 Verified async-lsp 0.2.4 facts

All adversarially confirmed against vendored source:

- **Client construction**: `MainLoop::new_client(builder) -> (MainLoop,
  ServerSocket)` (async-lsp lib.rs:497-500); the closure returns any
  `LspService` (typically a `Router`); the loop is driven by `run`/
  `run_buffered` while the `ServerSocket` makes the calls. Structural
  requirement: a pending `request().await` cannot complete unless the loop is
  concurrently polled — the crate's own test tokio-spawns both loops before
  calling `initialize`.
- **Socket surface**: `ClientSocket` and `ServerSocket` expose exactly four
  inherent public methods each — `new_closed()`, async `request::<R>`, sync
  `notify::<N>` (queued, not sent), sync `emit::<E>` (loopback to own
  service) — via one macro (lib.rs:663-722); both are `Clone + Debug`.
- **Typed API**: `ServerSocket` implements the full `LanguageServer`
  omni-trait, every method forwarding to `request`/`notify`
  (omni_trait.rs:209-210); `ClientSocket` implements `LanguageClient`;
  `Router::from_language_server`/`from_language_client` produce fully
  registered routers (omni_trait.rs:219-237, 320).
- **No channel-level coupling exists.** `MainLoop`'s `rx` and `PeerSocket`'s
  `tx` are private (lib.rs:469, 724-727); the only public coupling is
  `run`/`run_buffered` over *futures*-trait byte streams (lib.rs:521-533), so
  any two-loop wiring pays full `serde_json` + `Content-Length` framing both
  directions (lib.rs:418-463). Even the `forward` feature (not enabled here)
  still serializes.
- **Hazards for test clients**: (1) unhandled non-`$/` notifications
  terminate the loop with `Error::Routing` (router.rs:64-73) — the harness
  router needs registrations or an `unhandled_notification` catch-all; (2) no
  built-in timeouts or outgoing-request cancellation anywhere (lib.rs:22,
  concurrency.rs:1-8) — the harness must wrap; (3) peer close surfaces as
  `Err(Error::Eof)` from `run()` — the expected termination (lib.rs:424-426).
- **No new async-lsp feature is needed for client use**: `new_client`, both
  sockets, `Router`, `run/run_buffered` are feature-free; `omni-trait` is
  already enabled (`Cargo.toml:23`). The real client-side cost is the I/O
  bridge (`run_buffered` speaks futures traits; a tokio harness has tokio
  traits): a tokio-util dev-dep, or the hand-rolled adapter the crate already
  has at `src/transport.rs:96-147`. No dedicated test API exists (upstream
  `tests/unit_test.rs:2` carries a TODO).

### 4.2 Wiring options and the recommendation

Hard constraint shaping everything (verified): `LanguageServerWithState` is
`pub(crate)` (src/server/with_state/mod.rs:108; the public `pub use` list in
src/server/mod.rs:18-30 excludes it), and `Transport` offers only Stdio or
TCP-connect (src/transport.rs:53-75). Correction from verification: `serve()`
is not literally the *only* public entry — `oneshot::workspace_diagnostics`
(src/oneshot/workspace_diagnostics.rs:210) also drives
`LanguageServerWithState` directly — but that path is diagnostics-only,
skips the middleware stack entirely, and hardcodes UTF-8, so the conclusion
stands: a `tests/`-dir harness cannot cover the stack without widening the
public API. Hence a two-tier architecture:

- **W0 — direct drive (status quo).** The existing unit tests and `oneshot`:
  trait impl + state + conversion, no serialization, no framing, no
  middleware; a closed socket makes every server→client request fail with
  `ServiceStopped`.
- **W1 — typed two-MainLoop twin.** Server loop + client loop
  (`new_client` + `Router::from_language_client`), both over in-memory bytes.
  Exercises framing/serde both directions and the typed server→client request
  path. Costs: ~300+ lines, both loops share the dependency under test (a bug
  in async-lsp's client path can mask a server bug), and the typed client
  hides exact wire bytes. **Skip.**
- **W2 — byte-level over `tokio::io::duplex` (recommended bulk).** Server
  side: the real stack from `serve.rs:60-74` run over one duplex end via an
  extracted `pub(crate)` helper; client side: a hand-rolled raw JSON-RPC
  client using `tokio::io::AsyncReadExt/AsyncWriteExt` — **no futures adapter
  on the client side at all**, since it never touches `MainLoop`. The
  ~60-line framing helper (`Content-Length: N\r\n\r\n` + JSON) covers
  `write_message`/`read_message`/`request`/`notify`; the ~50-line adapter
  (template: `src/transport.rs:96-147`) is needed only on the server side.
  `tokio::io::duplex` exists under the already-enabled `io-util` feature.
  Fully in-memory, deterministic; the loop's biased ordering (outgoing flush
  > completed tasks > incoming, lib.rs:546-568) makes gated request-then-
  didChange sequences reliable.
- **W3 — real TCP against `Transport::Socket`.** Bind a listener on
  `127.0.0.1:0`, spawn `serve(Transport::Socket(port), s)`, `accept()`, and
  the accepted stream becomes the raw client's transport. The only wiring
  that executes `serve()` itself, `Transport::into_read_write`'s
  `TcpConnect` error path, and `serve()`'s error mapping. ~1 ms per connect;
  2–3 tests only.

| | W0 direct | W1 loop twin | W2 duplex + raw client | W3 TCP + `serve()` |
|---|---|---|---|---|
| serde round trip | no | yes, both sides | yes | yes |
| Content-Length framing | no | yes | yes | yes |
| Middleware stack | no | yes* | yes* | yes |
| `serve()` + `Transport` | no | no | no | **yes** |
| `TcpConnect` error path | no | no | no | **yes** |
| Sees exact wire bytes | n/a | no (typed) | **yes** | yes |
| Isolated from async-lsp client bugs | n/a | no | **yes** | yes |
| Harness size | 0 (exists) | ~300+ lines | ~150–200 lines | ~60 (reuses W2) |
| Where it lives | anywhere | src only | src only | `tests/` or src |

\* only if built from the same code as `serve()` — hence the extracted-helper
refactor; without it, tests rebuild `serve.rs:60-74` by hand and drift.

**Recommendation (one): build W2 — a raw byte-level client over
`tokio::io::duplex`, driving the real stack through an extracted
`pub(crate)` run-helper — plus 2–3 W3 TCP tests through `serve()`.** W1's one
unique capability (serving `workspace/configuration` and
`client/registerCapability` from a typed chair) costs ~20 lines of raw
response-writing in W2.

### 4.3 Prioritized test catalog

W2 = duplex white-box; W3 = TCP black-box. Ordered by value.

| # | Test | What it pins | Wiring |
|---|---|---|---|
| 1 | Initialize + position-encoding negotiation end to end | Negotiation inputs survive the JSON round trip; advertised `position_encoding` in `InitializeResult`; `LifecycleLayer` Uninitialized→Initializing. Bonus: client offering `["utf-16","utf-8"]` gets `utf-8` (preference order, with_state/mod.rs:28-34) | W2 (+once in W3) |
| 2 | UTF-16 ↔ UTF-8 conversion through real serialization | Composition of `modify_params`/`modify_response` with serde (with_state/mod.rs:57-63, 81-86); echo `TestServer` proves the handler saw UTF-8; raw client asserts the response serializes as UTF-16 units. Conversion math is unit-tested; the composed wire path is not | W2 |
| 3 | Requests before `initialize` → `-32002` | `LifecycleLayer` gating (async-lsp server.rs:72-83), installed in serve.rs:61, unreachable from unit tests | W2/W3 |
| 4 | Double `initialize` / post-`shutdown` request → `-32600` | Lifecycle uniqueness rules | W2/W3 |
| 5 | Unknown method → `-32601` | Router/omni-trait dispatch layer — currently unpinned (unit tests bypass dispatch). One parametrized test covers every unimplemented method, so the surface can grow without new tests | W2/W3 |
| 6 | Incremental `didChange` over the wire | Negotiated-encoding → rope-edit pipeline as composed (state/documents.rs:133-173); UTF-16 coordinate translation plus notification ordering | W2 |
| 7 | Staleness `CONTENT_MODIFIED` + retry flow | `implement_method!` version snapshot/re-check (with_state/mod.rs:74-79) — WS1 gap 1. Deterministic recipe: handler gated on start/release channels; send hover → wait start → `didChange` → release → assert `-32801` → retry → success | W2 |
| 8 | Handler panic → structured error | `CatchUnwindLayer` presence and message shape (`Request handler of textDocument/hover panicked: ...`) — documents what downstream servers see | W2 |
| 9 | Concurrency limit of 8 | `ConcurrencyLayer` semaphore (configured `MAX_CONCURRENT_REQUESTS = 8` at serve.rs:18-21): nine gated hovers, exactly 8 gates open, 9th only after release | W2 |
| 10 | Shutdown/exit lifecycle and clean termination | `exit` breaks the loop with `Ok`, output flushed and closed; in W3 additionally assert the `serve()` future resolves `Ok(())` (serve.rs:76-79) — nothing currently tests that the entry point terminates | W2 + W3 |
| 11 | Server→client requests: `client/registerCapability`, `workspace/configuration` | Spawned socket-request round trip incl. the generation guard (workspace/diagnostics.rs:279-331). Harness note: an unanswered request is only logged under `tracing` — the test must respond to observe the effect | W2 |
| 12 | `Transport::Socket` connect failure | `Err(ServerError::TcpConnect { .. })` mapping (transport.rs:57-59, serve.rs:58) and `serve()`'s early return. Residual rare race: ephemeral port reuse between drop and connect [Speculation: failure probability in CI is low but nonzero] | W3 |
| 13 | `serve()` happy path over TCP | The composition of the only general public entry point — initialize, one document request, shutdown, exit, `serve()` resolves `Ok(())` | W3 |
| 14 | `$/cancelRequest` → `-32800` (optional) | async-lsp `ConcurrencyLayer` behavior we merely configured, not our code — lowest value | W2 |
| 15 | Framing robustness smoke (low priority) | Malformed header → `Error::Protocol`; queued messages preserve order. Mostly pins async-lsp; one smoke test is enough | W2 |

Nine of these (3, 4, 5, 7, 8, 9, 10, 12, 13) are **wire-only** — no existing
test can express them; four (1, 2, 6, 11) pin composed halves of
already-unit-tested logic.

**Not applicable: `textDocument/publishDiagnostics`.** The crate never sends
it (`grep -ri "publish" src/` is empty); diagnostics are pull-only. A wire
test for it would exercise only async-lsp plumbing plus downstream-server
code.

### 4.4 Location and feature configurations

- **Tier 1 — `src/server/tests.rs`** (new, wired `#[cfg(test)] mod tests;` in
  `src/server/mod.rs`), matching the sibling-file convention
  (src/server/with_state/mod.rs:251-252). Lives in `src/` because only there
  is the `pub(crate)` type and the `pub(crate)` stack helper reachable.
  Companion refactor: extract `serve.rs:60-74` into a `pub(crate) async fn`
  run-over-streams helper used by both `serve()` and the tests — a
  behavior-preserving move that eliminates stack drift.
- **Tier 2 — `tests/lsp_wire.rs`**, black-box through `serve()` + TCP only
  (precedent: `tests/architecture.rs`).
- **Feature configurations — the harness needs no gates.** Tokio features
  available to tests are the union of normal and dev sets: `io-util` (duplex)
  and `net` (TCP) are present in every configuration. `--no-default-features`
  just drops `TracingLayer` (serve.rs:63-64); `--all-features` adds
  tree-sitter — keep the wire `TestServer` free of tree-sitter API so all
  three configurations compile the same tests. One real gap: **tokio's `time`
  feature is not enabled anywhere** (Cargo.toml:31, 41), so
  `tokio::time::timeout` is unavailable as-is — use
  `futures::FutureExt::timeout` (available via futures' default `std`
  feature; the futures-util method check was against 0.3.30 [Inference: the
  0.3.34 delta does not change this method]), or add `"time"` to the
  dev-dependency.
- **Flakiness controls**: channel gates, never sleeps; bounded timeouts on
  every cross-task await; duplex buffer ≥ 64 KiB; `processId: None` in the
  test client's `initialize` keeps `ClientProcessMonitorLayer` inert
  (async-lsp client_monitor.rs:49-57) — never send a dead pid; expected-EOF
  assertion at shutdown so a closing server is an assertion, not a hang.

### 4.5 Cost/benefit against the full-method-surface plan

The owner's plan expands the wired `async-lsp` method surface (13 requests
wired through `implement_methods!` today, with_state/mod.rs:234-248, against
20 trait functions in server_trait.rs).

- **One-time**: ~300–400 lines — framing helper (~60), tokio→futures adapter
  (~50, modeled on transport.rs:96-147), gated/echo `TestServer`
  scaffolding (~80), the `pub(crate)` run-helper extraction (~10 moved
  lines), per-test plumbing. Amortizes over every method and every future
  regression in stack composition.
- **Per method**: ~15–30 lines — one wire smoke test (initialize, open a doc,
  call the method, assert echo/converted positions), shareable as a
  parametrized table. The expensive per-method logic (encoding conversion)
  already has unit tests (§3); the wire tier only pins dispatch, conversion,
  and framing. Unimplemented methods' `-32601` is a single parametrized test
  (item 5), so growth adds no tests.
- The wire-only behaviors (lifecycle gating, panic-to-error, staleness
  retry, concurrency bound, clean termination) guard exactly the promises
  `serve()`'s doc comment makes (serve.rs:27-32) — currently unverified
  promises.

## 5. Proposed next steps

Ordered; each is an independently shippable change with its own battery run.

1. **Extract the requests harness** (§3): create `src/requests/testing.rs`,
   move the 9 tests inline per the migration map, delete `tests.rs`, update
   `mod.rs`. Pure motion; verify no diff beyond imports/paths.
2. **Take (or explicitly decline) the two revision items** (§2.2): delete
   `converts_utf8_columns_to_utf16` (conversions.rs:88, exact doctest
   duplicate); decide the 8 merge-optional bytes tests (default: keep).
3. **Extract the `serve()` stack helper** (§4.2): `pub(crate)` run-over-streams
   function from `serve.rs:60-74`, `serve()` delegating to it. This is the
   only production-code change in the whole plan; review it as
   behavior-preserving.
4. **Build the W2 harness** (§4.2/§4.4): `src/server/tests.rs` with the raw
   JSON-RPC client + duplex wiring; land tests 1, 2, 3, 7 first (negotiation
   shape, composed conversion, lifecycle gating, staleness — staleness closes
   WS1 gap 1).
5. **Add the W3 tier** (§4.3 items 12–13): `tests/lsp_wire.rs` with the
   TCP-connect-failure and happy-path tests. Decide the timeout mechanism
   (futures `FutureExt::timeout` vs a tokio `time` dev-dep) before writing
   the first bounded await.
6. **Fill the remaining unit-tier gaps in priority order** (§2.3): UTF-32
   conversion arms and preference order (gap 2), `From<ServerError>` untested
   arms (gap 3), `handle_document_save` (gap 4), `DocumentMatcher` semantics
   (gap 5), `Document::query` failure (gap 6).
7. **Decide the `# Panics` question** (gap 8): 8 documented panic contracts,
   zero `#[should_panic]` tests, and at least one (multi-byte delimiter) is
   reachable from hostile client input — either pin the contracts with
   `#[should_panic]` tests or convert the reachable ones to `Result`s per the
   error-handling rule.
8. **Optionally**: W2 tests 4, 5, 6, 8–11 and the merge-optional bytes cleanup,
   as capacity allows.

## Sources

Researcher reports (input to this document):

- /tmp/als-testing-research/ws1-classification.md — test revision
- /tmp/als-testing-research/ws2-harness-design.md — harness extraction design
- /tmp/als-testing-research/ws3-async-lsp-facts.md — async-lsp 0.2.4 facts
- /tmp/als-testing-research/ws4-integration-architecture.md — integration
  architecture

This repository (read and spot-verified this session):

- src/requests/{mod.rs, tests.rs} and the seven request files
- src/server/{serve.rs, mod.rs, state/tests.rs, with_state/mod.rs,
  with_state/tests.rs}
- src/text_utils/{conversions.rs, range_ext/{mod.rs, bytes.rs, lsp.rs,
  tree_sitter.rs, bytes_tests.rs, lsp_tests.rs, tree_sitter_tests.rs}}
- src/{error.rs, transport.rs, oneshot/server.rs,
  oneshot/workspace_diagnostics.rs, workspace/diagnostics.rs}
- Cargo.toml, clippy.toml, arch-lint.toml, tests/architecture.rs,
  .github/workflows/rust.yml

Vendored async-lsp 0.2.4 (read): src/lib.rs, src/router.rs,
src/omni_trait.rs, src/concurrency.rs, src/panic.rs, src/client_monitor.rs,
tests/unit_test.rs, examples/{client_builder,client_trait,inspector,
server_builder,server_trait}.rs
