# Invariants Cycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `feature/invariants` cycle — configurable ignore (W2), capability-gated dispatch (W4), off-executor conversion reads (W5) — per spec `docs/superpowers/specs/2026-09-18-invariants-cycle-design.md`.

**Architecture:** No engine changes (walker stays on `ignore`, batch engine stays `for_each_bounded` — spec decision records D1/D2). W2 adds two `ServerOptions` knobs plumbed into `WorkspaceWalkConfig` and the watcher globs; W4 turns `MethodInventory` into a dispatch-time gate inside the `lsp_dispatch!` macro; W5 moves the three `read_document_from_disk` call sites off the executor (state cache + async prime + a per-request `STANDALONE_READS_DISK` marker that makes the macro wrap the hook in `spawn_blocking`).

**Tech Stack:** Rust 2024, MSRV 1.90, `ignore` 0.4.33 (existing), `dashmap`, `tokio` spawn_blocking, `lsp_macros` proc macros.

## Global Constraints

- **No git write commands.** The implementer never commits; after each task's clean review the owner commits. Each task lists a suggested commit message.
- **Done bar per task:** `make battery` green with ZERO warnings, both feature legs; `make dupes` passes (0/0). A failing check is investigated (skills `no-workarounds` + `superpowers:systematic-debugging`), never suppressed.
- **rust-skills + LSP are mandatory** for all code work in this plan and any dispatched agent; grep only for string literals.
- All written artifacts (code comments, docs, commit messages suggested here) in English.
- Spec: `docs/superpowers/specs/2026-09-18-invariants-cycle-design.md`. Its decision records D1–D7 are settled — do not re-litigate.
- New public API: `#[must_use]`, full `///` docs, `# Examples` doctests that compile under `--no-default-features` (no tree-sitter API in doctests).
- New `std::fs` sites need an `arch-lint: allow(no-sync-io) reason="…"` comment; blocking IO runs on `spawn_blocking` or in synchronous notification handlers only (spec §5).
- `clippy::all`/`pedantic`/`cargo` deny; `unwrap`/`expect` deny outside tests.
- Tests: inline `#[cfg(test)]` / sibling `tests.rs`, temp workspaces via `crate::testing::temp_workspace(prefix, name)`, no sleeps, bounded waits.

---

### Task 1: Folder removal stops re-canonicalizing removed roots

**Files:**
- Modify: `src/server/state/workspace.rs:25-52` (`handle_workspace_folders_change`), `:259-263` (`workspace_folder_path` comment)
- Test: existing `removing_folder_roots_keeps_open_drops_workspace_documents` (`src/server/state/tests.rs`) pins the behavior — no new test file

**Interfaces:**
- Consumes: `self.workspace_roots: DashMap<Url, PathBuf>` (stored paths are already canonical — `set_workspace_folders` canonicalizes on insert).
- Produces: unchanged signature; removed roots now come from the stored map.

- [ ] **Step 1: Run the covering tests to establish the baseline**

Run: `cargo nextest run --workspace --all-features removing_folder_roots`
Expected: 1 passed.

- [ ] **Step 2: Replace the removed-roots collection**

In `handle_workspace_folders_change`, delete the `removed_roots` block that maps `workspace_folder_path` over `params.event.removed`, and take the paths from the map instead — removal and collection in one pass:

```rust
pub(crate) fn handle_workspace_folders_change(
    &self,
    params: DidChangeWorkspaceFoldersParams,
) -> ControlFlow<Result<()>> {
    // The old walk list's root premise moved: it is not stale but
    // meaningless.
    self.walk_cache.clear();

    // Removed roots drop their already-canonicalized paths straight from
    // the map — no disk re-canonicalization per event, and only folders
    // we actually tracked can have workspace documents to drop.
    let removed_roots: Vec<PathBuf> = params
        .event
        .removed
        .iter()
        .filter_map(|folder| {
            self.workspace_roots
                .remove(&folder.uri)
                .map(|(_, path)| path)
        })
        .collect();

    self.remove_workspace_documents_in_roots(&removed_roots);

    for folder in params.event.added {
        if let Some(path) = workspace_folder_path(&folder) {
            self.workspace_roots.insert(folder.uri, path);
        }
    }

    ControlFlow::Continue(())
}
```

Semantics note (spec §5): removal now only drops documents under roots the server actually tracked. A `removed` event for a never-added folder previously canonicalized the path anyway and could drop Workspace documents under it; such documents cannot exist (loads happen only under tracked roots), so the tightening is unobservable except in protocol-pathological cases.

- [ ] **Step 3: Correct the stale arch-lint comment**

`workspace_folder_path` now serves only ADDED folders:

```rust
fn workspace_folder_path(folder: &WorkspaceFolder) -> Option<PathBuf> {
    let path = folder.uri.to_file_path().ok()?;
    // arch-lint: allow(no-sync-io) reason="canonicalization of added workspace folders only — removal reuses the stored canonical paths"
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}
```

- [ ] **Step 4: Verify + battery**

Run: `cargo nextest run --workspace --all-features` then `cargo test --doc --workspace --all-features`, then `make battery`.
Expected: all green, zero warnings.

- [ ] **Step 5: Report** — status, files touched, test summary. Suggested commit: `Drop per-event canonicalization of removed workspace roots`.

---

### Task 2: Off-executor conversion reads (W5)

**Files:**
- Modify: `src/server/state/mod.rs` (fallback cache field + accessors + prime)
- Modify: `src/server/with_state/mod.rs` (`conversion_document` goes cache-only; extract `document_from_disk_text`)
- Modify: `src/lsp_requests/mod.rs` (`Request` trait: `STANDALONE_READS_DISK` const)
- Modify: `macros/src/request.rs` (attribute field `standalone_reads_disk` stamping the const)
- Modify: `src/lsp_requests/symbol.rs`, `src/lsp_requests/workspace_symbol_resolve.rs` (marker = true)
- Modify: `macros/src/dispatch.rs` (prime step in the URL-anchored core; conditional `spawn_blocking` around standalone hook calls)
- Test: `src/server/state/tests.rs` (cache contract), `src/server/with_state/tests.rs` (engine path)

**Interfaces:**
- Consumes: `read_document_from_disk` (`with_state/mod.rs:51`), `FileStamp`, dispatch engines.
- Produces: `ServerState::fallback_document(&self, &Url) -> Option<Document>`, `ServerState::prime_conversion_fallback(&self, Url)` (async), `Request::STANDALONE_READS_DISK: bool` (default `false`).

- [ ] **Step 1: Write the failing cache-contract test** in `src/server/state/tests.rs`:

```rust
/// The conversion-fallback cache: priming reads disk once per
/// (URL, stamp) on the blocking pool; a changed stamp re-reads; the
/// cache-only read replaces the old per-request disk read.
#[tokio::test]
async fn conversion_fallback_primes_once_per_stamp_and_rereads_on_change() {
    let root = temp_workspace("state", "fallback-cache");
    let file_path = root.join("untracked.test");
    fs::write(&file_path, "first").expect("file can be written");
    let uri = Url::from_file_path(&file_path).expect("path converts to a URL");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );

    assert!(
        state.fallback_document(&uri).is_none(),
        "nothing cached before the first prime",
    );
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state.fallback_document(&uri).expect("primed").text_contents(),
        "first",
    );

    // Same stamp: the second prime must not re-read — observable through
    // a disk write that the stamp cannot yet see (same mtime second, same
    // size): the cache keeps serving "first".
    fs::write(&file_path, "secon").expect("same-size write keeps the stamp");
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state.fallback_document(&uri).expect("still cached").text_contents(),
        "first",
        "an unchanged stamp does not re-read",
    );

    // Different size: the stamp changes, the prime re-reads.
    fs::write(&file_path, "second version").expect("stamp-changing write");
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state.fallback_document(&uri).expect("re-primed").text_contents(),
        "second version",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
```

