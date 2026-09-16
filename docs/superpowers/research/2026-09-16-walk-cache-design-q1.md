# Walk-cache design — Q1: the invalidator set (research)

Branch `feature/walk-cache` design cycle, 2026-09-16. Question: which signals must
invalidate a cached workspace file list (the `WorkspaceWalker` result reused between
`workspace/diagnostic` polls). Evidence → conclusion → recommendation.

---

## Evidence — local code facts

### Only one `RegisterCapability` exists in the crate

Grep over `src/`: a single `client/registerCapability` request, the configuration
registration in `src/workspace/diagnostics.rs:288` (`register_configuration`,
method `workspace/didChangeConfiguration` with a `section` register option, sent
from the `initialized` hook at `src/workspace/diagnostics.rs:257-260`).
**No `workspace/didChangeWatchedFiles` registration is ever sent** — the framework
does not register a watcher today. [source]

### What `did_change_watched_files` does today

The hook is dispatched unconditionally (`src/server/with_state/mod.rs:174-182`,
no client-capability gate on the dispatch itself) and lands in
`handle_watched_files_change` (`src/server/state/documents.rs:352-395`):

- Untracked URIs are skipped with the comment "Untracked URIs are not loaded here -
  the next workspace scan picks up new files instead" (`src/server/state/documents.rs:357-359`).
- `Open`-origin documents are skipped: "The editor owns open documents; disk events
  never touch them" (`src/server/state/documents.rs:361-363`).
- `DELETED` removes the tracked `Workspace` entry (`:365-372`); `Created`/`Changed`
  re-read the file synchronously and replace the snapshot (`:380-390`), keeping the
  last-known snapshot when the read fails.

So today the notification updates **per-document snapshots of already-tracked files**
only. It does not maintain any file-list, and no walk cache exists yet
(`src/workspace/walker.rs` has no cache — grep clean), so there is nothing for the
hook to invalidate at present. The hook is the natural place to hang invalidation.
[source]

### File-watching capabilities: neither advertised nor inspected

- Client-side, the framework reads exactly three flags: `workspace.configuration`,
  `workspace.didChangeConfiguration.dynamicRegistration`, and
  `workspace.diagnostic.refreshSupport` (`src/workspace/diagnostics.rs:109-125`).
  Nothing reads `workspace.didChangeWatchedFiles.dynamicRegistration`. [source]
- Server-side, `enable_workspace_folder_tracking`
  (`src/workspace/diagnostics.rs:195-213`) advertises
  `workspace.workspaceFolders.supported = true` and `change_notifications = true` —
  workspace *folders*, not watched files. No `didChangeWatchedFiles`-related server
  capability is advertised. [source]

Conclusion of the local pass: watcher registration (b) would be net-new code; the
notification plumbing itself already exists and is fed by whatever the client
volunteers.

---

## Evidence — spec (LSP 3.17)

### The workspace pull imposes no freshness obligation

The `workspace/diagnostic` section states the request "can be long running and is
**not bound to a specific workspace or document state**" (spec partial
`language/pullDiagnostics.md`, § Workspace Diagnostics, lines 325-330 —
https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/language/pullDiagnostics.md).
[spec]

That sentence is the whole answer to the freshness question: a pull response is a
"current calculable instance" at the server's discretion, not a consistent snapshot
obligation. The `unchanged` report / `resultId` machinery is described as something
"a server can" use to save bandwidth (§ Document Diagnostics, `UnchangedDocumentDiagnosticReport`
doc comment: "indicating that the last returned report is still accurate") — an
optimization the server opts into, never an obligation to detect changes. [spec]

