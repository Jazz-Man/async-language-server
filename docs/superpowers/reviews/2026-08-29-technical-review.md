# Technical Review — async-language-server

**Date:** 2026-08-29
**Branch / HEAD:** `feature/abstraction` @ `c7e86df`
**Cycle:** read-only — findings and rules only, no code changes (owner's directive)
**Method:** LSP-first analysis (`documentSymbol` inventories on `server_state.rs`, `document.rs`, `workspace_diagnostics.rs`, `examples/tree_sitter.rs`, plus reads where bodies were needed), grep sweeps for failure-path patterns, full CI battery run, thiserror 2.0.20 documentation researched first-hand (docs.rs), findings checked against the `rust-skills` rule set (rule ids cited per finding).

---

## Executive summary

The library's core shape is sound and idiomatic Rust: a strategy-style `Server`
trait as the user layer, an adapter (`LanguageServerWithState`) over async-lsp,
builder-style configuration, and a tower middleware stack. For the stated
purpose — "implement `Server`, override a few methods, run `serve()`" — the
abstraction level is right; nothing found suggests restructuring toward a GoF
"Abstract Factory" (that pattern solves a different problem: creating families
of related objects).

The real debt is concentrated in two places:

1. **The verification gate is currently red** — `cargo clippy --all-targets --
   -D warnings` fails on three `unused_async` sites (two examples, one test
   server). Everything else in the battery is green.
2. **The error model loses information** — a stringly-typed
   `Unknown(String)` catch-all, several swallowed or untraced failure paths,
   and two panics reachable from inputs. Each is small; together they are the
   gap between "thiserror is used" and "no error is missed".

Counts: **1 Critical, 9 Important, 8 Minor**, plus positive findings and two
previously known items (one specced, one now formalized).

For orientation (the owner is new to Rust): none of the findings indicate a
wrong architecture. They are hygiene, robustness, and contract-shape issues —
normal for a fork being brought up to a higher quality bar.

---

## Verification battery (run 2026-08-29, local HEAD)

| Check | Result |
|---|---|
| `cargo build --all-targets` | ✅ pass |
| `cargo test` (default) | ✅ 87 + 12 passed |
| `cargo test --no-default-features` | ✅ 64 + 12 passed |
| `cargo test --all-features` | ✅ 87 + 12 passed |
| `cargo fmt --check` | ✅ pass |
| `cargo clippy --all-targets -- -D warnings` | ❌ **exit 101** |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | ✅ pass |

Clippy errors (all the same lint, `unused_async` — "unused `async` for async
trait impl function with no `.await` statements"):

- `examples/minimal.rs:40` (`document_diagnostics` impl)
- `examples/tree_sitter.rs:47` (`document_diagnostics` impl)
- `src/oneshot/workspace_diagnostics.rs:323` (test server impl)

Root-cause note: `src/server_trait.rs:1-4` carries a module-level
`#![allow(clippy::unused_async)]` that masks this exact lint inside the crate's
own trait defaults; the examples and the oneshot test server sit outside that
allow and expose it. Because the trait uses RPITIT (`-> impl Future`), an impl
without `.await` may be a plain `fn` returning `std::future::ready(...)`. See
**C1** and **M5**.

---

## Dimension A — Abstraction & API design

### What the design actually is (Rust terms)

- **`Server` trait** (`src/server_trait.rs`) — a *strategy / plugin interface*
  with default implementations (the "optional override" pattern). In
  TypeScript terms: like an interface with optional methods that the framework
  calls back into.
- **`LanguageServerWithState`** (`src/server_with_state.rs`) — an *adapter*
  (and partial facade) implementing async-lsp's `LanguageServer` by driving
  the user's `Server` implementation with document state handled around it.
- **`DocumentMatcher`, `ServerOptions`** — *builder-style* configuration
  (`with_*` consuming self, `#[must_use]`).
- **`serve()`** (`src/serve.rs`) — composition root wiring a tower
  `ServiceBuilder` middleware stack.

"Abstract Factory" (create families of related objects through a factory
interface) does not map onto any problem this crate has. The closest Rust
idiom to what the owner described is exactly what exists: a trait with default
methods as the extension seam. **Verdict: keep the shape; no pattern change is
recommended.** [Judgment, high confidence]

### Public-surface findings

- **A1 (Important, I9 below): README is the rendered crate documentation but
  does not document the API.** `#![doc = include_str!("../README.md")]`
  (`src/lib.rs:1`) makes README the docs landing page, yet it never mentions
  `Server`, `serve()`, `DocumentMatcher`, workspace diagnostics, or `oneshot`,
  contains no code example, and keeps the upstream author's first-person voice
  ("a personal project of mine", `README.md:24`). Rules: `doc-crate-readme`,
  product.md documentation stance.
- **A2 (Minor, M2): `#[non_exhaustive]` is applied inconsistently.**
  `Transport` is `#[non_exhaustive]` (`src/transport.rs:31`); `ServerError`
  (`src/result.rs:27`), `DocumentSymbolResponse`-style re-exports aside, and
  `LspTransportRead`/`LspTransportWrite` (`src/transport.rs:89,119`) are not.
  The fork's stated policy is additive growth (product.md); `#[non_exhaustive]`
  is the mechanism that makes enum growth additive for downstream matches.
  Rule: `api-non-exhaustive`.
- **A3 (Minor, M6): `DocumentMatcher` exposes public fields *and* builder
  methods** (`src/document_matcher.rs:27-91`). The builders imply value
  semantics; the pub fields permit post-construction mutation and struct-literal
  construction, which `#[non_exhaustive]` or privacy would close. Low impact at
  current size; worth deciding deliberately. Rule: `api-builder-pattern`.
- **A4 (Note): `LspTransportRead`/`LspTransportWrite` expose tokio half types
  in public variants.** This is the crate's deliberate transport extension
  point; acceptable under `api-std-types-boundary`'s "unless the crate is an
  intentional part of the contract" carve-out. No change recommended.
- **A5 (Note): `ServerError` is a single crate-wide error enum** covering
  transport, RPC, walk, and I/O failures. `err-canonical-struct` warns against
  crate-wide catch-alls; at this crate's size one error type behind one
  `ServerResult` alias is a reasonable trade-off. Revisit if it grows.
  [Judgment]

---

## Dimension B — Error handling audit

### B0. thiserror research (docs.rs, v2.0.20 as pinned)

Facts taken from the official documentation:

- `#[derive(Error)]` generates `Display` (from `#[error("...")]`, with `{var}`
  / `{0}` / `{:?}` shorthand) and `std::error::Error`; thiserror deliberately
  does not appear in the public API.
- `#[from]` generates a `From` impl and **always implies `#[source]`**; the
  variant must contain nothing beyond the source (and a backtrace).
- `#[error(transparent)]` forwards `Display` **and** `source()` to the inner
  error; documented uses: an "anything else" variant
  (`Other(#[from] anyhow::Error)`) and hiding a representation behind an
  opaque public type.
- The docs explicitly point to `anyhow` for application code — the
  library/application split this crate already follows (thiserror here).

### Findings

- **B1 (Important, I1): the `Unknown(String)` catch-all destroys error
  chains.** `ServerError::unknown` stringifies (`src/result.rs:47-49`), and
  `From<String>/&str/&String/BoxDynError` all funnel into `Unknown(String)`
  (`src/result.rs:59-81`). Real production paths use it:
  `src/workspace_walker.rs:66` (`entry.map_err(ServerError::unknown)`) and
  `src/workspace_walker.rs:91` (`ServerError::from(format!(...))`). A walk
  I/O failure arrives downstream as an untyped string with no `source()`.
  thiserror's documented answer for the "anything else" slot is
  `#[error(transparent)] Other(#[from] BoxDynError)` — chain-preserving and
  addition-friendly. Rules: `err-source-chain`, `err-thiserror-lib`,
  `err-canonical-struct`.
- **B2 (Important, I3): `Encoding::from_lsp` panics on an unrecognized
  encoding kind** (`src/text_utils/encoding.rs:75`, documented `# Panics`).
  The input comes from client `initialize` capabilities; the LSP
  `PositionEncodingKind` is a string-backed open type, so a value outside
  UTF-8/16/32 can arrive from a newer/buggy client. [Inference on wire
  openness, based on the type's string-backed shape] The panic is
  client-triggerable in the `initialize` path. Rules: `api-parse-dont-validate`
  (convert boundary data fallibly), `err-result-over-panic`. Fix direction:
  filter unknown encodings during negotiation instead of panicking on
  conversion.
- **B3 (Important, I4): one unreadable entry aborts the whole workspace
  scan.** `WorkspaceWalker::files()` propagates the first walk error
  (`src/workspace_walker.rs:66`), so a single permission-denied directory
  kills all workspace diagnostics. Rule: `api-dir-enumeration` ("treat a
  directory walk as a stream of fallible entries" — skip and trace, don't
  fail the stream).
- **B4 (Important, I5): `Document::query` silently swallows invalid query
  strings.** `Query::new(lang, query).ok()?` (`src/document.rs:178`) turns a
  malformed tree-sitter query into `None` with zero signal — a downstream
  developer error becomes "no captures" silently. This is a public API. Rule:
  swallowed-error hygiene (no-workarounds spirit); fix direction: `debug!`
  tracing under the `tracing` feature, mirroring the invalid-glob handling at
  `src/document_matcher.rs:117-124` (which does this right).
- **B5 (Important, I6): `didChange` fallback can drop an open document.**
  When incremental application fails and the disk re-read also fails, the
  document is removed from the store (`src/server_state.rs:506-510`) — the
  editor still considers it open, but subsequent requests see `None`, with no
  trace emitted. Separately, the fallback replaces the in-memory buffer with
  *disk* content, which discards unsaved editor changes; the sync-handler
  constraint motivating this is documented in comments and CLAUDE.md, the
  data-loss consequence is not. Fix direction: keep last-known text on re-read
  failure (instead of removing) + trace both branches.
- **B6 (Important, I7): fire-and-forget client requests drop results without
  tracing.** `let _ = state.client().request::<RegisterCapability>(...)`
  (`src/workspace_diagnostics.rs:277`) and the diagnostics-refresh sibling at
  `:339`. If registration/refresh fails, the feature silently misbehaves.
  Fix direction: log the error under the `tracing` feature (the codebase
  already has the positive pattern at `document_matcher.rs:118-123`).
- **B7 (Important, I8): public trait `RangeExt` ships default methods that
  `unimplemented!()`** (`src/text_utils/range_ext/mod.rs:127,173`) with
  `#[allow(unused_variables)]`. Any type implementing `RangeExt` without the
  specialized impls inherits panicking methods; the `# Panics` sections
  document delimiter preconditions but not this. Rules: `err-result-over-panic`,
  `doc-panics-section`. Fix direction: remove the defaults (make the methods
  required) or document the per-type contract explicitly.
- **B8 (Minor, M1): `TcpConnect` discards the connect error's cause.**
  `.map_err(|_| ServerError::TcpConnect(port))` (`src/transport.rs:59`) loses
  refused-vs-timeout-vs-DNS detail at exactly the failure users debug most.
  Rule: `err-source-chain`. Fix direction: carry `#[source] std::io::Error`
  in the variant.
- **B9 (Minor, M3/M4): Display message conventions.** Messages are
  capitalized ("Failed to connect to port…", `src/result.rs:29`) contra
  `err-lowercase-msg`; `Rpc`'s Display shows only the code, never the message
  (`src/result.rs:35-36`). The doctest at `src/result.rs:17-25` asserts the
  current strings, so a fix updates it too.
- **B10 (Note): the `From<ServerError> for ResponseError` edge mapping is
  sound** (`src/result.rs:85-95`): `Rpc` → its code, `Unknown` →
  `UNKNOWN_ERROR_CODE`, the rest → `INTERNAL_ERROR` with `to_string()`. This
  matches `err-edge-mapping` (map at the boundary; the wire type cannot carry
  a source chain anyway). Possible refinement: map `Io` kinds to more specific
  codes. No change required.

### Positive findings (error handling)

- **Zero `unwrap()` in non-test `src/` code.** The only non-test panics are
  two documented ones (`transport.rs:73` `# Panics` section is exemplary;
  `encoding.rs:63` documented) and the `RangeExt` defaults above; one
  invariant `expect` with a contract message (`server_state.rs:489`, see C
  below).
- **`# Errors` coverage on the public fallible surface is complete**: every
  public fallible item (`serve`, `Transport::into_read_write`,
  `oneshot::workspace_diagnostics`) has `# Errors`; `# Panics` present on all
  public panicking items (`document.rs:139`, `encoding.rs:63`,
  `range_ext`, `transport.rs:48`).
- **`oneshot` boundary mapping is clean**: `ResponseError → ServerError::rpc`
  preserves code and message (`src/oneshot/server.rs:119-121`).

---

## Dimension C — Correctness spot-audit

- **C1 (Critical): clippy gate red** — see battery table. Three `unused_async`
  errors. Fix is mechanical (drop `async`, return `std::future::ready(...)`,
  valid under RPITIT), but note it interacts with the module-level allow on
  `server_trait.rs` (M5): the trait defaults should ideally drop `async` too
  and shed the allow. CI (`rust.yml`) runs this command on push — the gate
  would fail today.
- **C2 (Important, I2, previously known — now formalized): the `*_resolve`
  request hooks never run.** `implement_method!` calls `modify_response` only
  when `extract_url` returns `Some` (`src/server_with_state.rs:51-59,68-84`);
  `CompletionResolve` and `CodeActionResolve` have no URL (comments at
  `src/requests.rs:361,421`), so their `modify_response` — which converts
  text-edit ranges — is dead code, and resolve responses leave in UTF-8
  positions under a UTF-16 client. Related: `Request` trait carries
  `#[allow(dead_code)]` (`src/requests.rs:32`). Status: known, unspecced.
  Natural home: the same URL-less conversion machinery the symbols work
  introduces (parked spec `2026-08-28-symbols-design.md` §4).
- **C3 (previously known, specced): workspace/diagnostic ranges are not
  converted** to the negotiated encoding (no conversion calls in
  `src/workspace_diagnostics.rs`). Specced for fix in
  `docs/superpowers/specs/2026-08-28-symbols-design.md` §5 — owner-approved.
- **Invariant check (positive): `doc_parser(doc).expect("has tree - must have
  parser")`** (`src/server_state.rs:489`) holds under current code: a
  `tree_sitter_tree` is only ever set alongside `tree_sitter_lang`
  (insert/parse paths), so the guard at `:487` implies `doc_parser` returns
  `Some`. Acceptable per `err-expect-bugs-only`; the message states the
  contract. Fragile only if future code sets a tree without a language.
- **LSP inventories matched ground truth** — e.g. `ServerState`'s public
  surface (`client`, `document`, `documents` at `src/server_state.rs:58-85`,
  all `#[must_use]`) is coherent; the `documents()` iteration method is public
  while `document_urls()`/`workspace_roots()` are `pub(crate)` — a deliberate
  Documents-not-URLs exposure. [Inference on deliberateness]
- `position_to_encoding` same-encoding early return (`src/text_utils/
  conversions.rs:31-33`) makes the `_ => unreachable!()` at `:70` genuinely
  unreachable — correct as written.

---

## Dimension D — Documentation accuracy

- **D1 = A1/I9**: README (the rendered crate docs) omits the crate's actual
  API and keeps upstream's first-person voice. Highest-leverage docs fix.
- **D2 (Minor)**: `Transport::into_read_write`'s `# Errors` says "the port is
  not valid" (`src/transport.rs:45`); a `u16` port is always representable in
  a `SocketAddr` — the real error is connect failure. Wording fix only.
- **D3 (positive)**: doc hygiene otherwise strong — `missing_docs` enforced
  and passing, doctests run green in all three feature configurations, intra-
  doc links used (`[`ServerError::Unknown`]` etc.), examples compile (modulo
  C1's lint).

---

## Findings register

| ID | Severity | Location | Summary | Rules |
|---|---|---|---|---|
| C1 | Critical | `examples/minimal.rs:40`, `examples/tree_sitter.rs:47`, `src/oneshot/workspace_diagnostics.rs:323` | clippy `-D warnings` fails: `unused_async` ×3; CI gate red | tech.md battery |
| I1 | Important | `src/result.rs:33,47-81`, `src/workspace_walker.rs:66,91` | `Unknown(String)` stringly catch-all; source chains destroyed | `err-source-chain`, `err-thiserror-lib`, thiserror docs |
| I2 | Important | `src/server_with_state.rs:68-84`, `src/requests.rs:355-433` | `*_resolve` `modify_response` never runs; resolve responses unconverted (known, now formalized) | — |
| I3 | Important | `src/text_utils/encoding.rs:75` | panic on unrecognized client position encoding in `initialize` path | `api-parse-dont-validate`, `err-result-over-panic` |
| I4 | Important | `src/workspace_walker.rs:66` | single walk error aborts entire workspace scan | `api-dir-enumeration` |
| I5 | Important | `src/document.rs:178` | invalid tree-sitter query silently → `None` (public API, no signal) | no-workarounds spirit |
| I6 | Important | `src/server_state.rs:496-511` | `didChange` fallback drops open doc on disk-read failure; untraced; unsaved-buffer hazard undocumented | observability |
| I7 | Important | `src/workspace_diagnostics.rs:277,339` | fire-and-forget client requests; failures invisible | observability |
| I8 | Important | `src/text_utils/range_ext/mod.rs:124-131,165-177` | public trait default methods `unimplemented!()` | `err-result-over-panic`, `doc-panics-section` |
| I9 | Important | `README.md` | rendered crate docs omit the API; no example; upstream voice | `doc-crate-readme` |
| M1 | Minor | `src/transport.rs:59` | `TcpConnect` discards io::Error cause | `err-source-chain` |
| M2 | Minor | `src/result.rs:27` vs `src/transport.rs:31` | `#[non_exhaustive]` inconsistency | `api-non-exhaustive` |
| M3 | Minor | `src/result.rs:29,32,35` | Display messages capitalized; contra `err-lowercase-msg` | `err-lowercase-msg` |
| M4 | Minor | `src/result.rs:35-36` | `Rpc` Display hides the message field | — |
| M5 | Minor | `server_trait.rs:1-4`, `server_state.rs:1-2,40,96,117,516`, `result.rs:1`, `server_with_state.rs:280,450`, `tree_sitter_utils.rs:21`, `range_ext/mod.rs:124,165` | lint-allow inventory: 7 module-level + ~10 inline allows; `server_trait`'s four are the broadest masks (one hides C1's lint crate-internally) | tech.md allows-are-debt |
| M6 | Minor | `src/document_matcher.rs:27-91` | pub fields + builder duality on `DocumentMatcher` | `api-builder-pattern` |
| M7 | Minor | `src/result.rs:85-95` | edge mapping could differentiate `Io` kinds | `err-edge-mapping` |
| M8 | Minor | `src/transport.rs:45` | `# Errors` wording: "port is not valid" unreachable | docs |

Known items folded in: I2 (was known-unspecced), C3 (specced, symbols work).

---

## Ranked backlog (candidates — owner picks)

| # | Item | Findings | Effort |
|---|---|---|---|
| 1 | Make clippy green: de-`async` the three impls (`std::future::ready`), align trait defaults, start shedding `server_trait.rs` allows | C1, M5 | S |
| 2 | Error-model rework: chain-preserving `Other(#[from] BoxDynError)` (or typed variants) replacing `Unknown(String)`; carry `#[source]` in `TcpConnect`; update doctest | I1, M1, M3, M4 | M |
| 3 | `didChange` fallback: keep last-known text on re-read failure + tracing; document the unsaved-buffer trade-off | I6 | S–M |
| 4 | Negotiation without panic: filter unknown client encodings | I3 | S |
| 5 | Walker resilience: skip-and-trace per entry | I4 | S |
| 6 | `Document::query`: trace invalid queries (mirror glob handling) | I5 | S |
| 7 | Tracing on fire-and-forget client requests | I7 | S |
| 8 | README rewrite: API tour + example + fork voice | I9 | M |
| 9 | `RangeExt`: remove panicking defaults or document per-type contract | I8 | M |
| 10 | `#[non_exhaustive]` pass on public enums (`ServerError`, transport halves) | M2 | S |
| 11 | Fold I2 (resolve conversion) into the symbols cycle's URL-less machinery | I2 | (with symbols) |
| 12 | Remaining minors: M6–M8 | M6–M8 | S each |

---

## Provenance

- Tools: LSP `documentSymbol` (5 files), targeted reads, grep sweeps, one
  clippy rerun with corrected exit capture, battery run (background), docs.rs
  thiserror 2.0.20 fetched via web-reader.
- rust-skills rules cited: `err-thiserror-lib`, `err-canonical-struct`,
  `err-edge-mapping`, `err-source-chain`, `err-lowercase-msg`,
  `err-result-over-panic`, `err-expect-bugs-only`, `api-non-exhaustive`,
  `api-builder-pattern`, `api-std-types-boundary`, `api-parse-dont-validate`,
  `api-dir-enumeration`, `doc-errors-section`, `doc-panics-section`,
  `doc-crate-readme`.
- Speculative items are labeled inline ([Inference]/[Judgment]); everything
  else is verified against the cited file:line at HEAD `c7e86df`.