- [ ] **Step 2: Run it — expect compile failure** (`fallback_document` / `prime_conversion_fallback` missing).

- [ ] **Step 3: Implement the cache** in `src/server/state/mod.rs`.

Field (add to `ServerState` and `with_options`):

```rust
    conversion_fallbacks: Arc<DashMap<Url, (Option<FileStamp>, Document)>>,
```

Constant + methods (private impl block):

```rust
/// Bound for the conversion-fallback cache: a pure optimization whose
/// entries may be dropped at any time, so a bound breach clears the
/// whole map rather than paying for an eviction policy.
const CONVERSION_FALLBACK_BOUND: usize = 128;

/// A disk snapshot for a file URL the server does not track, used by
/// request conversions. Cache-only: filled by
/// [`ServerState::prime_conversion_fallback`].
pub(crate) fn fallback_document(&self, url: &Url) -> Option<Document> {
    self.conversion_fallbacks
        .get(url)
        .map(|entry| entry.value().1.clone())
}

/// Primes the fallback cache for `url` off the executor: reads the disk
/// stamp, skips when the cached entry matches, and otherwise reads and
/// installs the snapshot. Failures leave the cache untouched — the
/// conversion then skips, exactly like today's failed disk read.
pub(crate) async fn prime_conversion_fallback(&self, url: Url) {
    if url.scheme() != "file" {
        return;
    }
    let state = self.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let Ok(path) = url.to_file_path() else {
            return;
        };
        // arch-lint: allow(no-sync-io) reason="the conversion-fallback stamp probe runs on the blocking pool by design"
        let stamp = std::fs::metadata(&path).ok().and_then(|meta| {
            Some((meta.modified().ok()?, meta.len()))
        });
        if state
            .conversion_fallbacks
            .get(&url)
            .is_some_and(|entry| entry.value().0 == stamp)
        {
            return;
        }
        // arch-lint: allow(no-sync-io) reason="the conversion-fallback read runs on the blocking pool by design"
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if state.conversion_fallbacks.len() >= CONVERSION_FALLBACK_BOUND {
            state.conversion_fallbacks.clear();
        }
        state.conversion_fallbacks.insert(
            url.clone(),
            (stamp, crate::server::document_from_disk_text(&url, text)),
        );
    })
    .await;
}
```

`#[derive(Debug, Clone)]` on `ServerState` already handles the new `Arc` field; add the field initializer `conversion_fallbacks: Arc::new(DashMap::new()),` in `with_options`.

- [ ] **Step 4: Refactor the reader, route `conversion_document` through the cache** in `src/server/with_state/mod.rs`:

```rust
/// Resolves the document a request's conversions run against: the
/// tracked snapshot for `url` when tracked; otherwise the primed
/// fallback cache (a disk snapshot read once per (URL, stamp) on the
/// blocking pool); for URL-less requests, the sole tracked document
/// when exactly one is tracked (the resolve-family heuristic), else
/// none.
fn conversion_document(state: &ServerState, url: Option<&Url>) -> Option<Document> {
    let Some(url) = url else {
        return state.sole_document();
    };
    state.document(url).or_else(|| state.fallback_document(url))
}

/// Builds the per-request fallback document from bytes already read.
pub(crate) fn document_from_disk_text(url: &Url, text: String) -> Document {
    #[cfg(feature = "tree-sitter")]
    let syntax = (None, None);
    #[cfg(not(feature = "tree-sitter"))]
    let syntax = ();
    Document::from_parts(
        url.clone(),
        String::new(),
        None,
        0,
        Rope::from(text),
        syntax,
    )
}

/// Reads a per-request document snapshot from a file URL. Blocking by
/// design; called only from blocking contexts (the fallback prime and
/// the standalone symbol hooks, which the dispatch engines wrap in
/// `spawn_blocking`). Never panics on external input — failures return
/// `None` and conversion is skipped.
pub(crate) fn read_document_from_disk(url: &Url) -> Option<Document> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    // arch-lint: allow(no-sync-io) reason="the dispatch fallback reads one file per request from blocking contexts only"
    let text = std::fs::read_to_string(path).ok()?;
    Some(document_from_disk_text(url, text))
}
```

Export `document_from_disk_text` from `src/server/mod.rs` next to the existing `read_document_from_disk` re-export.

- [ ] **Step 5: Add the `STANDALONE_READS_DISK` marker**

In `src/lsp_requests/mod.rs`, on the `Request` trait:

```rust
    /// Whether the standalone hooks read files from disk. The dispatch
    /// engines run such hooks on the blocking pool; the default hooks
    /// never touch the filesystem.
    const STANDALONE_READS_DISK: bool = false;
```

In `macros/src/request.rs`, extend the `#[lsp_request(...)]` attribute with an optional `standalone_reads_disk` flag argument (mirror the existing optional-argument parsing and impl-stamping pattern of `incoming_standalone` in that file): when present, the generated `impl Request` emits `const STANDALONE_READS_DISK: bool = true;`; the default stays the trait default. Align identifiers with the file's actual attribute-struct layout — the pattern there governs; the emitted item must be exactly the const above.

Then mark the two disk-reading requests — `symbol.rs`:

```rust
#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::WorkspaceSymbolParams,
    response = Option<async_lsp::lsp_types::WorkspaceSymbolResponse>,
    outgoing_standalone(self::convert_locations),
    standalone_reads_disk,
)]
```

`workspace_symbol_resolve.rs` — add `standalone_reads_disk,` to its existing attribute list.

- [ ] **Step 6: Wire the engines** in `macros/src/dispatch.rs`.

In `url_anchored_core`, insert between steps 1 and 2:

```rust
        // 1.5 Off-executor fallback prime: an untracked file URL gets its
        //     disk snapshot read once per (URL, stamp) on the blocking
        //     pool, so conversions never read disk on the executor thread.
        if let Some(untracked) = url
            .as_ref()
            .filter(|url| state.document(url).is_none())
        {
            state.prime_conversion_fallback(untracked.clone()).await;
        }
```

Replace the step-5 `None` arm with the conditional blocking-pool hop (both standalone invocations get the same treatment; shown once):

```rust
            None => {
                if <#request as crate::lsp_requests::Request>::STANDALONE_READS_DISK {
                    let state_for_pool = state.clone();
                    result = tokio::task::spawn_blocking(move || {
                        let mut result = result;
                        <#request as crate::lsp_requests::Request>::modify_response_standalone(
                            &state_for_pool,
                            &mut result,
                        );
                        result
                    })
                    .await
                    .map_err(|join_error| {
                        ResponseError::new(
                            ErrorCode::InternalError,
                            format!(
                                "{} conversion failed: {join_error}",
                                stringify!(#trait_method),
                            ),
                        )
                    })?;
                } else {
                    <#request as crate::lsp_requests::Request>::modify_response_standalone(
                        &state, &mut result,
                    );
                }
            }
```

