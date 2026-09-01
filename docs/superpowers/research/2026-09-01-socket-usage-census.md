# Socket usage census (rm-socket stage 1, task 2)

Date: 2026-09-01 · Tree: 46c17bc + the deprecation marks · Branch: `feature/rm-socket`

Census of every socket-transport usage site, gathered from three independent
finders so that no single tool's blind spot defines the removal surface:

1. **Compiler sweep** — `cargo build --all-targets` with Task 1's
   `#[deprecated]` marks: 57 warning rows, **56 unique `file:line:col`
   sites**. (The extra row: `src/transport.rs:43:6` is emitted twice —
   identical message and span — as a cargo multi-unit artifact. It appears
   once in a `--lib`-only build; the duplicate shows up in the `--tests`
   pass. Which exact units re-emit it: [Unverified]; the site/message
   identity is verified.)
2. **LSP findReferences** (rust-analyzer) on the five deprecation anchors:
   `Transport`, `LspTransportRead`, `LspTransportWrite`,
   `ServerError::TcpConnect`, `serve` — 53 references (definitions included).
3. **Targeted greps** — TCP types in code, tokio features in the manifest,
   socket/TCP prose in current-truth docs.

Both linter-class finders share one blind spot: **doctests**. rustdoc
suppresses the `deprecated` lint and `--all-targets` never builds them, so
the deprecated-using doctest lines are enumerated manually below and marked
`[manual — doctest lint blind spot]`.

Classes: `production` (lib code) / `example` (`[[example]]` targets) /
`test` / `doc` (doctest code and prose) / `manifest`.
Dispositions are stage-2 suggestions, not decisions: `delete` (site dies
with the removed item/file), `rewrite-via` (call site must switch to the
post-removal shape), `update-text` (prose/docs), `keep` (examined, not a
removal target).

---

## Axis 1 — LSP census (`findReferences`)

### `Transport` (enum, `src/transport.rs:35`) — 17 refs

| file:line:col | class | what it is | disposition |
|---|---|---|---|
| src/transport.rs:35:10 | production | definition | delete |
| src/transport.rs:43:6 | production | `impl Transport` header | delete |
| src/transport.rs:57:16 | production | `Self::Socket(port)` pattern in `into_read_write` | delete |
| src/transport.rs:70:23 | production | `Self::Stdio` pattern in `into_read_write` | delete |
| src/transport.rs:81:23 | production | `impl fmt::Display for Transport` header | delete |
| src/transport.rs:84:13 | production | Display match arm `Self::Stdio` | delete |
| src/transport.rs:85:13 | production | Display match arm `Self::Socket` | delete |
| src/server/mod.rs:30:27 | production | `pub use crate::transport::Transport;` re-export | delete |
| src/server/serve.rs:15:16 | production | `use ...transport::Transport;` import | delete (with serve signature rewrite) |
| src/server/serve.rs:56:34 | production | `transport: Transport` param of `serve` | rewrite-via (signature) |
| examples/minimal.rs:15:72 | example | import | rewrite-via |
| examples/minimal.rs:87:11 | example | `serve(Transport::Stdio, ...)` arg | rewrite-via |
| examples/tree_sitter.rs:17:70 | example | import | rewrite-via |
| examples/tree_sitter.rs:97:11 | example | `serve(Transport::Stdio, ...)` arg | rewrite-via |
| tests/lsp_wire.rs:6:72 | test | import | delete (with file) |
| tests/lsp_wire.rs:115:45 | test | `serve(Transport::Socket(port), ...)` | delete (with file) |
| tests/lsp_wire.rs:130:43 | test | `serve(Transport::Socket(port), ...)` | delete (with file) |

### `LspTransportRead` (enum, `src/transport.rs:95`) — 8 refs, all `src/transport.rs`

