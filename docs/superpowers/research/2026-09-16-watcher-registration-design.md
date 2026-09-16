# Research: `didChangeWatchedFiles` registration design

Date: 2026-09-16 · Branch read: `feature/walk-cache` (read-only research; no code changed)
Question: **where do we register, and how do we derive watcher globs from our matchers?**
Method: evidence first (vendored `lsp-types` source, our code, four Rust LSP servers), then
conclusions, then a recommendation. The `rust-skills` API-design rules were applied as the
lens for the final recommendation.

---

## 1. Evidence

### 1.1 The shapes (lsp-types 0.95.1, vendored source)

Path: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lsp-types-0.95.1/src/lib.rs`
(line numbers from that file; verbatim mirror of the LSP 3.17 spec section
[`workspace/didChangeWatchedFiles`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_didChangeWatchedFiles)
— the spec page itself was not fetched; the vendored types are the mirror we compile against).

| item | lines | shape |
|---|---|---|
| `DidChangeWatchedFilesClientCapabilities` | 2360–2376 | `dynamic_registration: Option<bool>`, `relative_pattern_support: Option<bool>` (3.17.0) |
| `DidChangeWatchedFilesParams` | 2378–2385 | `changes: Vec<FileEvent>` |
| `FileChangeType` | ~2387–2403 | `CREATED = 1`, `CHANGED = 2`, `DELETED = 3` |
| `FileEvent` | ~2406–2416 | `uri: Url`, `typ: FileChangeType` (serde key `type`) |
| `DidChangeWatchedFilesRegistrationOptions` | 2418–2422 | `{ watchers: Vec<FileSystemWatcher> }` — the whole register_options payload |
| `FileSystemWatcher` | 2425–2437 | `glob_pattern: GlobPattern`, `kind: Option<WatchKind>` |
| `GlobPattern` | 2444–2453 | untagged enum: `String(Pattern)` \| `Relative(RelativePattern)` — **both forms exist in 0.95.1** |
| `RelativePattern` | 2467–2475 | `{ base_uri: OneOf<WorkspaceFolder, Url>, pattern: Pattern }` (3.17.0) |
| `Pattern` | 2477–2495 | `type Pattern = String`; documented syntax: `*` (one segment), `?`, `**` (any segments incl. none), `{}` grouping, `[]` ranges, `[!...]` negated ranges |
| `WatchKind` | 2496–2503 | bitflags `Create = 1`, `Change = 2`, `Delete = 4`; `kind` omitted defaults to 7 (2433–2435) |
| `WatchKind` serde | 2505–2523 | serialize as bare `u8`; deserialize rejects unknown bits |

Key answers embedded in the source:

- **`RelativePattern` is available in 0.95.1**, gated on the client capability
  `workspace.didChangeWatchedFiles.relativePatternSupport` (2372–2376).
- The spec comment at 2366–2368: *"the current protocol doesn't support static
  configuration for file changes from the server side"* — **dynamic registration is the
  only way** a server can request watcher events. A server that never registers receives
  `didChangeWatchedFiles` only from clients that watch on their own initiative.

### 1.2 Our matchers (glob storage)

`src/documents/matcher.rs`:

- `with_url_globs` appends verbatim strings to `url_globs: Vec<String>`
  (`src/documents/matcher.rs:79-88`); no normalization anywhere.
- `DocumentMatchers::new` compiles each matcher's globs into one `globset::GlobSet`
  (`src/documents/matcher.rs:163-204`). Individual globs that fail `Glob::new` are
  **warn-skipped, not errors** (`src/documents/matcher.rs:172-191`); a matcher whose
  whole globset fails contributes nothing.
- Matching is path-absolute and root-agnostic: `find_path` runs each GlobSet over the
  full file path (`src/documents/matcher.rs:215-226`).

Real glob examples in the tree:

- `"**/*.json"` — `examples/tree_sitter.rs:27`, `src/testing.rs:204`, doctest in
  `src/documents/matcher.rs:26`
- `"**/*.txt"` — `examples/batch_diagnostics.rs:42`
- `"**/*.demo"` / `"*.demo"` — `src/oneshot/workspace_diagnostics.rs:147` (doctest), `:460`, `:602`
- `format!("**/*.{extension}")` + `format!("*.{extension}")` — `src/testing.rs:186`

The dominant shape is `**/*.<ext>`; bare `*.<ext>` siblings exist alongside.

### 1.3 The registration precedent (what we already do)

`src/workspace/diagnostics.rs`:

- Gate: `can_register_configuration` → `configuration_gate(&client_dynamic_configuration)`
  = `supported && client_dynamic_registration && setting present`
  (`src/workspace/diagnostics.rs:86-96`; flags recorded from client capabilities in
  `configure`, `:105-124`).
- Hook: `initialized(state)` calls `register_configuration` + `request_configuration`
  (`src/workspace/diagnostics.rs:230-233`), invoked from the wrapper's `initialized`
  (`src/server/with_state/mod.rs:117-119`).
- Body: fire-and-forget `spawn`, a `RegisterCapability` request with a **fixed id**
  `"async-language-server.workspaceDiagnostics.configuration"`, `method`, and
  `register_options` built with `serde_json::json!`; failures `tracing::warn!`ed
  (`src/workspace/diagnostics.rs:276-302`).

There is **no watcher registration anywhere today**. The only
`workspace/didChangeWatchedFiles` surface in the crate is the inbound dispatch
(`src/server/with_state/mod.rs:174-182`) → state handling → the `Server` trait hook
(default no-op, `src/server/server_trait.rs:661-667`).

### 1.4 The existing consumer (what events do today)

`handle_watched_files_change` (`src/server/state/documents.rs:351-395`):

1. **Untracked URIs are skipped** — "the next workspace scan picks up new files instead"
   (`documents.rs:356-360`).
2. **Open documents are never touched** — the editor owns them (`documents.rs:361-364`).
3. `DELETED` → the `Workspace`-origin entry is removed and its semantic-tokens cache
   dropped (`documents.rs:366-373`).
4. `CREATED`/`CHANGED` → the tracked snapshot is replaced by a **synchronous re-read**
   of the file (`std::fs`, spec-mandated sync notifications); on read failure the old
   snapshot is kept with a `warn` (`documents.rs:375-391`).

The only other path that refreshes `Workspace` documents is
`refresh_workspace_documents` (`src/server/state/workspace.rs:76-140`), stamp-gated by
`FileStamp = (mtime, size)` (`src/server/state/mod.rs:32-34`, gate at
`workspace.rs:142-199`, IO off the executor via `spawn_blocking`, `workspace.rs:159-166`).
Its **sole caller** is the `workspace/diagnostic` request path
(`src/workspace/diagnostics.rs:404`). `insert_document` does not touch stamps, so a
watched-file re-read leaves stamp bookkeeping to the next refresh (conservative re-read).

### 1.5 Consequence for the kind-flag question

If a registration omitted `WatchKind::Change`:

- LIST invalidation (Create/Delete) would be served.
- But the **eager** refresh of tracked `Workspace` documents on external content edits
  (git checkout, build tools) would stop — the only eager path is the watcher's
  `CHANGED` branch (`documents.rs:375-391`); everything else waits for a
  `workspace/diagnostic` request to lazily stamp-refresh (`diagnostics.rs:404`).

So "Create/Delete only" **would regress** existing behavior, not just fail to improve it.

### 1.6 Glob syntax: globset vs LSP watcher patterns

- globset `Glob` (0.4.x) supports `*`, `**`, `?`, `{a,b}`, `[...]`, `[!...]`/`[^...]`;
  `!` is special **only inside character classes** (`globset-0.4.x/src/glob.rs:927-930`).
  There is no gitignore-style leading-`!` negation in `Glob` (that lives in globset's
  `Gitignore` type, which we do not use) — so our `url_globs` cannot contain negation
  globs that would need translating. Neither side has it; nothing is lost in pass-through.
- Divergence that does exist: globset's default `literal_separator = false` means a bare
  `*` **can cross `/`** when *we* match (`*.json` matches `a/b/c.json`), while an LSP
  watcher's `*` matches **within one path segment** (spec text mirrored at lsp-types
  `lib.rs:2478-2480`) — sent as-is, `*.json` would under-watch (root-level files only).
  All `**/`-prefixed globs (our dominant shape) are unambiguous in both dialects.
- `{}` alternation is valid on both sides — confirmed in the field by PHPantom shipping
  `"**/*.{yaml,yml,xml}"` as a watcher glob (§2.1).

### 1.7 Web evidence

#### A. PHPantom (`https://github.com/PHPantom-dev/phpantom_lsp`, commit `1f0ed98`) — closest analogue, same-language reference