`ResponseError::new(code, message)` takes `impl Into<String>`/`String` — if its signature differs, adapt the message construction to the actual signature (message stays lowercase, carries the join error — `err-lowercase-msg`). Apply the identical conditional wrap to BOTH `None` standalone arms of `sole_document_core` (params and response) — `workspace_symbol_resolve` is the request that makes them disk-reading.

Also update the macro's own skeleton test `engine_emits_url_anchored_skeleton`: the needle `"conversion_document"` count stays 1, and add needles `"prime_conversion_fallback"` and `"STANDALONE_READS_DISK"`.

- [ ] **Step 7: Verify**

Run: `cargo nextest run --workspace --all-features` (state, with_state, lsp_requests, macros tests), `cargo test --doc`, then `make battery` and `make test-no-default-features`.
Expected: all green — the symbol conversion tests (which call the hooks directly, still sync) and every engine test must pass unchanged; only the transport of the reads changed.

- [ ] **Step 8: Report.** Suggested commit: `Move conversion disk reads off the executor (primed fallback cache, blocking-pool standalone hooks)`.

---

### Task 3: Configurable ignore — options, walker, global matcher (W2a)

**Files:**
- Modify: `src/server/options.rs` (two public builder methods + fields)
- Modify: `src/server/state/mod.rs` (fields + accessors, `with_options` wiring)
- Modify: `src/workspace/walker.rs` (`WorkspaceWalkConfig` extension, `configure_walker`, per-root global matcher in `files()`)
- Modify: `src/server/state/workspace.rs` (`walk_blocking` builds the config from state)
- Test: `src/workspace/walker.rs` (golden tests), `src/server/state/tests.rs` (inert default)

**Interfaces:**
- Produces: `ServerOptions::with_ignore_filenames(impl IntoIterator<Item = impl Into<String>>) -> Self`, `ServerOptions::with_global_ignore_file(impl Into<PathBuf>) -> Self`; `ServerState::ignore_filenames() -> &[String]`, `ServerState::global_ignore_file() -> Option<&Path>`; `WorkspaceWalkConfig::with_ignore_filenames(...)` / `.with_global_ignore_file(Option<PathBuf>)` (pub(crate)). Task 4 consumes `ignore_filenames()`.

- [ ] **Step 1: Write the failing walker tests** in `src/workspace/walker.rs` `mod tests`:

```rust
    // Custom ignore names work with no `.git` anywhere (spec §3.3): the
    // mechanism is git-independent, gitignore syntax, cascading per
    // directory, with negation.
    #[test]
    fn custom_ignore_filenames_exclude_entries_without_git() {
        let root = temp_workspace("walker", "custom-ignore");
        fs::create_dir_all(root.join("nested")).expect("nested dir can be created");
        fs::create_dir_all(root.join("skipped-dir")).expect("skipped dir can be created");
        fs::write(root.join("a.test"), "a").expect("file can be written");
        fs::write(root.join("skip.test"), "skip").expect("file can be written");
        fs::write(root.join("keep.log"), "keep").expect("file can be written");
        fs::write(root.join("drop.log"), "drop").expect("file can be written");
        fs::write(root.join("skipped-dir/x.test"), "x").expect("file can be written");
        fs::write(root.join("nested/inner.test"), "inner").expect("file can be written");
        fs::write(root.join(".mylspignore"), "skip.test\nskipped-dir/\n*.log\n!keep.log\n")
            .expect("custom ignore file can be written");

        let walker = WorkspaceWalker::new(
            std::slice::from_ref(&root),
            WorkspaceWalkConfig::default()
                .with_ignore_filenames([".mylspignore"]),
        )
        .expect("walker can be created");
        let canonical = &walker.roots()[0];
        assert_eq!(
            walker.files().expect("walk succeeds"),
            vec![
                canonical.join("a.test"),
                canonical.join("keep.log"),
                canonical.join("nested/inner.test"),
            ],
        );

        // Cascading: a nested .mylspignore drops only what it names.
        fs::write(root.join("nested/.mylspignore"), "inner.test\n")
            .expect("nested ignore file can be written");
        assert_eq!(
            walker.files().expect("walk succeeds"),
            vec![canonical.join("a.test"), canonical.join("keep.log")],
        );

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    // The global ignore file applies to every root regardless of git
    // presence; patterns anchor at each root (git per-repo semantics).
    #[test]
    fn global_ignore_file_filters_every_root() {
        let root = temp_workspace("walker", "global-ignore");
        let sibling = temp_workspace("walker", "global-ignore-b");
        fs::create_dir_all(root.join("vendor")).expect("vendor dir can be created");
        fs::write(root.join("vendor/v.test"), "v").expect("file can be written");
        fs::write(root.join("top.test"), "t").expect("file can be written");
        fs::write(root.join("deep.test"), "d").expect("file can be written");
        let global = root.join("global.ignore");
        fs::write(&global, "/top.test\nvendor/\ndeep.test\n")
            .expect("global ignore file can be written");
        fs::write(sibling.join("s.test"), "s").expect("file can be written");
        fs::write(sibling.join("deep.test"), "deep").expect("file can be written");

        let walker = WorkspaceWalker::new(
            &[root.clone(), sibling.clone()],
            WorkspaceWalkConfig::default().with_global_ignore_file(Some(global.clone())),
        )
        .expect("walker can be created");
        let files = walker.files().expect("walk succeeds");
        assert!(files.contains(&root.join("global.ignore")), "the global file itself is a plain file: {files:?}");
        assert!(!files.contains(&root.join("top.test")), "anchored pattern drops the root file");
        assert!(!files.contains(&root.join("vendor/v.test")), "directory pattern prunes");
        assert!(
            !files.contains(&root.join("deep.test")) && !files.contains(&sibling.join("deep.test")),
            "unanchored pattern matches in every root",
        );
        assert!(files.contains(&sibling.join("s.test")));

        fs::remove_dir_all(root).expect("temp workspace can be removed");
        fs::remove_dir_all(sibling).expect("temp workspace can be removed");
    }
```

- [ ] **Step 2: Run — expect compile failure** (`with_ignore_filenames` missing).

- [ ] **Step 3: Implement.**

`src/server/options.rs` — fields on `ServerOptions`:

```rust
    pub(crate) ignore_filenames: Vec<String>,
    pub(crate) global_ignore_file: Option<std::path::PathBuf>,
```

methods (follow the file's `#[must_use]` + doctest pattern):

```rust
    /// Names of ignore files honored during workspace walks — gitignore
    /// syntax, matched per directory with cascading, independent of git
    /// presence (a project without `.git` still honors them, unlike
    /// `.gitignore` itself). Session-fixed, like matchers. Unconfigured
    /// (the default): no ignore files beyond the built-in git family,
    /// and the walk is byte-identical to a server that never set this.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_ignore_filenames([".mylspignore"]);
    /// ```
    #[must_use]
    pub fn with_ignore_filenames(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.ignore_filenames = names.into_iter().map(Into::into).collect();
        self
    }

    /// Sets one global ignore file (gitignore syntax) applied to every
    /// workspace walk across all roots, regardless of git presence. The
    /// location is the downstream server's choice — the framework
    /// defines no default path. Unset (the default): no global
    /// exclusions.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_global_ignore_file("/etc/my-server/ignore");
    /// ```
    #[must_use]
    pub fn with_global_ignore_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.global_ignore_file = Some(path.into());
        self
    }