| file:line:col | class | what it is | disposition |
|---|---|---|---|
| src/transport.rs:95:10 | production | definition | delete |
| src/transport.rs:56:57 | production | return type of `into_read_write` | delete |
| src/transport.rs:67:17 | production | `LspTransportRead::Socket(stream_read)` | delete |
| src/transport.rs:72:17 | production | `LspTransportRead::Stdio(stdin)` | delete |
| src/transport.rs:102:20 | production | `impl AsyncRead for LspTransportRead` header | delete |
| src/transport.rs:104:24 | production | `self: Pin<&mut Self>` in `poll_read` | delete |
| src/transport.rs:111:13 | production | match arm `Self::Socket` (AsyncRead) | delete |
| src/transport.rs:112:13 | production | match arm `Self::Stdio` (AsyncRead) | delete |

### `LspTransportWrite` (enum, `src/transport.rs:128`) — 14 refs, all `src/transport.rs`

| file:line:col | class | what it is | disposition |
|---|---|---|---|
| src/transport.rs:128:10 | production | definition | delete |
| src/transport.rs:56:75 | production | return type of `into_read_write` | delete |
| src/transport.rs:68:17 | production | `LspTransportWrite::Socket(stream_write)` | delete |
| src/transport.rs:73:17 | production | `LspTransportWrite::Stdio(stdout)` | delete |
| src/transport.rs:135:21 | production | `impl AsyncWrite for LspTransportWrite` header | delete |
| src/transport.rs:136:34 | production | `self: Pin<&mut Self>` in `poll_write` | delete |
| src/transport.rs:138:13 | production | match arm `Self::Socket` (poll_write) | delete |
| src/transport.rs:139:13 | production | match arm `Self::Stdio` (poll_write) | delete |
| src/transport.rs:143:34 | production | `self: Pin<&mut Self>` in `poll_flush` | delete |
| src/transport.rs:145:13 | production | match arm `Self::Socket` (poll_flush) | delete |
| src/transport.rs:146:13 | production | match arm `Self::Stdio` (poll_flush) | delete |
| src/transport.rs:150:34 | production | `self: Pin<&mut Self>` in `poll_close` | delete |
| src/transport.rs:152:13 | production | match arm `Self::Socket` (poll_close) | delete |
| src/transport.rs:153:13 | production | match arm `Self::Stdio` (poll_close) | delete |

### `ServerError::TcpConnect` (variant, `src/error.rs:88`) — 5 refs + 1 review-added compiler site

| file:line:col | class | what it is | disposition |
|---|---|---|---|
| src/error.rs:88:5 | production | definition | delete |
| src/transport.rs:62:47 | production | construction in `into_read_write` | delete |
| src/error.rs:148:34 | test | unit test `tcp_connect_preserves_its_source` | delete |
| src/error.rs:210:57 | test | unit test `tcp_connect_maps_to_internal_error` | delete |
| tests/lsp_wire.rs:120:38 | test | failure-path assertion | delete (with file) |
| tests/lsp_wire.rs:120:51 | test | field binding `{ port: p, .. }` in the same assertion | delete (with file) |

### `serve` (fn, `src/server/serve.rs:56`) — 9 refs

| file:line:col | class | what it is | disposition |
|---|---|---|---|
| src/server/serve.rs:56:14 | production | definition | rewrite-via (stdio-only signature or equivalent — stage-2 call) |
| src/server/mod.rs:24:22 | production | `pub use self::serve::serve;` re-export | rewrite-via (stays, shape may change) |
| examples/minimal.rs:15:83 | example | import | rewrite-via |
| examples/minimal.rs:87:5 | example | call | rewrite-via |
| examples/tree_sitter.rs:17:81 | example | import | rewrite-via |
| examples/tree_sitter.rs:97:5 | example | call | rewrite-via |
| tests/lsp_wire.rs:6:83 | test | import | delete (with file) |
| tests/lsp_wire.rs:115:39 | test | call | delete (with file) |
| tests/lsp_wire.rs:130:37 | test | call | delete (with file) |