- `src/indexing/watch.rs` — `build_watched_file_registration()` derives watchers from
  config: one `FileSystemWatcher { glob_pattern: GlobPattern::String(format!("**/*.{ext}")),
  kind: Some(Create | Change | Delete) }` per configured extension, plus fixed watchers
  (`**/*.php`, `**/*.{yaml,yml,xml}`, `**/composer.json` with kind `Some(Change)` only,
  `**/.phpantom.toml`, Laravel-conditional `**/*.sql`). The `Registration` is built as a
  typed `DidChangeWatchedFilesRegistrationOptions` and serialized with
  `serde_json::to_value` into `register_options`. Fixed id constant
  `WATCHED_FILES_REGISTRATION_ID = "workspace/didChangeWatchedFiles"` shared with the
  re-registration path.
- Lifecycle: the first registration is pushed from `initialized` (batch in
  `src/server.rs` — `registrations.push(watched_files_registration)`, next to
  `type_hierarchy_registration()`; per the `registered_watcher_state` doc in
  `src/lib.rs`: "`None` until `initialized` performs the first registration").
- Re-registration: `reregister_watched_files_if_changed` keeps the last-registered
  `(extensions, is_laravel)` tuple, and on an actual config-driven change runs
  **unregister-then-register with the same id** (guarding against churn on unrelated
  edits). No watcher-specific client-capability gate was found in the registration path
  (their type-hierarchy registration *is* gated on a dynamic-registration flag — gating
  is a choice they make per feature, not a blanket rule).