```

`src/server/state/mod.rs` — fields `ignore_filenames: Arc<[String]>`, `global_ignore_file: Option<PathBuf>` on `ServerState`, wired in `with_options` from `options`, accessors:

```rust
    /// The configured ignore-file names (`ServerOptions::with_ignore_filenames`).
    pub(crate) fn ignore_filenames(&self) -> &[String] {
        &self.ignore_filenames
    }

    /// The configured global ignore file path, if any.
    pub(crate) fn global_ignore_file(&self) -> Option<&std::path::Path> {
        self.global_ignore_file.as_deref()
    }
```

`src/workspace/walker.rs` — extend the config and the walk:

```rust
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceWalkConfig {
    include_hidden_files: bool,
    respect_ignore_files: bool,
    ignore_filenames: Vec<String>,
    global_ignore_file: Option<PathBuf>,
}
```

(+ the two `pub(crate)` builders, same style as the existing pair; `Default` keeps `Vec::new()` / `None`.)

`configure_walker` gains:

```rust
    for name in &config.ignore_filenames {
        builder.add_custom_ignore_filename(name);
    }
```

VERIFY against the local `ignore` 0.4.33 source (`~/.cargo/registry/src/*/ignore-0.4.33/src/walk.rs`): custom ignore filenames are honored independently of the `standard_filters(false)` setup in `configure_walker`. If the crate ties them to a filter flag, enable that flag for the names and record the finding in the task report.

`files()` — per-root global matcher, checked in the visitor before sending:

```rust
    pub(crate) fn files(&self) -> ServerResult<Vec<PathBuf>> {
        let (sender, receiver) = mpsc::channel();

        for root in &self.roots {
            let mut builder = WalkBuilder::new(root);
            configure_walker(&mut builder, &self.config);
            let global = self
                .config
                .global_ignore_file
                .as_ref()
                .map(|file| Arc::new(global_ignore_matcher(file, root)));

            builder.build_parallel().run(|| {
                let sender = sender.clone();
                let global = global.clone();
                Box::new(move |entry| match entry {
                    Ok(entry) => {
                        // arch-lint: allow(no-sync-io) reason="the ignore-crate walk is a synchronous batch scan by design"
                        if entry.file_type().is_some_and(|ty| ty.is_file())
                            && global
                                .as_ref()
                                .is_none_or(|matcher| {
                                    !matcher.matched(entry.path(), false).is_ignore()
                                })
                        {
                            // The receiver outlives every send: it is
                            // dropped only after all walks have joined.
                            let _ = sender.send(entry.into_path());
                        }
                        WalkState::Continue
                    }
                    Err(error) => {
                        tracing::warn!("skipping unreadable workspace entry: {error}");
                        WalkState::Continue
                    }
                })
            });
        }

        drop(sender);
        let mut files = receiver.into_iter().collect::<Vec<_>>();
        files.sort();
        Ok(files)
    }
```

and the compiler (also in `walker.rs`; add `use ignore::gitignore::{Gitignore, GitignoreBuilder};` and `use std::sync::Arc;`):

```rust
/// Compiles the global ignore file against one walk root: gitignore
/// syntax, patterns anchored at the root — git's per-repo semantics. A
/// missing or unreadable file matches nothing (warned, never fatal).
fn global_ignore_matcher(file: &Path, root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    match fs::read_to_string(file) {
        Ok(text) => {
            for line in text.lines() {
                if let Err(error) = builder.add_line(None, line) {
                    tracing::warn!(
                        "skipping bad pattern '{line}' in '{}': {error}",
                        file.display(),
                    );
                }
            }
        }
        Err(error) => tracing::warn!(
            "skipping unreadable global ignore file '{}': {error}",
            file.display(),
        ),
    }
    builder.build()
}
```

SPEC DEVIATION RECORDED (tell the controller): the spec's "compiled matcher cached by stamp" is dropped — compilation happens once per walk inside the walk's existing `spawn_blocking` hop, and `WalkCache` already bounds how often walks run at all. Caching a compiled matcher would optimize a path the walk cache already gates. Simpler code, same observable behavior.

`src/server/state/workspace.rs` — `walk_blocking` builds the config from state (the compile + read of the global file thereby rides the same blocking hop):

```rust
        let walked = tokio::task::spawn_blocking(move || -> ServerResult<Vec<WalkedFile>> {
            let config = WorkspaceWalkConfig::default()
                .with_ignore_filenames(state.ignore_filenames().iter().cloned())
                .with_global_ignore_file(state.global_ignore_file().map(ToOwned::to_owned));
            let walker = WorkspaceWalker::new(&roots, config)?;
            // ... rest unchanged
```

(adjust the surrounding closure to use the `config` variable; the `state` capture already exists).

- [ ] **Step 4: Inertness pin** in `src/server/state/tests.rs` — the existing walk tests with default options are the byte-identical pin (they must stay green untouched). Add one explicit default-check:

```rust
#[test]
fn ignore_configuration_defaults_to_inert() {
    let options = ServerOptions::default();
    assert!(options.ignore_filenames.is_empty());
    assert!(options.global_ignore_file.is_none());
}
```

- [ ] **Step 5: Verify + battery** (`make battery`, plus `make test-no-default-features` — none of this is feature-gated, but the legs must both stay green). Report. Suggested commit: `Add configurable ignore: custom ignore filenames and a global ignore file`.

---

### Task 4: Ignore-file watching and walk-cache invalidation (W2b)

**Files:**
- Modify: `src/server/state/mod.rs` (`watcher_globs` extension)
- Modify: `src/server/state/documents.rs` (`handle_watched_files_change` ignore branch + `is_ignore_file`)
- Test: `src/server/state/tests.rs` (invalidation pins + the spec §6 open-document immunity pin + updated globs expectation)

**Interfaces:**
- Consumes: `ServerState::ignore_filenames()` (Task 3).
- Produces: none (internal).

- [ ] **Step 1: Write the failing tests** in `src/server/state/tests.rs`:

```rust
/// An event on a configured ignore file invalidates the walk cache: a
/// file the new rules exclude falls out, a file they newly include loads
/// — through the ordinary refresh path, no new state channel. Open
/// documents are never subject to ignore rules (spec §6).
#[tokio::test]
async fn ignore_file_events_invalidate_the_walk_cache_not_documents() {
    let root = temp_workspace("state", "ignore-watch");
    fs::write(root.join("a.test"), "a").expect("file can be written");
    fs::write(root.join("b.test"), "b").expect("file can be written");
    fs::write(root.join(".mylspignore"), "b.test\n").expect("ignore file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_ignore_filenames([".mylspignore"]),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    let a_uri = urls
        .iter()
        .find(|url| url.as_str().ends_with("a.test"))
        .expect("a.test is walked")
        .clone();
    let b_uri = urls
        .iter()
        .find(|url| url.as_str().ends_with("b.test"))
        .expect("b.test is walked");
    assert_eq!(urls.len(), 2, "no ignore event yet: both files are in");

    // Open b.test: even after the ignore rules drop it from the walk, an
    // open document stays tracked and reportable (spec §6, pin 2).
    let mut open_state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_ignore_filenames([".mylspignore"]),
    );
    open_state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&open_state);
    open_document(&mut open_state, b_uri.clone(), "open b");
    open_state.set_file_watching(true);
    open_state.set_watchers_registered(true);
    let urls = open_state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(urls, vec![b_uri.clone()], "the open document reports");

    // The event flips the rules: b.test is now ignored, and a freshly
    // created c.test (previously excluded) — keep it simple: drop b only.
    fs::remove_file(root.join(".mylspignore")).expect("ignore file can be removed");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".mylspignore")).expect("path converts"),
        FileChangeType::DELETED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(urls.len(), 2, "without the ignore file both files walk again");
    assert!(state.document(&a_uri).is_some());
    assert!(state.document(b_uri).is_some());

    // The open document survives the same event untouched.
    let _ = open_state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".mylspignore")).expect("path converts"),
        FileChangeType::DELETED,
    )]);
    let urls = open_state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(urls, vec![b_uri.clone()], "open document still reports");
    assert!(
        open_state.document(&b_uri).is_some(),
        "an open document is never evicted by ignore rules",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// The built-in `.gitignore` closes the walk-cache spec's staleness
/// limitation: an edit event invalidates, so membership changes are seen
/// on the next poll.
#[tokio::test]
async fn gitignore_edit_events_invalidate_the_walk_cache() {
    let root = temp_workspace("state", "gitignore-watch");
    fs::write(root.join("a.test"), "a").expect("file can be written");
    fs::write(root.join(".gitignore"), "\n").expect("gitignore exists");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(urls.len(), 1);

    fs::write(root.join(".gitignore"), "a.test\n").expect("a.test becomes ignored");
    fs::write(root.join("b.test"), "b").expect("file can be written");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".gitignore")).expect("path converts"),
        FileChangeType::CHANGED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls.len(),
        1,
        "the gitignore edit re-walks: a.test excluded, b.test included",
    );
    assert!(urls[0].as_str().ends_with("b.test"));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
```

Also update the existing `watcher_globs_sort_and_dedup_across_matchers` expectation: with no ignore names configured the built-ins still join, so the assertion becomes `["**/.gitignore", "**/.ignore", "*.a", "*.m", "*.z"]` (sorted), with the message `"globs shared across matchers register once, in a stable order; the built-in ignore names always register"`.