Corollary on staleness signaling: the only freshness-adjacent lever the spec gives
the *server* is `workspace/diagnostic/refresh` (server → client, "This is useful if
a server detects a project wide configuration change which requires a re-calculation
of all diagnostics", § Diagnostics Refresh). Nothing requires the server to *know*
when its view went stale — with no obligation, serving a stale file list over a
cached walk is spec-legal; the question of *which* invalidators to keep fresh is
purely a quality decision for this framework. [spec]

### didChangeWatchedFiles: registration-based, with a legacy exception

Spec section `workspace/didChangeWatchedFiles.md`
(https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/workspace/didChangeWatchedFiles.md): [spec]

- "It is **recommended that servers register** for these file system events using
  the registration mechanism. **In former implementations clients pushed file events
  without the server actively asking for it.**" — i.e. unregistered events are a
  legacy behavior some clients keep, not something a server may rely on.
- "Servers are allowed to run their own file system watching mechanism and not rely
  on clients … However this is not recommended" — self-watching is a legal fallback
  (reasons listed: cross-OS difficulty, cost, multiple servers per client).
- The `DidChangeWatchedFilesClientCapabilities` doc comment: "Did change watched
  files notification **supports dynamic registration**. Please note that **the
  current protocol doesn't support static configuration** for file changes from the
  server side." — dynamic `client/registerCapability` with
  `DidChangeWatchedFilesRegistrationOptions { watchers: [globs] }` is the *only*
  way to request events; there is no static server-capability equivalent
  (contrast `workspace.workspaceFolders`, which the framework already advertises).
- Client capability gate: `workspace.didChangeWatchedFiles.dynamicRegistration`
  (+ `relativePatternSupport`, 3.17) — to be checked before registering, exactly
  like the existing `didChangeConfiguration.dynamicRegistration` gate at
  `src/workspace/diagnostics.rs:116`.

## Evidence — Zed

Method: targeted GitHub code search plus raw-file greps of `zed-industries/zed`
`main` (commit family of 2026-09; files: `crates/project/src/lsp_store.rs`,
`crates/project/src/lsp_store/dynamic_registration.rs`, `crates/lsp/src/lsp.rs`).
[source]

### Zed honors dynamic file-watching registration

- Client capabilities advertise support: `workspace.didChangeWatchedFiles` =
  `{ dynamic_registration: true, relative_pattern_support: true }`
  (`crates/lsp/src/lsp.rs`).
- `client/registerCapability` is handled per method; the
  `workspace/didChangeWatchedFiles` arm parses the registration options and calls
  `on_lsp_did_change_watched_files` (`crates/project/src/lsp_store/dynamic_registration.rs:388-399`);
  the symmetric `client/unregisterCapability` arm tears the glob set down
  (same file, line 945; `on_lsp_unregister_did_change_watched_files`,
  `crates/project/src/lsp_store.rs:4238-4283`).
- `on_lsp_did_change_watched_files` stores each watcher glob in per-server,
  per-worktree `language_server_watched_paths` (`crates/project/src/lsp_store.rs:4200-4230`),
  and supports both worktree-relative and absolute-path patterns (`register_watcher`,
  `crates/project/src/lsp_store.rs:4042-4090`). An integration test pins the flow
  (`crates/project/tests/integration/dynamic_registration.rs`). [source]

### Zed does NOT push events unregistered

There are exactly two notification send sites in `crates/project/src/lsp_store.rs`
(grep over the raw file — `notify::<lsp::notification::DidChangeWatchedFiles>`):

- worktree events (~line 13877): the loop resolves the server's registered
  `worktree_paths` glob set first — no registration, no notification — then
  filters each change through `watched_paths.is_match`; `PathChange::Loaded`
  (startup scan) is dropped, so rescan noise is not reported as `CREATED` [source];
- absolute-path events (~line 11894, `lsp_notify_abs_paths_changed`): the caller
  in the watcher dispatcher only reaches it through the registered `abs_paths`
  entry (`crates/project/src/lsp_store.rs:15874-15881`) [source].

A repo-wide code search for `notification::DidChangeWatchedFiles` surfaces no third
send path. So current Zed implements the *registered-only* model: the spec's legacy
"In former implementations clients pushed file events without the server actively
asking for it" does not describe Zed. A server that never registers gets zero
`workspace/didChangeWatchedFiles` notifications from Zed. [source]

Practical side-note: Zed's glob matching runs against worktree-relative paths for
worktree watchers and absolute paths for absolute watchers; a registration pattern
like `**/*` rooted via `relativePattern` (supported) or a plain `**/*.ext` string
is what downstream servers would use. [source]

## Evidence — peer practice

**rust-analyzer** keeps its workspace file list fresh through a watcher-driven VFS
whose watching side defaults to the *client*: the `files.watcher` setting is
`"client"` by default ("Controls file watching implementation"
— https://rust-analyzer.github.io/book/configuration.html), meaning the client's
`didChangeWatchedFiles` events feed the VFS; `"server"` and `"client|server"` opt
into rust-analyzer's own OS-level watcher. It registers watchers dynamically with
the client (the VS Code extension forwards file events through
`client.registerCapability`-style watchers) and reconciles its file set from those
events — there is no rescan-on-request and no TTL. [source]

**typescript-language-server** (v6 line) is also watcher-driven for everything
outside open documents: a dedicated `WatchEventManager` computes the watcher set
from the projects in play and *dynamically registers* it via
`connection.client.register(lsp.DidChangeWatchedFilesNotification.type, { watchers })`
(`src/watchEventManager.ts`, `src/lsp-client.ts` —
https://github.com/typescript-language-server/typescript-language-server); incoming
`didChangeWatchedFiles` events go through `handleFileChanges`, which does nothing
when no watcher registration exists (`if (!this.watchers.size …)`). The registration
is recomputed/disposed as coverage changes. tsserver itself re-reads file *contents*
lazily; the list of relevant files/configs is maintained by events, not rescans.
[source]

**gopls** is the closest structural peer: on initialize/didChangeWorkspaceFolders it
sends `client/registerCapability` for `workspace/didChangeWatchedFiles` with
`WatchCreate|WatchChange|WatchDelete` per workspace folder, gated on the client's
`didChangeWatchedFiles.dynamicRegistration` (`DynamicWatchedFilesSupported`) and on
`relativePatternSupport` for relative vs absolute globs, unregistering the previous
registration ID when the folder set changes (`gopls/internal/server/general.go`,
`registerWatchedDirectoriesLocked` — https://github.com/golang/tools). An
experimental server-side fallback exists (`fileWatcher` setting: `fsnotify`/`poll`)
but defaults to `"off"` — "gopls relies exclusively on the language client … to send
file change notifications" (https://github.com/golang/tools/blob/master/gopls/doc/settings.md).
When dynamic registration is unsupported, gopls registers nothing and simply lives
with whatever events the client volunteers (plus, for MCP-agent edits since v0.22,
its own synthetic event injection — not a general rescan). No TTL anywhere.
[source]

Summary of the peer column: all three are watcher-driven with dynamic registration
gated on the client capability; none polls or re-scans per request to stay fresh;
gopls alone ships an opt-in server-side watcher as a fallback, off by default.

## Conclusion

1. **Freshness is the server's discretion.** The pull model binds `textDocument/diagnostic`
   to the synced document version but explicitly leaves the workspace pull "not bound
   to a specific workspace or document state" [spec]. A stale cached file list is
   spec-legal; nothing forces the server to notice changes. Every invalidator is a
   quality decision, chosen for how stale this crate tolerates being.
2. **Against Zed, (b) is real only if the framework registers.** Zed advertises
   `workspace.didChangeWatchedFiles.dynamicRegistration: true`, implements register/
   unregister for the method, and — decisively — never sends the notification to an
   unregistered server: both send sites filter on the server's registered glob sets
   [source]. The spec's legacy unregistered-push behavior ("former implementations")
   is absent in Zed. Therefore `(b)` **requires adding a
   `client/registerCapability` for `workspace/didChangeWatchedFiles`** (gated on the
   client capability, mirroring the existing `didChangeConfiguration.dynamicRegistration`
   gate at `src/workspace/diagnostics.rs:116`) to be a reliable invalidator; without
   it the hook fires never on Zed. The plumbing half already exists:
   `handle_watched_files_change` is dispatched and mutates tracked workspace
   documents (`src/server/state/documents.rs:352-395`) — it would also clear the
   cache on create/delete events, and a rename can be treated as delete+create
   (Zed reports renames as separate DELETED/CREATED; the framework's separate
   `workspace/didRenameFiles` handling, `src/server/state/documents.rs:397+`, is not
   an invalidator — it fires only for files the *client* renamed through its own UI).
3. **(a) stays.** Workspace-folder changes already flow through
   `did_change_workspace_folders` and `enable_workspace_folder_tracking` advertises
   `change_notifications` (`src/workspace/diagnostics.rs:195-213`); folder add/remove
   changes the root set the walk enumerates, so it must invalidate. gopls
   re-registers its watchers on the same signal — the cache clear belongs at the
   same hook. [source+measured]
4. **(c) is cheap and sound — include it as a transition, not a signal.** The cache
   is only read while workspace diagnostics are enabled; `WorkspaceDiagnosticsState::set_enabled`
   already detects transitions (`src/workspace/diagnostics.rs:138-140`). Drop the
   cache when disabled (no reader) and invalidate on the disabled→enabled
   transition (events during the disabled window may have been the only list
   changes and the cache may predate them). Cost: one branch on an existing swap.
5. **didOpen/didClose are NOT list invalidators.** Judged on list membership only:
   `didOpen` opens a file that already exists on disk, so it was already walkable —
   no membership change; content freshness for opened files comes from the `Open`
   origin (editor-owned text), which is orthogonal to the cached *list*.
   `didClose` likewise. The one leak — an editor creating a brand-new file on save —
   is a disk event, not a `didOpen`, and reaches the server through the watcher (b)
   or the next folder change, never through `didOpen` itself. [measured-inference
   from the framework's own origin rules: `src/documents/*`, Open wins over disk]

## Recommendation

Minimal sound invalidator set for this framework:

1. `workspace/didChangeWorkspaceFolders` — invalidate (hook exists; add the clear).
2. `workspace/didChangeWatchedFiles` create/delete events — invalidate, **contingent
   on adding the dynamic registration**: one `RegisterCapability` in `initialized`
   (next to `register_configuration`, `src/workspace/diagnostics.rs:257-260`) with
   per-root `**`-style globs for the implementor's matchers' file extensions,
   gated on `workspace.didChangeWatchedFiles.dynamicRegistration` and using
   relative patterns only when `relativePatternSupport` (both flags read at
   `initialize`). The registration should carry a stable ID for later unregister on
   folder changes, as gopls does.
3. Workspace-diagnostics `Disabled→Enabled` transition — invalidate; `Enabled→Disabled`
   — drop the cache (free: reuse `set_enabled`'s transition return).
4. No TTL, no rescan-on-request, no `didOpen`/`didClose` invalidation.

Degradation rule when the client cannot register watchers (capability absent):
the cache must not be trusted across polls — either disable caching for that
session or bound it (e.g. refresh at most once per `workspace/diagnostic` poll and
never serve a list older than the last event-bearing signal). Spec-wise either is
legal [spec]; the conservative choice for correctness-valuing downstream servers is
"no registration → no cache".

The refresh-support lever (`workspace.diagnostic.refreshSupport`, already read at
`src/workspace/diagnostics.rs:119-125`) is *not* an invalidator: `workspace/diagnostic/refresh`
is server→client — it asks the client to re-pull, it never tells the server
anything. It belongs to the staleness-notification side of a future design, not to
cache invalidation.