- Kind flags: `Create | Change | Delete` (7) for source/config files; `Change`-only for
  dependency manifests (`composer.json`/`.lock`) whose *content* matters but whose
  create/delete is uninteresting. Their event consumer mirrors our tracked-doc logic:
  `CHANGED` events are acted on only for files already parsed/indexed, Created/Deleted
  always (comment on `apply_watched_file_changes`).
- On `RelativePattern`: avoided deliberately — "dynamic registration with an absolute
  `RelativePattern` base is unevenly supported across editors" (comment on
  `global_config_watcher`, which polls an out-of-workspace config file instead).

#### B. rust-analyzer (`https://github.com/rust-lang/rust-analyzer`, commit `fa88768`)

- `crates/rust-analyzer/src/reload.rs`, `switch_workspaces`: when its own
  `files.watcher` config is `Client`, it derives watchers from the **workspace model** —
  local roots' include dirs mapped to four patterns each (`**/*.rs`, `**/Cargo.{lock,toml}`,
  `**/rust-analyzer.toml`, `**/*.md`). With client `relativePatternSupport` it emits
  `GlobPattern::Relative { base_uri: root }`; otherwise it prefixes the base into
  absolute string globs (`format!("{base}/**/*.rs")`). `kind: None` on every watcher
  (relying on the spec default 7). Registration id is the fixed
  `"workspace/didChangeWatchedFiles"`; `register_options` via `serde_json::to_value`.
  Re-runs on every workspace switch (same id ⇒ clients replace the registration).
- The capability reader is `did_change_watched_files_relative_pattern_support()`
  (`crates/rust-analyzer/src/lsp/capabilities.rs`) — the only watcher-related client
  capability they consult; no `dynamicRegistration` gate (VS Code always supports).
- `should_refresh_for_change` (bottom of `reload.rs`) short-circuits
  `ChangeKind::Modify → false` for *project-model refresh* — i.e. even rust-analyzer
  separates "content changed" from "structure changed" in its consumers, while keeping
  the subscription set at 7.

#### C. tinymist (`https://github.com/Myriad-Dreamin/tinymist`) — the bypass pattern

The LSP `didChangeWatchedFiles` machinery is not used at all. The VS Code extension
creates its own client-side `vscode.workspace.createFileSystemWatcher("**/*")`
(`editors/vscode/src/lsp.ts`, `registerClientSideWatch`), and the server drives the
watch set via a custom `tinymist/fs/watch` request; the extension reads file *contents*
and pushes them over a custom `tinymist/fsChange` request. They need content of
non-opened files, which watched-files notifications do not carry — our crate does not
have that requirement (`Workspace` documents are read by us, not pushed by the client).
Useful as a boundary marker, not a pattern to copy.