- [ ] **Step 2: Run — expect the new tests to fail** (no ignore branch: the `.mylspignore` DELETED event currently falls into the untracked-URI `continue` without invalidating) **and the globs test to fail** (built-ins absent).

- [ ] **Step 3: Implement.**

`src/server/state/mod.rs` — `watcher_globs`:

```rust
    /// Watcher glob patterns: the matchers' url globs plus the ignore
    /// file names the walk honors — the configured names and the
    /// built-ins (`.gitignore`, `.ignore`), whose edits change walk
    /// membership. Sorted, deduplicated.
    pub(crate) fn watcher_globs(&self) -> Vec<String> {
        let mut globs: Vec<_> = self.matchers.watcher_globs();
        globs.push(String::from("**/.gitignore"));
        globs.push(String::from("**/.ignore"));
        for name in self.ignore_filenames() {
            globs.push(format!("**/{name}"));
        }
        globs.sort();
        globs.dedup();
        globs
    }
```

`src/server/state/documents.rs` — at the top of the event loop in `handle_watched_files_change`, before the CREATED/DELETED check:

```rust
            if self.is_ignore_file(&event.uri) {
                // Ignore-file events change walk membership semantics,
                // not documents: invalidate the walk list and leave the
                // document store alone — open documents are never
                // subject to ignore rules (spec §6).
                self.walk_cache.invalidate();
                continue;
            }
```

and the predicate (same impl block):

```rust
    /// Whether `uri` names an ignore file the walk honors: a configured
    /// custom name or a built-in (`.gitignore`, `.ignore`).
    fn is_ignore_file(&self, uri: &Url) -> bool {
        uri.to_file_path().is_ok_and(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name == ".gitignore"
                    || name == ".ignore"
                    || self.ignore_filenames().iter().any(|configured| *configured == name)
            })
        })
    }
```

- [ ] **Step 4: Verify + battery + dupes** (`make battery`, `make test-no-default-features`, `make dupes` — two new watcher tests in one file; if the dupes gate twins them, merge per the spec-matrix idiom, never an ignore entry). Report. Suggested commit: `Watch ignore files and invalidate the walk cache on their events`.

---

### Task 5: Capability-gated dispatch — inventory, gate, fixtures green (W4a)

**Files:**
- Modify: `src/server/inventory.rs` (resolve names + predicates, `gateable`, `dispatch_allowed`, `warn_once_unadvertised`, tests)
- Modify: `src/server/state/mod.rs` (passthroughs)
- Modify: `macros/src/dispatch.rs` (gate at the top of both cores; skeleton-test needles)
- Modify: `src/server/testing.rs` (`EchoServer` advertises hover), `src/server/tests/*` fixtures that drive requests, `src/server/with_state/tests.rs` (seeding), `crate::testing` (`src/testing.rs`: `all_request_capabilities` + `allow_all_methods`)
- Test: `src/server/inventory.rs` (unit), existing suites stay green

**Interfaces:**
- Produces: `MethodInventory::dispatch_allowed(&self, &str) -> bool`, `MethodInventory::warn_once_unadvertised(&self, &str) -> bool`, `ServerState::dispatch_allowed(&self, &'static str) -> bool`, `ServerState::warn_once_unadvertised(&self, &'static str)`, `crate::testing::all_request_capabilities() -> ServerCapabilities`, `crate::testing::allow_all_methods(&mut ServerState)` (both `#[cfg(test)]`).

- [ ] **Step 1: Failing inventory tests** in `src/server/inventory.rs` `mod tests`:

```rust
    // The dispatch gate: a gateable method absent from the capabilities
    // is not dispatchable; the type-hierarchy trio (no capability field
    // in lsp-types 0.95.1) is exempt; resolve methods gate on their
    // provider's resolve_provider option.
    #[test]
    fn dispatch_allowed_follows_advertisement_except_the_type_hierarchy_trio() {
        let none = MethodInventory::from_capabilities(&ServerCapabilities::default());
        assert!(!none.dispatch_allowed("hover"));
        assert!(none.dispatch_allowed("prepare_type_hierarchy"));
        assert!(none.dispatch_allowed("supertypes"));
        assert!(none.dispatch_allowed("subtypes"));

        let mut caps = ServerCapabilities::default();
        caps.hover_provider = Some(HoverProviderCapability::Simple(true));
        let hover = MethodInventory::from_capabilities(&caps);
        assert!(hover.dispatch_allowed("hover"));
        assert!(!hover.dispatch_allowed("definition"));
    }

    #[test]
    fn resolve_methods_gate_on_their_providers_resolve_option() {
        let mut caps = ServerCapabilities::default();
        caps.completion_provider = Some(CompletionOptions::default());
        let no_resolve = MethodInventory::from_capabilities(&caps);
        assert!(!no_resolve.dispatch_allowed("completion"));
        assert!(!no_resolve.dispatch_allowed("completion_resolve"));

        caps.completion_provider = Some(CompletionOptions {
            resolve_provider: Some(true),
            ..CompletionOptions::default()
        });
        let with_resolve = MethodInventory::from_capabilities(&caps);
        assert!(with_resolve.dispatch_allowed("completion"));
        assert!(with_resolve.dispatch_allowed("completion_resolve"));
    }

    #[test]
    fn warn_once_unadvertised_fires_once_per_method() {
        let none = MethodInventory::from_capabilities(&ServerCapabilities::default());
        assert!(none.warn_once_unadvertised("hover"));
        assert!(!none.warn_once_unadvertised("hover"), "once per method");
    }
```

