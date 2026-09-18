# Invariants Cycle Design

Branch: `feature/invariants` · Date: 2026-09-18 · Status: owner-approved
design, spec pending owner review

Cycle mandate: `.superpowers/sdd/cycle-mandate-ecosystem-invariants.md`
(scratch). Research inputs (committed):

- `docs/superpowers/research/2026-09-18-dua-core-walker-evaluation.md`
- `docs/superpowers/research/2026-09-18-invariant-audits-recon.md`
- `docs/superpowers/research/2026-09-18-rayon-batch-evaluation.md`

## 1. Context

Three research threads opened this cycle: replacing the workspace walker
(`ignore` 0.4.33) with Byron's dua-core, evaluating Rayon for the batch
engine, and re-auditing two framework invariants — capability-gated
activation and non-blocking execution — because past reviews of those
invariants did not always run on the strongest model. The research
verdicts resolved the two engine questions (both keep the incumbent;
see decision records), leaving the audits and a configurable-ignore
feature as the work that ships code:

- **W2** configurable ignore — the only new public surface;
- **W4** capability-gated dispatch — the invariant the audits showed is
  not enforced today (71-row matrix);
- **W5** non-blocking fixes — the 35-row blocking inventory, one real
  offender plus one stale pattern;
- **W6** a disposition table closing the "own machinery vs ecosystem"
  question with evidence.

## 2. Decision records

Ratified by the owner during brainstorming, 2026-09-18.

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Walker stays on `ignore` 0.4.33 | dua-core gate R1 failed absolutely: no ignore machinery in the crate (controller-verified, 0 source matches); adopting meant re-implementing ripgrep-grade ignore semantics. The evaluation doc is the decision record. |
| D2 | `for_each_bounded` stays | Prototype hybrid benched on a 4096-file corpus: delta ≈ 0 inside the 2.5–7 % noise floor, and the hybrid pays a per-refresh `ThreadPool` build plus a batch-unwind panic shape. |
| D3 | Ignore opt-in is configuration-level | No Cargo feature. The dependency-savings argument died with D1 (`ignore` stays for walking regardless); a second non-default feature would re-activate the third CI leg for nothing. Unconfigured ⇒ mechanism fully inert. |
| D4 | Untracked-URL conversion contract kept; blocking removed | Dropping the disk-read fallback degrades multi-byte position conversion for downstream servers (the owner's Markdown LSP is multi-byte-heavy). Keep the contract, move the reads off the executor. |
| D5 | Programmatic ignore override globs deferred | YAGNI: the file-based form covers the need (user excludes, downstream names the file); no current consumer asked for construction-time globs. |
| D6 | `require_git` stays `true`, git-less behavior documented | Ecosystem-consistent (ripgrep, git itself). The git-less gap is closed by custom ignore names (D3 machinery), which are git-independent by design. |
| D7 | Type-hierarchy trio is gate-exempt | `lsp_types` 0.95.1 has no capability fields for `prepare_type_hierarchy`/`supertypes`/`subtypes`; gating them would kill working methods. Always-allowed, recorded as an upstream gap. |

## 3. W2 — Configurable ignore

### 3.1 Public surface (additive)

```rust
impl ServerOptions {
    /// Names of ignore files honored during workspace walks (gitignore
    /// syntax, matched per directory, cascading), e.g. `.mylspignore`.
    /// Session-fixed, like matchers.
    pub fn with_ignore_filenames(
        self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self;

    /// Path to one global ignore file (gitignore syntax) applied to
    /// every walk regardless of git presence. Resolved by the
    /// downstream server; the framework defines no default location.
    pub fn with_global_ignore_file(self, path: impl Into<PathBuf>) -> Self;
}
```

Unconfigured defaults: no names, no global file — the walk config is
byte-identical to today's and the mechanism is fully inert (no files
read, no watcher additions, no behavior change).

### 3.2 Mechanics

- **Local names** are plumbed `ServerOptions` → state →
  `WorkspaceWalkConfig` and applied through
  `WalkBuilder::add_custom_ignore_filename` (verified:
  `ignore-0.4.33/src/walk.rs:820`). Prune-native: excluded directories
  are never descended into. Gitignore syntax and per-directory
  cascading come from the crate, not from us.
- **Global file** is compiled with `ignore::gitignore::Gitignore` and
  applied as a filter over collected walk entries before matcher
  association. Trade-off (accepted): the global list does not prune
  during traversal — one small file's worth of over-walking, unlike
  per-directory names. The compiled matcher is cached by the file's
  `FileStamp`; a changed stamp recompiles on the next walk.
