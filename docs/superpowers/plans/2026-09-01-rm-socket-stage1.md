# Socket removal stage 1 (deprecation + census) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deprecate the four socket-coupled public items, run the two-finder usage census plus dependency/docs axes, and commit the census report that the stage-2 architecture decision will rest on.

**Architecture:** Deprecation marks are transient scaffolding: they make `-D warnings` flag every internal use, turning the compiler into a completion checklist. The census cross-checks LSP `findReferences` against those compiler warnings and adds the two axes neither covers (tokio features, prose/examples). No behavior changes.

**Tech Stack:** rustc `#[deprecated]`, LSP findReferences, cargo, grep.

**Spec:** `docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md`

## Global Constraints

- The deprecation note text is EXACTLY: `sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md`
- Marks change NO behavior; while marks are in place the `-D warnings` battery is EXPECTED to fail (the intended transient state recorded in the spec) — census commands run WITHOUT `-D`.
- Owner's rule: no fallbacks, no socket mentions left in tests, no socket-only dependencies — the census CLASSIFIES for stage 2; nothing is removed in this stage.
- **Git is read-only for agents** (hook): tasks end with a suggested commit message; the owner commits. Never `git add`/`git commit`.
- **Navigation is LSP-first** (findReferences via the LSP tools); grep only for literal text. English artifacts; every census entry cites file:line.

---

### Task 1: Deprecation marks

**Files:**
- Modify: `src/transport.rs`, `src/error.rs`, `src/server/serve.rs`

**Interfaces:**
- Produces: `#[deprecated]` on `Transport`, `LspTransportRead`, `LspTransportWrite`, `ServerError::TcpConnect`, and `serve()` — the anchors Task 2's findReferences and the compiler sweep both key on.

- [ ] **Step 1: Mark the transport items** — `src/transport.rs`: on the `Transport` enum, `LspTransportRead`, and `LspTransportWrite`:

```rust
#[deprecated(note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md")]
```

(The doc examples inside `transport.rs` that construct `Transport::Socket(9999)` will now warn; leave them — Task 3 records them as census rows.)

- [ ] **Step 2: Mark the error variant** — `src/error.rs`, on the `TcpConnect` variant (variant-position attribute):

```rust
#[deprecated(note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md")]
TcpConnect { ... }
```

- [ ] **Step 3: Mark `serve()`** — `src/server/serve.rs`, on `pub async fn serve`:

```rust
#[deprecated(note = "sockets are being removed; see docs/superpowers/specs/2026-09-01-rm-socket-stage1-design.md")]
```

- [ ] **Step 4: Capture the compiler census** (the completeness proof):

```bash
cargo build --all-targets 2>&1 | grep -B1 -A3 "deprecated" > /tmp/deprecation-warnings.txt
wc -l /tmp/deprecation-warnings.txt
```

Expected: warnings at every internal use — `serve.rs` (into_read_write/TcpConnect path), the `serve()` doctest, `examples/minimal.rs` + `examples/tree_sitter.rs` (serve call sites), `tests/lsp_wire.rs` (Socket + TcpConnect asserts), `src/error.rs` unit tests + doctest (TcpConnect constructions), `src/transport.rs` doctest. Zero behavior change: `cargo test` (without `-D`) still fully green.

- [ ] **Step 5: Report for commit**

Suggested: `chore: deprecate socket-coupled items ahead of removal (stage 1)`

---

### Task 2: LSP census + dependency/docs axes

**Files:**
- Create: `docs/superpowers/research/2026-09-01-socket-usage-census.md`

**Interfaces:**
- Consumes: Task 1's marks (findReferences anchors) + `/tmp/deprecation-warnings.txt` (completeness cross-check).
- Produces: the committed census report — one table per axis with columns `symbol | file:line | class (production/test/doc/manifest) | stage-2 disposition suggestion`.

- [ ] **Step 1: LSP findReferences census** — run LSP `findReferences` on each of: `Transport`, `LspTransportRead`, `LspTransportWrite`, `ServerError::TcpConnect`, `serve`. Record every reference with file:line and its class.

- [ ] **Step 2: Cross-check against the compiler sweep** — every compiler-flagged site MUST appear in the LSP table and vice versa; any mismatch is a census defect (resolve by re-reading the site; do not guess).

- [ ] **Step 3: Dependency axis** — `grep -rn "tokio::net\|TcpStream\|TcpListener\|SocketAddr" src/ examples/ tests/` plus read `Cargo.toml`'s tokio feature lists (main + dev): record every socket-only dependency surface and the feature delta stage 2 must apply (expectation to VERIFY, not assume: main `net` goes; dev `net` if present goes; `io-std` stays).

- [ ] **Step 4: Docs/text axis** — `grep -ri "socket\|tcp" CLAUDE.md .claude/rules/ README.md examples/ | grep -v Binary` — every prose/example mention becomes a census row with disposition `update-text`.

- [ ] **Step 5: Write the report** — `docs/superpowers/research/2026-09-01-socket-usage-census.md` with the tables, the closing counts stage 2 needs (number of `serve()` call sites; whether anything outside `transport.rs` touches TCP types directly; the full tokio-feature delta), and a one-paragraph synthesis. Label anything uncertain `[Unverified]`.

- [ ] **Step 6: Verify** — `cargo test` (no `-D`) green — the census changed no code; `cargo build --all-targets 2>&1 | grep -c deprecated` matches the report's warning-site count.

- [ ] **Step 7: Report for commit**

Suggested: `docs: socket usage census (deprecation + LSP + dependency/docs axes)`

---

## Self-Review (done at plan time)

- **Spec coverage:** marks on all four items + `serve()` (Task 1); two-finder census cross-checked (Task 2 Steps 1-2); dependency axis incl. tokio feature delta (Step 3); docs axis (Step 4); committed report with classification + closing counts (Step 5); no-behavior-change verification (Task 1 Step 4 / Task 2 Step 6). Out-of-scope honored — nothing is removed.
- **Placeholders:** none; the note text, grep commands, and report columns are exact.
- **Type consistency:** the five findReferences anchors match Task 1's marked items one-for-one.