Fix the second test's style: drop `== false` — write `assert!(!no_resolve.dispatch_allowed("completion_resolve"));`.

- [ ] **Step 2: Run — compile failure** (`dispatch_allowed` missing).

- [ ] **Step 3: Implement the inventory.**

`METHOD_NAMES`: append the six resolve names (`"completion_resolve"`, `"code_action_resolve"`, `"link_resolve"`, `"code_lens_resolve"`, `"inlay_hint_resolve"`, `"workspace_symbol_resolve"`) and update the doc comment: the resolve family now IS present — it gates dispatch on its provider's resolve option (its defaults still never produce `method_not_implemented`, so `warn_once_default` behavior for them is simply never triggered).

`InventoryInner` gains `gateable: Box<[bool]>`; `new()` and `from_capabilities()` build it via:

```rust
/// Methods the dispatch gate never blocks: `lsp_types` 0.95.1 carries
/// no capability field for them, so "not advertised" is not decidable —
/// always-allowed (spec D7).
fn gateable(method: &str) -> bool {
    !matches!(
        method,
        "prepare_type_hierarchy" | "supertypes" | "subtypes"
    )
}
```

Predicates (add to `advertised_continued`):

```rust
        "completion_resolve" => caps
            .completion_provider
            .as_ref()
            .is_some_and(|options| options.resolve_provider == Some(true)),
        "code_action_resolve" => caps.code_action_provider.as_ref().is_some_and(
            |provider| matches!(provider, CodeActionProviderCapability::Options(options)
                if options.resolve_provider == Some(true)),
        ),
        "link_resolve" => caps
            .document_link_provider
            .as_ref()
            .is_some_and(|options| options.resolve_provider == Some(true)),
        "code_lens_resolve" => caps
            .code_lens_provider
            .as_ref()
            .is_some_and(|options| options.resolve_provider == Some(true)),
        "inlay_hint_resolve" => caps.inlay_hint_provider.as_ref().is_some_and(
            |provider| matches!(provider, OneOf::Right(options)
                if options.resolve_provider == Some(true)),
        ),
        "workspace_symbol_resolve" => caps.workspace_symbol_provider.as_ref().is_some_and(
            |provider| matches!(provider, OneOf::Right(options)
                if options.resolve_provider == Some(true)),
        ),
```

`MethodInventory` methods:

```rust
    /// Whether the dispatch gate lets `method` run: gateable methods
    /// must be advertised; the type-hierarchy trio (no capability field
    /// upstream) is always allowed. Before `initialize` nothing is
    /// advertised, so only the exempt methods dispatch — requests are
    /// not allowed before `initialize` anyway.
    pub(crate) fn dispatch_allowed(&self, method: &str) -> bool {
        index_of(method)
            .is_none_or(|index| !self.inner.gateable[index] || self.inner.advertised[index])
    }

    /// Warns once per method blocked by the dispatch gate: implemented
    /// but not advertised is an implementor bug, and it must be loud.
    pub(crate) fn warn_once_unadvertised(&self, method: &str) -> bool {
    let Some(index) = index_of(method) else { return false; };
        if self.inner.warned[index].swap(true, Ordering::Relaxed) {
            return false;
        }
        tracing::warn!(
            "LSP method '{method}' is implemented but not advertised in the \
             server capabilities; the request was rejected — advertise the \
             capability or remove the override",
        );
        true
    }
```