Related site the five anchors do not surface (found while reading; not a
deprecated-use warning — a non-deprecated method called through a value of
deprecated type): `src/server/serve.rs:61` calls
`transport.into_read_write()`. It cannot survive the `transport.rs` deletion
and is covered by the `serve` rewrite; listed here so stage 2 does not meet
it unawares. The same applies to the `reader`/`writer` flow into
`run_over_streams` at `src/server/serve.rs:62` — `run_over_streams` itself
(`pub(crate)`, stream-generic, TCP-free) is unaffected and is the seam the
W2 wire tests ride on.

---

## Axis 2 — Cross-check: compiler sweep ↔ LSP

55 sites enumerated at review time; the 56th (`lsp_wire.rs:120:51`, a
field-binding site) was caught by review and added above. After the
addition, all 56 unique sites are tabulated (same `file:line`), and
vice versa except the resolved categories below. **Unresolved mismatches:
0.**

Column deltas are systematic, not discrepancies: for `serve(...)` calls
rustc anchors the callee name (same as LSP); for enum/variant paths rustc
anchors the **final path segment** (`Socket`/`Stdio`) and, when a pattern
binds through the variant, the **field** too, while rust-analyzer anchors
the type/`Self` token. E.g. line 57 `if let Self::Socket(port) = self`:
compiler 57:22 (variant) + 57:29 (field); LSP 57:16 (`Self`).

Resolved discrepancies (each verified by reading the site):

1. **`src/transport.rs:40:5` — compiler-only.** The `#[default] Stdio`
   variant: the `#[derive(Default)]` expansion's use of the deprecated
   variant is attributed to the variant's own span. LSP records no reference
   there (no name token). No stage-2 action beyond deleting the enum.
2. **`src/transport.rs:104:24`, `136:34`, `143:34`, `150:34` — LSP-only.**
   `self: Pin<&mut Self>` inside the deprecated enums' own `AsyncRead` /
   `AsyncWrite` impls: rust-analyzer counts `Self`-type positions;
   rustc does not warn for uses inside the deprecated item's own impls.
3. **Definition sites — LSP-only by design** (`transport.rs:35:10`, `95:10`,
   `128:10`; `error.rs:88:5`; `serve.rs:56:14`): definitions are not uses,
   so the compiler never warns there.
4. **`src/transport.rs:43:6` double row** (57 rows / 56 unique sites): same
   message, same span, re-emitted across cargo's unit builds under
   `--all-targets`. Emitted exactly once by `cargo build --lib`; the
   duplicate appears in the `--tests` pass. Unit-level attribution of the
   re-emission: [Unverified].
5. **Doctest blind spot** — invisible to both finders; manual rows:

| file:line | class | what it is | disposition |
|---|---|---|---|
| src/transport.rs:25 `[manual — doctest lint blind spot]` | doc | doctest import of `Transport` | delete (with file) |
| src/transport.rs:27 `[manual — doctest lint blind spot]` | doc | `Transport::Stdio.to_string()` assertion | delete (with file; the Display doctest dies with it — no assertion survives unless stage 2 keeps a Display equivalent) |
| src/transport.rs:28 `[manual — doctest lint blind spot]` | doc | `Transport::Socket(9999)` Display assertion | delete |
| src/error.rs:74 `[manual — doctest lint blind spot]` | doc | `ServerError::TcpConnect { ... }` in the enum's example | rewrite-via (re-point the example at a surviving variant) |
| src/server/serve.rs:39 `[manual — doctest lint blind spot]` | doc | doctest import of `{Transport, serve}` | rewrite-via |
| src/server/serve.rs:45 `[manual — doctest lint blind spot]` | doc | `serve(Transport::Stdio, MyServer)` in the `no_run` example | rewrite-via |

---

## Axis 3 — Dependency axis

TCP/socket code (`grep -rn "tokio::net\|TcpStream\|TcpListener\|SocketAddr" src/ examples/ tests/`):