- The walk itself stays in its single `spawn_blocking` hop
  (`walk_blocking`); the global-file read/compile joins that hop, not
  the executor.
- Exclusions apply in one place — the walker, before matchers see
  files. Matcher globs stay include-side; there is exactly one
  exclusion mechanism. Every walk consumer (workspace diagnostics,
  refresh, oneshot) inherits it with no per-caller logic.

### 3.3 Git-less semantics (documented explicitly, per D6)

| Scenario | `.gitignore` honored? | Custom names honored? |
|---|---|---|
| Project with `.git` | yes | yes |
| No `.git` (quick POC, single file) | **no** (`require_git = true`) | yes |
| `.git` present, no project `.gitignore` (global gitignore via git config) | global gitignore honored (git config resolution, engine behavior) | yes |

Custom names and the global file are git-independent by construction —
they close the exclusion gap for git-less projects.

### 3.4 Watch lifecycle

- `watcher_globs()` (the include-side matcher globs today) is extended
  with the configured ignore filenames and the built-in `.gitignore`
  name, whenever watchers register (i.e. under enabled diagnostics with
  client support, as today).
- `handle_watched_files_change` gains a branch **before** document
  handling: an event whose file name matches a configured ignore name
  (or the built-in `.gitignore`) → `walk_cache().invalidate()`; the
  event is not processed as a document event. The existing refresh path
  then
  re-walks with fresh ignore state; now-ignored documents fall out via
  the existing retain pass, newly-included files load on the next walk.
- This closes the walk-cache spec's documented `.gitignore`-staleness
  limitation with the same registration — no new state channel.

### 3.5 Acceptance criteria

- A configured name excludes files/directories from walks in a
  git-less root; cascading per directory works; negation (`!`)
  patterns behave per gitignore semantics.
- The global file excludes across roots, independent of git presence;
  editing it (stamp change) changes the next walk's result.
- With nothing configured, walker golden tests are byte-identical to
  today.
- An ignore-file event invalidates the walk cache (a previously
  invisible file becomes visible after the event); a `.gitignore` edit
  likewise.
- An open document matching a new ignore rule stays tracked and
  functional (ignore governs walking only — §6).
- `make battery` green, zero warnings, both feature legs; `make dupes`
  0/0.

## 4. W4 — Capability-gated dispatch

### 4.1 Mechanism

Recon fact (71-row matrix): the 48 `lsp_dispatch!` rows have no
dispatch-time capability gate; the only check is a once-per-method
warning when an *advertised* method hits its trait default, and the
resolve family's `resolve_provider` gates exist in docs only.

Fix: the dispatch wrapper consults `MethodInventory`
(`src/server/inventory.rs`, built from the final merged
`ServerCapabilities`) **before** running conversion hooks or the
handler. Method absent from the inventory → immediate
`METHOD_NOT_FOUND` (`-32601`) error response, plus a once-per-method
warning naming the method (implemented-but-unadvertised is an
implementor bug; it must be loud, not silent).

Both directions of the invariant become mechanical:

- unadvertised → never activates (new gate);
- advertised-but-defaulted → warns once (existing guard, unchanged).

### 4.2 Exceptions and edge semantics

| Surface | Gate |
|---|---|
| `initialize`, `initialized`, `shutdown` | none — lifecycle, spec-unconditional (`shutdown` keeps the async-lsp default; documented) |
| `workspace/diagnostic` | existing `supported()` gate (unchanged) |
| resolve family (6 rows) | gated on the corresponding `*_resolve_provider` capability; the inventory gains a resolve→provider mapping |
| type-hierarchy trio | always-allowed; `lsp_types` 0.95.1 lacks the capability fields (D7) — re-evaluate on lsp_types upgrade |
| notifications (13 handlers) | no capability gate — spec-unconditional (didOpen/didChange family), documented in the matrix |
| dynamic registrations (watchers, configuration section, refresh) | existing triple gates confirmed by recon; matrix rows cite them |

### 4.3 Test-fixture impact

Wire dispatch tests (the dispatch-row test that sends valid params for
every method, the echo round-trips) run against fixture servers whose
`server_capabilities` must now advertise what they serve, or the gate
rejects the calls. This is planned work in this cycle, not a surprise:
fixtures gain full capability advertisements.