(fix the indentation of the `let Some(index)` line to match the file's style.)

`src/server/state/mod.rs` passthroughs:

```rust
    /// Whether the dispatch gate lets `method` run (see
    /// [`MethodInventory::dispatch_allowed`]).
    pub(crate) fn dispatch_allowed(&self, method: &'static str) -> bool {
        self.advertised_methods.dispatch_allowed(method)
    }

    /// Warns once per method rejected by the dispatch gate (see
    /// [`MethodInventory::warn_once_unadvertised`]).
    pub(crate) fn warn_once_unadvertised(&self, method: &'static str) {
        self.advertised_methods.warn_once_unadvertised(method);
    }
```

- [ ] **Step 4: The gate in the macro.** In `macros/src/dispatch.rs`, `engine()` prepends the gate to the core (both engines share it):

```rust
fn engine(row: &DispatchRow) -> TokenStream {
    let DispatchRow { trait_method, alsp, request, resolve } = row;
    let gate = quote! {
        // 0. Capability gate: a method absent from the final
        //    ServerCapabilities never activates — reject before any
        //    conversion or handler runs (spec W4). The type-hierarchy
        //    trio is exempt upstream; lifecycle methods never pass
        //    through here.
        if !state.dispatch_allowed(stringify!(#trait_method)) {
            state.warn_once_unadvertised(stringify!(#trait_method));
            return Err(ResponseError::new(
                ErrorCode::METHOD_NOT_FOUND,
                concat!(stringify!(#trait_method), " is not advertised in the server capabilities"),
            ));
        }
    };
    let core = if *resolve {
        sole_document_core(trait_method, request)
    } else {
        url_anchored_core(trait_method, request)
    };
    let gated = quote! { #gate #core };
    wrapped(alsp, request, &gated)
}
```

Update `engine_emits_url_anchored_skeleton` needles: add `"dispatch_allowed"` and `"METHOD_NOT_FOUND"`.

- [ ] **Step 5: Fixtures and seeding.**

`src/server/testing.rs` — `EchoServer` advertises what it serves:

```rust
impl Server for EchoServer {
    fn server_capabilities(
        _client: async_lsp::lsp_types::ClientCapabilities,
    ) -> Option<async_lsp::lsp_types::ServerCapabilities> {
        Some(async_lsp::lsp_types::ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        })
    }
    // hover unchanged
}
```

`src/testing.rs` — two `#[cfg(test)]` helpers:

```rust
/// Capabilities advertising every gateable dispatch method — the
/// dispatch-row wire fixture (spec W4). The type-hierarchy trio is
/// exempt from the gate and needs no field. Constructor names for the
/// provider-capability enums follow `inventory.rs`'s predicates; verify
/// any uncertain one with an LSP hover — the assertion test below makes
/// a wrong construction fail loudly, not silently.
pub(crate) fn all_request_capabilities() -> ServerCapabilities {
    use async_lsp::lsp_types::*;
    let tokens = SemanticTokensOptions {
        legend: SemanticTokensLegend {
            token_types: Vec::new(),
            token_modifiers: Vec::new(),
        },
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
        range: Some(true),
        ..SemanticTokensOptions::default()
    };
    ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        declaration_provider: Some(DeclarationCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_link_provider: Some(DocumentLinkOptions {
            resolve_provider: Some(true),
            ..DocumentLinkOptions::default()
        }),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            ..RenameOptions::default()
        })),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_range_formatting_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: String::new(),
            ..DocumentOnTypeFormattingOptions::default()
        }),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(true)),
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(true),
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            will_save_wait_until: Some(true),
            ..TextDocumentSyncOptions::default()
        })),
        color_provider: Some(ColorProviderCapability::Simple(true)),
        call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
        moniker_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Right(WorkspaceSymbolOptions {
            resolve_provider: Some(true),
            ..WorkspaceSymbolOptions::default()
        })),
        inlay_hint_provider: Some(OneOf::Right(InlayHintOptions {
            resolve_provider: Some(true),
            ..InlayHintOptions::default()
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: Vec::new(),
        }),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            tokens,
        )),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            ..CompletionOptions::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            resolve_provider: Some(true),
            ..CodeActionOptions::default()
        })),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            ..DiagnosticOptions::default()
        })),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        inline_value_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions::default()),
        workspace: Some(WorkspaceServerCapabilities {
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                will_create: Some(FileOperationRegistrationOptions {
                    filters: Vec::new(),
                }),
                will_rename: Some(FileOperationRegistrationOptions {
                    filters: Vec::new(),
                }),
                will_delete: Some(FileOperationRegistrationOptions {
                    filters: Vec::new(),
                }),
                ..WorkspaceFileOperationsServerCapabilities::default()
            }),
            ..WorkspaceServerCapabilities::default()
        }),
        ..ServerCapabilities::default()
    }
}
```

Its self-verifying test (same file, `#[cfg(test)]` in `src/testing.rs` or next to the inventory tests — put it in `src/server/inventory.rs` tests, where `METHOD_NAMES` lives):

```rust
    // The all-request fixture is complete: every gateable dispatch
    // method comes out advertised, so the dispatch-row wire test drives
    // the whole table through an open gate.
    #[test]
    fn all_request_capabilities_advertise_every_gateable_method() {
        let caps = crate::testing::all_request_capabilities();
        let inventory = MethodInventory::from_capabilities(&caps);
        for method in super::METHOD_NAMES {
            assert!(
                inventory.dispatch_allowed(method),
                "{method} must dispatch under the all-request fixture",
            );
        }
    }
```

And the gate opener:

```rust
/// Test-only: opens the dispatch gate entirely (every method allowed).
pub(crate) fn allow_all_methods(state: &mut ServerState) {
    state.set_advertised_methods_all();
}
```

Implement `all_request_capabilities` mechanically (complete code at implementation time — every field listed in `inventory.rs`'s predicates set to advertise; verify with an inventory assertion test: `MethodInventory::from_capabilities(&all_request_capabilities())` must have `dispatch_allowed` true for every `METHOD_NAMES` entry except nothing). For `allow_all_methods`, add a `#[cfg(test)]` method on `MethodInventory`:

```rust
    /// Test-only: every method allowed through the dispatch gate.
    #[cfg(test)]
    pub(crate) fn allow_all() -> Self {
        Self {
            inner: Arc::new(InventoryInner {
                advertised: vec![true; METHOD_NAMES.len()].into(),
                gateable: METHOD_NAMES.iter().map(|name| gateable(name)).collect(),
                warned: METHOD_NAMES.iter().map(|_| AtomicBool::new(false)).collect(),
            }),
        }
    }
```

and on `ServerState`:

```rust
    /// Test-only: opens the dispatch gate for every method.
    #[cfg(test)]
    pub(crate) fn set_advertised_methods_all(&mut self) {
        self.advertised_methods = MethodInventory::allow_all();
    }
```

Then make every existing test that drives a request through `LanguageServerWithState` (the dispatch engines) seed the gate: `with_state/tests.rs` adds `crate::testing::allow_all_methods(&mut wrapper.state);`-style setup (the tests module can reach the private `state` field — child module), and wire-tier fixtures that drive all methods (the dispatch-row test) advertise via `all_request_capabilities()` in their server's `server_capabilities`. Sweep by running the suites and fixing each failure at its fixture — the failures name the tests; do not loosen the gate.

- [ ] **Step 6: Verify + battery + dupes.** Report. Suggested commit: `Gate dispatch on advertised capabilities (breaking: unadvertised methods answer METHOD_NOT_FOUND)` — the breaking flag per product.md.

---

### Task 6: Capability-gate wire pins (W4b)

**Files:**
- Test: `src/server/tests/` (the wire file owning dispatch tests — locate `wired_methods_dispatch` / `unknown_methods_answer_method_not_found`)

**Interfaces:**
- Consumes: Task 5's gate + fixtures.

- [ ] **Step 1: Write the failing wire pins** (in the wire dispatch test file — locate it via the existing `wired_methods_dispatch` / `unknown_methods_answer_method_not_found` tests; local channel-asserting servers stay in this file, `GatedServer`/`PanickingServer` idiom):

```rust
// Spec W4 over the real stack: an unadvertised method answers -32601
// and its handler never runs. The gate message is the discriminator.
#[tokio::test]
async fn unadvertised_methods_answer_method_not_found_and_never_run_handlers() {
    #[derive(Clone)]
    struct CountingServer {
        entered: Arc<AtomicUsize>,
    }

    impl Server for CountingServer {
        fn hover(
            &self,
            _state: crate::server::ServerState,
            params: async_lsp::lsp_types::HoverParams,
        ) -> impl Future<Output = crate::server::ServerResult<Option<async_lsp::lsp_types::Hover>>>
        + Send {
            let entered = Arc::clone(&self.entered);
            async move {
                entered.fetch_add(1, Ordering::Relaxed);
                Ok(crate::server::testing::echo_hover(
                    params.text_document_position_params.position,
                ))
            }
        }
    }

    let entered = Arc::new(AtomicUsize::new(0));
    let (mut client, _handle) =
        spawn_wire_server(CountingServer {
            entered: Arc::clone(&entered),
        });
    let result = client.initialize_client(&["utf-8"]).await;
    assert!(
        result["capabilities"]["hoverProvider"].is_null(),
        "precondition: the fixture advertises nothing",
    );

    client
        .notify("textDocument/didOpen", did_open("file:///gate.test", "body"))
        .await;
    let response = client
        .request(2, "textDocument/hover", hover_params("file:///gate.test", 0))
        .await;

    let error = response["error"].as_object().expect("gate rejects");
    assert_eq!(error["code"].as_i64(), Some(-32601));
    assert!(
        error["message"]
            .as_str()
            .expect("message is a string")
            .contains("is not advertised"),
        "the gate message names the violation: {error:?}",
    );
    assert_eq!(
        entered.load(Ordering::Relaxed),
        0,
        "the handler never runs for an unadvertised method",
    );
}

// The resolve family gates on its provider's resolve option, and an
// advertised provider dispatches.
#[tokio::test]
async fn resolve_gates_follow_the_providers_resolve_option() {
    #[derive(Clone)]
    struct SymbolServer {
        resolve_options: bool,
    }

    impl Server for SymbolServer {
        fn server_capabilities(
            _client: async_lsp::lsp_types::ClientCapabilities,
        ) -> Option<async_lsp::lsp_types::ServerCapabilities> {
            let provider = if self.resolve_options {
                async_lsp::lsp_types::WorkspaceSymbolOptions::default()
            } else {
                async_lsp::lsp_types::WorkspaceSymbolOptions {
                    resolve_provider: Some(true),
                    ..async_lsp::lsp_types::WorkspaceSymbolOptions::default()
                }
            };
            Some(async_lsp::lsp_types::ServerCapabilities {
                workspace_symbol_provider: Some(async_lsp::lsp_types::OneOf::Right(provider)),
                ..async_lsp::lsp_types::ServerCapabilities::default()
            })
        }

        fn symbol(
            &self,
            _state: crate::server::ServerState,
            _params: async_lsp::lsp_types::WorkspaceSymbolParams,
        ) -> impl Future<
            Output = crate::server::ServerResult<
                Option<async_lsp::lsp_types::WorkspaceSymbolResponse>,
            >,
        > + Send {
            async { Ok(None) }
        }

        fn workspace_symbol_resolve(
            &self,
            _state: crate::server::ServerState,
            params: async_lsp::lsp_types::WorkspaceSymbol,
        ) -> impl Future<Output = crate::server::ServerResult<async_lsp::lsp_types::WorkspaceSymbol>>
        + Send {
            async move { Ok(params) }
        }
    }

    // Advertised WITHOUT resolve_provider: symbol dispatches, resolve is
    // gated off.
    let (mut client, _handle) = spawn_wire_server(SymbolServer {
        resolve_options: true,
    });
    client.initialize_client(&["utf-8"]).await;
    let symbol = client
        .request(2, "workspace/symbol", json!({ "query": "" }))
        .await;
    assert!(symbol.get("result").is_some(), "symbol dispatches: {symbol:?}");
    let resolve = client
        .request(
            3,
            "workspaceSymbol/resolve",
            json!({ "name": "x", "kind": 1, "location": {
                "uri": "file:///x.test", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
            }}),
        )
        .await;
    assert_eq!(
        resolve["error"]["code"].as_i64(),
        Some(-32601),
        "resolve without the provider's resolve option is gated: {resolve:?}",
    );

    // Advertised WITH resolve_provider: both dispatch.
    let (mut client, _handle) = spawn_wire_server(SymbolServer {
        resolve_options: false,
    });
    client.initialize_client(&["utf-8"]).await;
    let resolve = client
        .request(
            2,
            "workspaceSymbol/resolve",
            json!({ "name": "x", "kind": 1, "location": {
                "uri": "file:///x.test", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
            }}),
        )
        .await;
    assert!(
        resolve.get("result").is_some(),
        "resolve dispatches under the advertised resolve option: {resolve:?}",
    );
}

// The type-hierarchy trio has no capability field upstream (lsp-types
// 0.95.1): the gate passes them, and the trait default's own
// METHOD_NOT_FOUND is the reply — distinguishable from the gate's by
// message.
#[tokio::test]
async fn type_hierarchy_trio_passes_the_gate() {
    #[derive(Clone)]
    struct BareServer;
    impl Server for BareServer {}

    let (mut client, _handle) = spawn_wire_server(BareServer);
    client.initialize_client(&["utf-8"]).await;
    client
        .notify("textDocument/didOpen", did_open("file:///trio.test", "body"))
        .await;
    let response = client
        .request(
            2,
            "textDocument/prepareTypeHierarchy",
            hover_params("file:///trio.test", 0),
        )
        .await;
    let error = response["error"].as_object().expect("default replies");
    assert_eq!(error["code"].as_i64(), Some(-32601));
    assert!(
        !error["message"]
            .as_str()
            .expect("message is a string")
            .contains("is not advertised"),
        "the reply is the trait default's, not the gate's: {error:?}",
    );
}
```

Adapt imports to the file's existing header (the idioms — `bounded`, `spawn_wire_server`, `did_open`, `hover_params` — come from `crate::server::testing`; `Ordering`/`AtomicUsize` per the file's style; add any missing `Server`/`ServerResult`/`Future` imports the local servers there already use). The `CountingServer`/`SymbolServer` locals stay in this file — single consumers, per the `GatedServer` convention.

- [ ] **Step 2: Run — first fails** (handler runs today when unadvertised... after Task 5 it already answers -32601: this test then PASSES immediately; that is expected — it is the pin, not the driver. Confirm each of the three by temporarily flipping the fixture's advertisement if a red-first demonstration is wanted, then restore.)

- [ ] **Step 3: Verify + battery + dupes.** Report. Suggested commit: `Pin the capability gate over the wire in both directions`.

---

### Task 7: Docs sync + cycle close

**Files:**
- Modify: `.claude/rules/structure.md` (watcher-glob + ignore sentences), `CLAUDE.md` (Matching & workspace scanning paragraph), `README.md` (only if it enumerates `ServerOptions` — check; otherwise skip)

**Interfaces:** none.

- [ ] **Step 1: Invariant comments for the documented blocking sites (spec §5 "keep, document" rows)** — in `src/server/state/documents.rs`, at each open-document parse path (didOpen insert, didChange incremental reparse + failure re-read, didSave disk fallback, watched-files re-read), ensure a comment of the form already present on the watched-files read: `notification handlers must stay synchronous per the LSP spec and async-lsp, so the parse runs on the calling thread — spec §5, invariants cycle`. One comment per distinct site; do not reword existing accurate ones. In the oneshot module, one comment on the inline walk/read path: `no tokio runtime is current in CLI batch use, so the walk and reads run inline on the calling thread — spec §5, invariants cycle`.
- [ ] **Step 2: Sync the rules** — `.claude/rules/structure.md`, "Matching and workspace scanning": add that `ServerOptions::with_ignore_filenames` / `with_global_ignore_file` feed the walker (custom names prune during traversal; the global file compiles per root inside the walk's blocking hop), and that `watcher_globs` now includes ignore-file names + built-ins, whose events invalidate the walk cache. `CLAUDE.md` "Matching & workspace scanning": one sentence for the two knobs. Also record in `structure.md`'s dispatch paragraph: the dispatch gate (unadvertised → `-32601`).
- [ ] **Step 3: README check** — if `README.md` documents `ServerOptions` surface, add the two knobs with a short example; otherwise skip (do not invent a section).
- [ ] **Step 4: Final battery + dupes + dylint; ledger update.** Report. Suggested commit: `Document the ignore configuration and the dispatch gate`.

---

## Pre-Flight Plan Review (controller notes)

- Task 5 is intentionally the largest and lands the gate WITH all fixture updates in one commit — a green battery is per-task mandatory; landing the gate without fixtures would redden every dispatch test.
- Task 3 records one spec deviation (global-matcher stamp caching dropped) — flagged for the owner at review, per no-silent-deviation.
- Tasks 2 and 5 both touch `macros/src/dispatch.rs` (Task 2: prime + spawn_blocking wraps; Task 5: gate prepend) — sequential execution, no conflict, but Task 5's implementer must see Task 2's landed code.
- `add_custom_ignore_filename` behavior under `standard_filters(false)` (Task 3) is flagged for verification against the local ignore source before the design is considered final — the walker golden tests are the oracle.