| file:line | class | what it is | disposition |
|---|---|---|---|
| src/transport.rs:4 | production | `use std::net::SocketAddr` (**std**, not tokio) | delete (with file) |
| src/transport.rs:13–14 | production | `tokio::net::{TcpStream, tcp::{OwnedReadHalf, OwnedWriteHalf}}` | delete (with file) |
| src/transport.rs:58 | production | `SocketAddr::from(([127,0,0,1], port))` | delete (with file) |
| src/transport.rs:60 | production | `TcpStream::connect(addr)` | delete (with file) |
| tests/lsp_wire.rs:10 | test | `use tokio::net::TcpStream` | delete (with file) |
| tests/lsp_wire.rs:42 | test | `BufStream<TcpStream>` field (BufStream = `tokio::io`, io-util) | delete (with file) |
| tests/lsp_wire.rs:111 | test | `tokio::net::TcpListener::bind("127.0.0.1:0")` | delete (with file) |
| tests/lsp_wire.rs:127 | test | `tokio::net::TcpListener::bind("127.0.0.1:0")` | delete (with file) |

**Nothing outside `src/transport.rs` (production) and `tests/lsp_wire.rs`
(test) touches TCP types.** `examples/` and the rest of `src/` are clean.

Tokio feature delta — read from `Cargo.toml` (verified, not assumed):

| surface | today | stage-2 delta |
|---|---|---|
| `[dependencies] tokio` | `["io-std", "io-util", "net", "rt"]` | **remove `net`**; keep `io-std` (`Stdin`/`Stdout` in transport.rs's Stdio arms — the Stdio halves move, they don't vanish) and `rt` |
| `[dependencies] tokio` `io-util` | provided by main list only | lib consumer today is `src/transport.rs:11` (dies); surviving consumer is the W2 harness (`src/server/tests.rs:19` Ext traits, `:239 tokio::io::duplex`) — cfg(test), so io-util must stay in the feature union; today's sole provider is the main list. Suggestion: move `io-util` to `[dev-dependencies] tokio` and re-verify with a post-deletion grep. [Inference] on the final placement, verified facts as stated |
| `[dev-dependencies] tokio` | `["rt", "rt-multi-thread", "macros", "time", "sync", "fs"]` | **no `net`, no `io-std` — nothing to remove** (expectation confirmed); possibly `+io-util` per the row above |
| `async-lsp` features | `["client-monitor", "omni-trait"]` (+ `tracing` via crate feature) | no socket/TCP feature involved — **no delta**; this crate owns its transport, async-lsp's tokio transports are unused |

---

## Axis 4 — Docs/text axis

`grep -rin "socket\|tcp" CLAUDE.md .claude/rules/ README.md examples/`
(11 hits). `docs/superpowers/**` and `.superpowers/**` are **excluded by
decision**: specs, plans, briefs, and research notes are history — they
record what was decided, not what is true, and must not be edited to track
the removal.

| file:line | class | text | disposition |
|---|---|---|---|
| CLAUDE.md:7 | doc | "tokio stdio/TCP transports" (crate description) | update-text |
| CLAUDE.md:22 | doc | "over a `Transport` (`Stdio` default, or `Socket(port)`)" | update-text |
| CLAUDE.md:51 | doc | "closed `ClientSocket`" | **keep** — async-lsp's `ClientSocket` request channel, not the TCP transport |
| .claude/rules/structure.md:21 | doc | "`Transport` (`Stdio`, or `Socket(port)`)" | update-text |
| .claude/rules/structure.md:103 | doc | "closed `ClientSocket`" | **keep** (same as CLAUDE.md:51) |
| .claude/rules/product.md:5 | doc | "stdio/TCP transports" | update-text |
| README.md:4 | doc | "tokio stdio/TCP transports" (README is crate-level docs via `include_str!`) | update-text |
| README.md:70 | doc | "over stdio or a TCP socket" | update-text |
| .claude/rules/testing.md:27 | doc | W3 tier row: "`serve()` + `Transport::Socket` over real TCP" | update-text (tier's fate is stage 2's call; the row describes removed machinery) |
| .claude/rules/tech.md:95 | doc | "`ClientSocket`" in the dependency list | **keep** (channel type) |
| .claude/rules/error-handling.md:39 | doc | `TcpConnect { ... }` in the named-constructor snippet | update-text (pick a surviving variant for the illustration) |