### 4.4 Acceptance criteria

- A request for an unadvertised method returns `-32601` and never
  reaches conversion hooks or the handler; the warning fires once.
- An advertised method dispatches as today; advertised-but-defaulted
  still warns once (existing behavior preserved).
- Resolve rows are gated on their provider capabilities, both
  directions pinned.
- The full 48-row capability × behavior matrix (recon appendix A) is
  lifted into the spec's test plan; every previously unpinned cell
  gains a wire pin.
- Battery/dupes criteria as in §3.5.

## 5. W5 — Non-blocking guarantees

Recon fact (35-row matrix): the batch pipeline is clean (spawn_blocking
composites, no guard held across an await), but per-request paths carry
real executor-thread blocking.

| Site (recon citations) | Action |
|---|---|
| `read_document_from_disk` reached via the per-request dispatch fallback (`with_state/mod.rs:45`, 42 rows) | **Fix.** The wrapper knows the request URL upfront (`extract_url`): untracked + `file:` scheme → read on `spawn_blocking` into a bounded state-level cache keyed by URL, valued `(FileStamp, Document)`; the wrapper awaits the prime before running the hooks, so the hooks read the cache only — a miss (non-`file` scheme, failed read) skips conversion, identical to today's read failure. |
| `read_document_from_disk` on response paths: `workspace/symbol` (`lsp_requests/symbol.rs:66`, per-request HashMap cache) and `workspace_symbol_resolve` (`:49`) | **Fix.** The whole `modify_response_standalone` / `modify_params_standalone` invocation runs inside `spawn_blocking` for these paths — the complete set of disk-reading hooks (verified by import list: exactly three `read_document_from_disk` call sites). The hooks are self-contained sync fns; response types are `Send`. The per-request HashMap cache stays. |
| `workspace_folder_path` canonicalizing every *removed* folder per `didChangeWorkspaceFolders` (`workspace.rs:33-38`) | **Fix.** Removed roots take their paths from the already-canonical `workspace_roots` map (keyed by folder URI); only added folders canonicalize. The stale arch-lint "one-time" comment is corrected. |
| Open-document tree-sitter parses on the executor thread (didOpen/didChange/didSave/watched/recovery) | **Keep, document.** async-lsp requires synchronous notification handlers; parsing off-thread would break didChange ordering. Invariant comment at each site + spec row. |
| Oneshot inline walk/read when no tokio runtime is current | **Keep, document.** CLI batch design; the blocking pool is unavailable without a runtime. |
| `serve()` pipe locking, ConcurrencyLayer, ClientProcessMonitor, lock scopes | **Clean** per recon; pin as-is (no guard across an await anywhere — that finding gets a regression note in the disposition table). |

### 5.1 Acceptance criteria

- No `std::fs` read executes on an executor thread in any request path;
  the recon blocking matrix is re-audited at review time with every row
  either fixed or carrying its justification.
- Conversion correctness is unchanged: positions for untracked-URL
  requests still convert against disk content (existing conversion
  tests stay green; new tests cover the cache hit/miss/stamp-change
  paths).
- Folder-add/remove behavior unchanged (existing folder-change tests
  green; no re-canonicalization of removed roots).

## 6. Behavior contract — documents outside the workspace

Fixed as a contract (originating from owner questions during
brainstorming):

1. Per-document features (hover, completion, document diagnostics,
   document symbols, sync, edits) work for **any** file the client
   opens via `didOpen` and that matches a `DocumentMatcher` —
   workspace membership is never consulted. Chained navigation
   (go-to-definition into `~/.cargo/registry/...` or `node_modules`,
   then deeper) is unlimited in depth; resolution quality is the
   downstream server's responsibility, never the framework's.
2. Ignore rules govern **walking only** (batch scans). An open
   document is never excluded by any ignore source.
3. Open documents outside all roots are retained in state, receive
   per-document features, and are not part of workspace-diagnostic
   polls (current behavior, now contractual).
4. Requests about files the server does not track at all convert
   positions against on-disk content via the §5 fallback — the
   contract D4 preserves.

Pins: an existing test covers (3) (open doc outside roots survives
refresh); a new test pins (2) (an open document matching a fresh ignore
rule stays tracked); (1) is covered by the general document tests.

## 7. W6 — Custom machinery disposition