#### D. texlab (`https://github.com/latex-lsp/texlab`, commit `4cc18b3`) — the do-nothing pattern

The server implements `did_change_watched_files` (`crates/texlab/src/server.rs`) but a
code search over the repo finds **no** `DidChangeWatchedFilesRegistrationOptions`, no
`FileSystemWatcher`, and no `fileEvents` (extension side) — texlab registers no watchers
and relies on whatever the client environment provides. Consequence to note: a server
that consumes the notification without registering has no guarantee of receiving it
(§1.1: dynamic registration is the only server-side mechanism).

#### E. Authoritative anchor on "what do servers watch"

The LSP spec (anchored section
[`workspace/didChangeWatchedFiles`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_didChangeWatchedFiles),
verified through the vendored lsp-types mirror rather than a page fetch) fixes the
pattern syntax and the kind default; it prescribes no derivation rule. Field practice
from A/B answers the derivation question empirically: **every server watches its own
supported file types, scoped to workspace roots when the model has roots** — PHPantom
`**/*.{ext}` per configured extension, rust-analyzer language+manifest patterns per
local root. None watches `**/*` server-side (tinymist's client does, and filters
itself).

---

## 2. Conclusions

1. **Registration point: `initialized`.** Dynamic registration is the only mechanism
   (§1.1), the spec's lifecycle for `client/registerCapability` is post-`initialize`,
   and our own precedent (`register_configuration`) already lives in
   `initialized` with the exact shape this needs (§1.3). `initialize` is too early
   (client may not accept registrations before `initialized` returns);
   `did_change_configuration` is irrelevant because matchers are fixed at construction —
   there is no config that can change the watch set mid-session today.
2. **Capability gate: `workspace.didChangeWatchedFiles.dynamicRegistration`.** The
   symmetric analogue of our `didChangeConfiguration.dynamicRegistration` gate; the
   capability exists in lsp-types (§1.1). rust-analyzer/PHPantom skip the gate because
   they target VS Code; our framework serves arbitrary clients and already has the
   flag-recording pattern to copy (§1.3, `configure`).
3. **Glob derivation: pass `url_globs` through as-is**, filtered by the same
   `Glob::new` validity rule the matcher already applies (§1.2). Our globs are
   root-agnostic and matcher-vs-watcher dialects agree on everything we actually store.
   `RelativePattern` adds nothing for correctness (our matching is path-absolute, not
   root-scoped) and is unevenly supported in the field (§2.1, §1.6) — skip it for v1.
4. **Lang-only matchers (no `url_globs`): contribute nothing.** A language id has no
   extension mapping in this crate, and language-id matching is `didOpen`-driven; there
   is nothing to watch. A downstream server that needs extra watchers can issue its own
   `RegisterCapability` through `ServerState::client()` (the same handle
   `register_configuration` uses) — no new framework knob required.
5. **Kind flags: `Create | Change | Delete` (7), set explicitly.** Omitting `Change`
   regresses eager tracked-document refresh (§1.5); `Create`/`Delete` are what future
   list invalidation needs. Explicit beats omitted: the 7-default is spec text, and
   PHPantom's explicitness documents intent per watcher. The per-event cost is already
   bounded — untracked and Open documents are skipped before any IO (§1.4).
6. **Glob-syntax divergences, flagged:**
   - Bare `*`-prefixed globs (`"*.demo"`) match nested paths in our matcher but
     under-watch if sent to a client verbatim. Our stored examples pair them with
     `**/` twins; the divergence only bites if a server registers a bare glob as its
     *only* form. Pass-through keeps the `**/` forms correct; document the edge.
   - Negation: neither dialect has leading-`!` negation (§1.6) — a non-issue.
   - Case sensitivity: globset is case-sensitive; client watcher case behavior is
     platform-dependent — irrelevant for our lowercase-by-convention extensions,
     noted for completeness.

---

## 3. Recommendation (our framework)

A `register_watched_files` step, shaped on `register_configuration`:

1. **When**: in `initialized` (alongside `register_configuration`), spawned
   fire-and-forget; failures `tracing::warn!`ed with the error in hand — the existing
   precedent (`src/workspace/diagnostics.rs:276-302`) verbatim.
2. **Gate**: client `workspace.didChangeWatchedFiles.dynamicRegistration == true`,
   recorded in `ServerState` during `configure` like the existing flags, **and** at
   least one valid derivable glob. No gate on our workspace-diagnostics setting —
   tracked-document refresh (the existing consumer, §1.4) is always-on behavior.
3. **Derivation**: collect `url_globs` from all registered matchers, keep exactly the
   strings `Glob::new` accepts (the matcher itself warn-skips the rest, §1.2), dedupe,
   and emit one `FileSystemWatcher { glob_pattern: GlobPattern::String(glob),
   kind: Some(WatchKind::Create | WatchKind::Change | WatchKind::Delete) }` per glob.
   Pass-through, no normalization, no `RelativePattern`.
4. **Payload**: build a typed `DidChangeWatchedFilesRegistrationOptions` and serialize
   with `serde_json::to_value` (the `register_options` wire shape, §1.1) — the
   `api-typed-response` rule; unlike the configuration registration's `json!`, the
   full typed struct exists here and should be used.
5. **Id**: fixed, e.g. `"async-language-server.watchedFiles"`, mirroring
   `"async-language-server.workspaceDiagnostics.configuration"`. **No unregister
   path**: matchers are session-fixed, so the watch set cannot change. If matchers ever
   become reconfigurable, adopt PHPantom's compare-last-registered-state →
   unregister-same-id → register pattern (`§2.1`) rather than unregistering eagerly.
6. **Consumers unchanged**: the existing handler (`documents.rs:351-395`) already
   implements the right kind semantics — Delete purges, Create/Changed re-read, all
   bounded by the tracked/Open filters. Future list invalidation (walk cache) adds a
   Create/Delete consumer of the *same events*; no second registration, no kind split.

Open item for the plan discussion (not blocking): whether bare `*.` globs should get a
one-time `warn!` at derivation time ("watches only root-level files client-side") or be
silently passed through. Pass-through + doc note is the minimal version; the warning is
the friendly one.

---

## 4. Sources

Local code (branch `feature/walk-cache` at time of writing):

- `src/documents/matcher.rs:79-88, 163-204, 215-226` — glob storage, compile, matching
- `src/workspace/diagnostics.rs:86-96, 105-124, 230-233, 276-302, 404` — gate, flags,
  `initialized`, registration precedent, refresh caller
- `src/server/with_state/mod.rs:117-119, 174-182` — lifecycle hook, watched-files dispatch
- `src/server/state/documents.rs:351-395` — the watched-files consumer
- `src/server/state/workspace.rs:76-140, 142-199`; `src/server/state/mod.rs:32-34` —
  stamp-gated lazy refresh
- `src/server/server_trait.rs:661-667` — `Server` hook default
- Examples/fixtures: `examples/tree_sitter.rs:27`, `examples/batch_diagnostics.rs:42`,
  `src/testing.rs:186, 204`, `src/oneshot/workspace_diagnostics.rs:147, 460, 602`
- `lsp-types-0.95.1/src/lib.rs:2360-2523` (vendored:
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`)
- `globset-0.4.x/src/glob.rs:927-930` (vendored, same registry)

Web:

- PHPantom: <https://github.com/PHPantom-dev/phpantom_lsp> — `src/indexing/watch.rs`,
  `src/server.rs`, `src/lib.rs` (commit `1f0ed98`)
- rust-analyzer: <https://github.com/rust-lang/rust-analyzer> —
  `crates/rust-analyzer/src/reload.rs`, `crates/rust-analyzer/src/lsp/capabilities.rs`
  (commit `fa88768`)
- tinymist: <https://github.com/Myriad-Dreamin/tinymist> — `editors/vscode/src/lsp.ts`
- texlab: <https://github.com/latex-lsp/texlab> — `crates/texlab/src/server.rs`
  (commit `4cc18b3`; negative searches: `FileSystemWatcher`, `fileEvents`, 0 hits)
- LSP 3.17 spec, `workspace/didChangeWatchedFiles` section:
  <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_didChangeWatchedFiles>
  (shapes verified via the vendored lsp-types mirror; the spec page itself was not fetched)