Supplementary prose rows in surviving code (same axis, found by reading /
a src/-scoped grep the axis command didn't cover):

| file:line | class | text | disposition |
|---|---|---|---|
| .dupes-ignore.toml:71 | manifest | ignore-entry reason naming `LspTransportWrite`'s Socket/Stdio forwarding | update-text (dead config after stage 2) |
| src/server/serve.rs:51 | doc | `# Errors` bullet "If the transport uses a socket and it could not connect" | update-text (serve survives its rewrite; the bullet doesn't) |
| src/error.rs:83 | doc | variant doc "Failed to connect a socket to the given TCP port." | delete (with variant) |
| src/error.rs:86, src/server/serve.rs:54 | doc | the `#[deprecated]` notes themselves | delete with their items (serve's note goes in the rewrite) |
| src/transport.rs:20, 36, 48, 96, 98, 129, 131 | doc | socket/TCP doc comments inside transport.rs | delete (with file) |

---

## Closing counts (what stage 2 needs)

- **`serve()` call sites: 5** — `examples/minimal.rs:87`,
  `examples/tree_sitter.rs:97`, `tests/lsp_wire.rs:115`, `:130`, and the
  doctest `src/server/serve.rs:45`. Import/re-export rows for `serve`
  (4 + 1 doctest import) are adjacent but not calls.
- **TCP types outside `transport.rs`:** only `tests/lsp_wire.rs`
  (`TcpStream` :10/:42, `TcpListener` :111/:127). No production file other
  than `src/transport.rs`, no example touches TCP directly.
- **Tokio feature delta:** `[dependencies]` −`net`; `io-std` and `rt` stay;
  `io-util` must remain in the union (W2 harness), provider placement
  pending stage 2; `[dev-dependencies]` has no `net`/`io-std` today —
  nothing to remove there; `async-lsp` features unchanged.
- **Removal surface in one line:** `src/transport.rs` (157 lines, wholesale)
  + 2 re-export lines (`src/server/mod.rs:24`, `:30`) + the `serve` signature
  (and its `Transport` import, `into_read_write` call, `# Errors` bullet)
  + `tests/lsp_wire.rs` (wholesale) + 4 example rows + 6 doctest lines +
  5 serve call-site fixes + tokio `net` + 8 truth-doc text updates.

## Synthesis

The socket surface is almost perfectly encapsulated: one module
(`src/transport.rs`) owns every TCP type, both half-transports, and the only
direct `TcpStream`/`TcpListener` code outside the W3 integration test; the
compiler's 56 unique sites, rust-analyzer's 53 references, and the grep axes
tell one non-contradictory story once five systematic finder differences are
resolved (derive-expansion spans, `Self`-positions inside the deprecated
items' own impls, definition sites, one cargo double-emission, and the
shared doctest blind spot patched with six manual rows). What the
encapsulation does *not* contain is the API shape: `Transport` leaks through
`serve()`'s signature into both examples and both W3 tests, and through the
two `pub use` lines in `src/server/mod.rs` — so the real stage-2 work is the
`serve()` rewrite (or its replacement by `run_over_streams`-shaped API),
the `tests/lsp_wire.rs`/W3 decision (its replacement brainstorm is already
parked separately), and the manifest's `net` removal. The W2 wire tier
(`run_over_streams` over `tokio::io::duplex`) never touches TCP and
survives untouched, which is what keeps the tested middleware stack from
drifting after the transport layer goes.

## Verification (this task)

- `cargo test` (default features): all four test targets `ok` —
  1 + 13 + 143 + 2 = 159 passed, 0 failed. No code changed by this task.
- `cargo build --all-targets 2>&1 | grep -c "warning.*deprecated"` → **57**,
  matching the compiler-row count above (56 unique sites).