| Machinery | Disposition | Evidence |
|---|---|---|
| `WorkspaceWalker` over `ignore` | keep | dua-core evaluation, gate R1 |
| `for_each_bounded` (`futures::buffer_unordered` wrapper) | keep | rayon evaluation, delta ≈ 0 |
| `WalkCache` | keep | built and reviewed this branch's predecessor |
| `Document::derived` slot | keep | built and reviewed this branch's predecessor |
| `read_document_from_disk` | keep contract, change mechanism | D4 + §5 |
| Oneshot runner | keep | CLI design |
| Guards-across-await | none exist | recon pin |

The table is completed during implementation if new candidates surface;
every row names its evidence — no "replace with a crate" without a
measured or cited reason.

## 8. Public surface and compatibility

- Additive only: two `ServerOptions` builder methods (§3.1) and — added
  by owner ruling mid-cycle — the two mirroring
  `WorkspaceDiagnosticConfig` setters on the oneshot side (lands in
  commit `abdb9c0`), with full `///` docs per `missing_docs`. No
  signature changes; no new dependencies (rayon rejected by D2;
  dua-core removed); no feature changes (D3).
- **Behavior change (W4), flagged in the commit message**: requests for
  methods absent from the final `ServerCapabilities` now answer
  `-32601` instead of silently serving. Downstream servers that
  implement handlers without advertising the capability will see them
  stop being reachable — that is the invariant being enforced, and the
  warn-once names the method.
- No changes to the UTF-8 invariant or the `lsp_requests/` conversion
  architecture.

## 9. Known limitations

1. **Global ignore file watching is client-dependent** — watchers for
   patterns outside workspace roots may not be honored; degradation is
   stale-until-next-invalidation.
2. **Global file does not prune traversal** (§3.2 trade-off).
3. **Type-hierarchy gate exemption** until `lsp_types` grows the
   capability fields (D7).
4. **Ignore-file watching only under registered watchers** — the same
   `watchers_registered()` gate as the walk cache: no client watching
   support ⇒ stale until any other invalidation event (carried from
   the walk-cache spec).
5. **In-flight invalidation race** (carried from the walk-cache spec):
   a walk completing during an invalidation may store a stale list;
   bounded, self-healing on the next event.
6. **CLOSED 2026-09-18 by the dependency swap**: the conversion hops'
   stall window rode upstream bug oxalica/async-lsp#30 (MainLoop stops
   polling in-flight tasks while `poll_ready` waits at capacity).
   Originally accepted-over-alternatives (dropping the inline await
   would degrade every first request per untracked file), then closed
   early: the dependency now git-pins SebTardif's fix branch for PR #30
   (`Cargo.toml`, rev `2ac76fb9`, with the matching `allow-git` in
   `deny.toml`), the concurrency tripwire is flipped to assert recovery,
   and the window no longer exists. Swap back to the crates.io release
   when upstream ships the fix; if the overflow-blocked tripwire failure
   ever returns, the pin was lost.

## 10. Testing strategy

- W2: golden walker tests for §3.3 scenarios; cascading and negation;
  global-file stamp recompile; watcher-invalidation both directions;
  inert-when-unconfigured (byte-identical walk); §6 pin (open doc
  survives a fresh ignore rule).
- W4: wire pins per capability direction (advertised serves,
  unadvertised answers `-32601` and never reaches hooks); resolve
  provider gates; fixture capability advertisements; the recon matrix
  cells lifted into named tests.
- W5: behavior-identical conversion tests on the cache hit/miss/stamp
  paths; spawn_blocking-wrapped standalone conversions (existing
  workspace-symbol conversion tests stay green); folder-change tests
  unchanged.
- Battery: `make battery` zero warnings both feature legs; `make
  dupes` 0/0; dylint and arch-lint green (the corrected arch-lint
  comment in §5 included).

## 11. Out of scope

Public walker API; programmatic ignore override globs (D5); a Cargo
feature for ignore (D3); language-specific policy; CI changes;
lsp-poc-side adoption (follow-up note: re-pin and consume the two new
builder methods when desired); lsp_types upgrade to close D7.

## 12. Appendices

- A: capability × behavior matrix —
  `docs/superpowers/research/2026-09-18-invariant-audits-recon.md`
  (Matrix 1, 71 rows).
- B: blocking-site inventory — same doc (Matrix 2, 35 rows).
- C: walker verdict —
  `docs/superpowers/research/2026-09-18-dua-core-walker-evaluation.md`.
- D: rayon verdict —
  `docs/superpowers/research/2026-09-18-rayon-batch-evaluation.md`.
