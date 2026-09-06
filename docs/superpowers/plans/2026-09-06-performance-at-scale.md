# Performance at scale — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the crate fast at 10k+ file workspaces with all heavy lifting under the hood: core-derived concurrency defaults, bounded-parallel batch diagnostics, incremental refresh, an O(1)-merge report path, a single-snapshot dispatch engine, allocation-free parsing, and a criterion bench harness.

**Architecture:** One bounded fan-out primitive (`for_each_bounded`, futures `buffer_unordered`, width = `available_parallelism()` narrowable via `ServerOptions::with_diagnostics_parallelism`) drives refresh loads and both diagnostics surfaces; a conservative mtime+size gate skips unchanged files; the dispatch engine computes one conversion document per request with clone-free version accessors; `LanguageServerWithState` becomes `Clone` so oneshot fans out through the same wrapper the wire server uses. `CONTENT_MODIFIED` semantics and every wire-observable behavior except the request limit are unchanged.

**Tech Stack:** Rust edition 2024, async-lsp 0.2.4 (`ConcurrencyLayer::default`), futures `buffer_unordered`, tokio `spawn_blocking` (dev-side runtime features), tree-sitter `parse_with`, criterion (dev-dep).

**Spec:** `docs/superpowers/specs/2026-09-06-performance-at-scale-design.md` · **Research:** `docs/superpowers/research/2026-09-06-performance-hotspots-research.md`

## Global Constraints

- **No git commands, no commits, no branches** — the owner does all git work. Every task ends with a *checkpoint* (file group), never a commit.
- All written artifacts in **English only**. No `use … as Name` aliases (`as _` is fine). Imports at use sites.
- **No `#[allow]` to pass a check** — investigate root causes. Lint levels are deny: clippy `all`/`cargo`/`pedantic`, `expect_used`, `unwrap_used` (tests exempt via `clippy.toml`; **benches are NOT test-cfg — write bench code without `unwrap`/`expect`**), `missing_docs`.
- `CONTENT_MODIFIED` semantics unchanged: a version change on any open document mid-`workspace/diagnostic` still fails the whole request with that code. Parallelism changes throughput, not protocol behavior.
- Notification handlers stay synchronous (`std::fs` there is deliberate; do not "fix" it).
- The full battery must pass in all three feature configurations after every task; tree-sitter-gated code must also compile under `--no-default-features`.
- Piped output: check `${pipestatus[1]}` (zsh).
- **Task order is fixed 1→12** unless a step says otherwise; T8 depends on T5+T6+T7, T9 on T6, T10 on T8+T9.

**Baseline test counts** (from the merged stdio cycle; verify before T1):

| configuration | passed |
|---|---|
| default | 245 (210 lib + 1 arch + 22 macros + 12 doctests) |
| `--no-default-features` | 211 |
| `--all-features` | 245 |

## Amendment (execution-time, 2026-09-06)

Tasks 5 and 6 merged into Task 8. Reason discovered at T5: the knob's
`ServerState` field/accessor and the `for_each_bounded` engine are
`pub(crate)` items whose first **non-test** consumers are T8/T9 — rustc's
`dead_code` fires on them in the plain-lib compilation pass of
`clippy --all-targets` at T5/T6, and every compliant escape (`#[allow]`,
public widening, artificial consumers) is forbidden by the constraints.
T8's implementer receives the T5 + T6 + T8 briefs together and lands the
knob, the engine, and their first consumers as one green unit. T5's
initial attempt was reverted cleanly (report in
`.superpowers/sdd/task-5-report.md`).

## File Structure

| file | responsibility | task |
|---|---|---|
| `src/server/serve.rs` | core-derived request limit | 1 |
| `src/server/tests/robustness.rs` | dynamic-limit tripwire | 1 |
| `src/workspace/diagnostics.rs` | indexed merge; parallel diagnostics loop | 2, 8 |
| `src/server/state/mod.rs` | version/count accessors, sole doc, parallelism field, entry stamp | 3, 5, 7 |
| `src/server/with_state/mod.rs` | conversion_document reuse, sole-document | 3 |
| `macros/src/dispatch.rs` | single-snapshot engine emission + mirror tests | 3 |
| `src/server/state/documents.rs` | rope-reader parse sites | 4 |
| `src/server/options.rs` | `with_diagnostics_parallelism` | 5 |
| `src/workspace/parallel.rs` (new) | `for_each_bounded` + tests | 6 |
| `src/documents/matcher.rs`, `src/server/state/workspace.rs` | path-first matching, mtime gate | 7 |
| `src/server/state/documents.rs` (insert stamp), `src/workspace/diagnostics.rs` | parallel refresh + loop | 8 |
| `src/oneshot/server.rs`, `src/oneshot/workspace_diagnostics.rs` | Clone wrapper, streaming parallel | 9 |
| `benches/oneshot_diagnostics.rs` (new), `Cargo.toml` | criterion bench | 10 |
| `README.md`, `CLAUDE.md`, `.claude/rules/{structure,testing,tech}.md` | docs sync | 11 |

---

### Task 1: Core-derived serve limit + dynamic tripwire

**Files:**
- Modify: `src/server/serve.rs:15-23,83-89` — the constant and the layer
- Modify: `src/server/tests/robustness.rs:97-147` — tripwire test
- No other file.

**Interfaces:**
- Produces: none consumed later (the tripwire's limit is `available_parallelism()` at runtime).

- [ ] **Step 1: Swap the layer and update the doc bullet**

In `src/server/serve.rs`, delete:

```rust
const MAX_CONCURRENT_REQUESTS: NonZeroUsize = match NonZeroUsize::new(8) {
    Some(value) => value,
    None => unreachable!(),
};
```

replace the layer line:

```rust
            .layer(ConcurrencyLayer::new(MAX_CONCURRENT_REQUESTS))
```

with:

```rust
            .layer(ConcurrencyLayer::default())
```

and replace the doc bullet `- Maximum concurrency of 8 in-flight LSP requests at a time` with `- In-flight LSP requests bounded by the CPU core count`. Remove `use std::num::NonZeroUsize;` if nothing else uses it (nothing does after the deletion).

- [ ] **Step 2: Make the tripwire limit-dynamic**

In `src/server/tests/robustness.rs`, rename `at_most_eight_requests_run_concurrently` to `at_most_limit_requests_run_concurrently` and make the counts derive from the layer's default (`available_parallelism`, fallback 1):

```rust
    let limit = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    // One more request than the layer admits.
    for id in 0..=(limit as i64) {
        client
            .send_request(
                id,
                "textDocument/hover",
                hover_params("file:///tmp/wire.txt", 0),
            )
            .await;
    }

    for _ in 0..limit {
        timeout(WIRE_TIMEOUT, entered_rx.recv())
            .await
            .expect("handler entered")
            .expect("signal received");
    }
```

Keep the two absence-checks (`the ninth handler must wait for a permit` → reword to `the overflow handler must wait for a permit` and `did upstream PR #30 land?`) and the final `server.abort();` exactly as they are — the tripwire semantics are unchanged, only the arithmetic is dynamic. Cargo.toml's dev tokio already enables `time`/`sync` needed here.

- [ ] **Step 3: Verify**

Run: `cargo test --workspace at_most_limit`
Expected: 1 passed, 0 failed.
Run: `cargo test --workspace`
Expected: 245 passed, 0 failed (renamed test, none added).
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 4: Checkpoint** — report `src/server/serve.rs` + `src/server/tests/robustness.rs`. No git.

---

### Task 2: Indexed report merge (kill O(N²))

**Files:**
- Modify: `src/workspace/diagnostics.rs:456-511` — the three `push_*` helpers and their call sites
- Test: same file, inline `#[cfg(test)] mod tests` (add one unit test)

**Interfaces:**
- Produces: `push_workspace_reports_from_document_result(state, uri, result, reports: &mut Vec<…>, index: &mut HashMap<Url, usize>)` — T8's parallel loop calls it with the same two out-params.

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` of `src/workspace/diagnostics.rs` add:

```rust
    #[test]
    fn push_workspace_report_replaces_by_uri_and_appends_new() {
        use super::{WorkspaceReportSink, push_workspace_report};

        let mut sink = WorkspaceReportSink::default();
        let uri = crate::testing::url("file:///tmp/a.txt");
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Unchanged(
                WorkspaceUnchangedDocumentDiagnosticReport {
                    version: None,
                    uri: uri.clone(),
                    unchanged_document_diagnostic_report: Default::default(),
                },
            ),
            false,
        );
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(
                WorkspaceFullDocumentDiagnosticReport {
                    version: None,
                    uri: uri.clone(),
                    full_document_diagnostic_report: Default::default(),
                },
            ),
            true,
        );
        let other = crate::testing::url("file:///tmp/b.txt");
        push_workspace_report(
            &mut sink,
            WorkspaceDocumentDiagnosticReport::Full(
                WorkspaceFullDocumentDiagnosticReport {
                    version: None,
                    uri: other,
                    full_document_diagnostic_report: Default::default(),
                },
            ),
            false,
        );
        // replace=true overwrote the first URI; the new URI appended;
        // the `replace=false` pass over `other` left one entry per URI.
        assert_eq!(sink.reports.len(), 2);
    }
```

If `WorkspaceUnchangedDocumentDiagnosticReport`'s inner field lacks `Default`, construct `UnchangedDocumentDiagnosticReport { result_id: None, kind: Default::default() }` to match what compiles — mirror the existing test module's constructions in this file.

- [ ] **Step 2: Run it — expect a compile failure** (`WorkspaceReportSink` undefined).

- [ ] **Step 3: Implement the sink**

Replace `push_workspace_report` and add the sink:

```rust
/// Ordered report accumulator with an O(1) URI index: pushes replace or
/// append by URI in constant time; the final `Vec` order is the insertion
/// order (the caller's final sort normalizes output).
#[derive(Default)]
struct WorkspaceReportSink {
    reports: Vec<WorkspaceDocumentDiagnosticReport>,
    index: HashMap<Url, usize>,
}

fn push_workspace_report(
    sink: &mut WorkspaceReportSink,
    report: WorkspaceDocumentDiagnosticReport,
    replace: bool,
) {
    let uri = workspace_report_uri(&report).clone();
    match sink.index.get(&uri) {
        Some(&position) => {
            if replace {
                sink.reports[position] = report;
            }
        }
        None => {
            sink.index.insert(uri, sink.reports.len());
            sink.reports.push(report);
        }
    }
}
```

Thread the sink through `push_workspace_reports_from_document_result` and `push_related_reports` (same file): every `reports: &mut Vec<WorkspaceDocumentDiagnosticReport>` parameter becomes `sink: &mut WorkspaceReportSink`, and the call site in `workspace_diagnostic_items` builds `let mut sink = WorkspaceReportSink::default();` … `Ok(sink.reports)` after the existing sort.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace push_workspace_report`
Expected: 1 passed.
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: 246 passed, 0 failed; exit 0.

- [ ] **Step 5: Checkpoint** — report `src/workspace/diagnostics.rs`. No git.

---

### Task 3: Single-snapshot dispatch engine + clone-free accessors

**Files:**
- Modify: `src/server/state/mod.rs:73-88` — add accessors
- Modify: `src/server/with_state/mod.rs:47-53` — `conversion_document` sole-document path
- Modify: `macros/src/dispatch.rs:113-227` — both engines' emission; tests `:229-301`
- No behavior change on the wire beyond clone/read counts.

**Interfaces:**
- Produces (consumed by the macro emission): `ServerState::document_version(&self, url: &Url) -> Option<i32>`; `ServerState::sole_document(&self) -> Option<Document>`.

- [ ] **Step 1: Add the accessors**

In `src/server/state/mod.rs`, after `documents()`:

```rust
    /// Returns the version of the tracked document at `url`, if tracked.
    ///
    /// A clone-free probe: unlike [`ServerState::document`], it does not
    /// snapshot the document.
    pub(crate) fn document_version(&self, url: &Url) -> Option<i32> {
        self.documents.get(url).map(|entry| entry.document.version())
    }

    /// Returns the sole tracked document when exactly one is tracked.
    ///
    /// The resolve-family heuristic: with zero or several tracked
    /// documents there is no sole document to convert against.
    pub(crate) fn sole_document(&self) -> Option<Document> {
        let mut entries = self.documents.iter();
        let first = entries.next()?.document.clone();
        entries.next().is_none().then_some(first)
    }
```

In `src/server/with_state/mod.rs`, replace the URL-less branch of `conversion_document`:

```rust
    let Some(url) = url else {
        return state.sole_document();
    };
```

- [ ] **Step 2: Rewire the URL-anchored engine**

In `macros/src/dispatch.rs` `engine()`, the non-resolve core becomes (replacing steps 2–6; step 1 and the handler call keep their shapes):

```rust
            // 2. Version probe (clone-free) and one conversion document
            //    for the whole request.
            let ver: Option<i32> =
                url.as_ref().and_then(|url| state.document_version(url));
            let params_doc = conversion_document(&state, url.as_ref());
            if let Some(doc) = params_doc.as_ref() {
                <#request as crate::lsp_requests::Request>::modify_params(&state, doc, &mut params,);
            }

            // 3. Call the user-defined language server function.
            let mut result = server.#trait_method(state.clone(), params).await?;

            // 4. Staleness probe against the same clone-free version.
            if let Some(url) = url.as_ref()
                && state.document_version(url).is_some_and(|v| Some(v) != ver)
            {
                return Err(ResponseError::new(
                    ErrorCode::CONTENT_MODIFIED,
                    "document was modified during processing",
                ));
            }

            // 5. The staleness probe passed, so the conversion document is
            //    still valid for the response — reuse it instead of
            //    re-resolving (one snapshot and at most one disk read per
            //    request).
            match params_doc.as_ref() {
                Some(doc) => {
                    <#request as crate::lsp_requests::Request>::modify_response(&state, doc, &mut result,);
                }
                None => {
                    <#request as crate::lsp_requests::Request>::modify_response_standalone(
                        &state, &mut result,
                    );
                }
            }

            Ok(result)
```

The resolve core's `conversion_document(&state, None)` becomes `state.sole_document()`:

```rust
            let sole = state.sole_document();
            match sole.as_ref() {
```

(the two existing `match sole.as_ref()` arms are unchanged).

- [ ] **Step 3: Update the macro mirror tests**

In `macros/src/dispatch.rs` tests: `engine_emits_url_anchored_skeleton`'s needle list — replace `"conversion_document"` (now expected once, not twice) and add `"document_version"`; the emission must NOT contain a second `conversion_document` in the response step — assert `text.matches("conversion_document").count() == 1`. `engine_emits_sole_document_path_for_resolve_rows` gains a `"sole_document"` needle.

- [ ] **Step 4: Verify**

Run: `cargo test -p lsp_macros --lib`
Expected: 22 passed (updated tests).
Run: `cargo test --workspace && cargo test --workspace --no-default-features`
Expected: 246 / 212 passed, 0 failed — the wire staleness and UTF-16 round-trip tests must pass **unmodified** (behavior unchanged).
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 5: Checkpoint** — report `src/server/state/mod.rs`, `src/server/with_state/mod.rs`, `macros/src/dispatch.rs`. No git.

---

### Task 4: Rope-reader parse sites (no whole-file Strings)

**Files:**
- Modify: `src/server/state/documents.rs` (tree-sitter-gated parse sites: insert `:42-52`, incremental `:197-209`, full-replace `:143-152`, didSave `:296-314`, failed-incremental reload `:229-262`)
- Feature-gated: verify with `--no-default-features` too.

**Interfaces:**
- Produces: `fn parse_rope(parser: &mut Parser, rope: &Rope, old_tree: Option<&Tree>) -> Option<Tree>` (local, `#[cfg(feature = "tree-sitter")]`) used by every parse site in this file.

- [ ] **Step 1: Add the adapter**

At the bottom of `src/server/state/documents.rs` (above the test module), under the existing tree-sitter imports:

```rust
/// Parses a rope's text through tree-sitter's chunked-input callback,
/// avoiding the whole-document `String` that `text_contents()` would
/// allocate. The callback serves the chunk containing the requested byte
/// offset; tree-sitter drives it sequentially and may seek within edited
/// ranges when an old tree is supplied.
#[cfg(feature = "tree-sitter")]
fn parse_rope(
    parser: &mut Parser,
    rope: &Rope,
    old_tree: Option<&tree_sitter::Tree>,
) -> Option<tree_sitter::Tree> {
    parser.parse_with(
        &mut |byte_offset: usize, _point: tree_sitter::Point| -> &str {
            let end = rope.len_bytes();
            let offset = byte_offset.min(end);
            if offset == end {
                return "";
            }
            let (chunk, chunk_start, _) = rope.byte_to_chunk(offset);
            &chunk[offset - chunk_start..]
        },
        old_tree,
    )
}
```

If ropey 1.6's `byte_to_chunk` return shape differs from `(&str, usize, usize)` in your tree, adapt to whatever returns the containing chunk plus its start offset — the doc comment on `parse_rope` stays true either way.

- [ ] **Step 2: Replace the parse sites**

At every site the research listed, replace `parser.parse(&text, None)` / `parser.parse(doc.text_contents(), Some(tree))`-shaped calls with `parse_rope(&mut parser, rope, old)`:

- `insert_document`: build the rope first (`let text_rope = Rope::from(text);` — note `text` is still needed for the language lookup path only if it reads the string; it does not), parse against `&text_rope`, then move the rope into the `Document`.
- incremental re-parse: `parse_rope(&mut parser, doc.text(), Some(tree))` — deletes the `text_contents()` materialization per didChange batch.
- full-replace and didSave: same shape against the new rope.
- failed-incremental reload: after re-reading the text from disk, same shape.

- [ ] **Step 3: Verify**

Run: `cargo test --workspace && cargo test --workspace --no-default-features && cargo test --workspace --all-features`
Expected: 246 / 212 / 246 passed, 0 failed — tree-sitter document tests (open/change/save/reparse) pass unmodified; the parse results are unchanged by construction.
Run: `grep -n "text_contents()" src/server/state/documents.rs`
Expected: no matches (all parse-path uses gone; other files' uses are out of this task's scope).
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 4: Checkpoint** — report `src/server/state/documents.rs`. No git.

---

### Task 5: The parallelism knob

**Files:**
- Modify: `src/server/options.rs:3-28`
- Modify: `src/server/state/mod.rs` (field + accessor)
- Test: `src/server/options.rs` inline tests.

**Interfaces:**
- Produces: `ServerOptions::with_diagnostics_parallelism(self, width: NonZeroUsize) -> Self` (public); `ServerOptions::diagnostics_parallelism(&self) -> usize` and `ServerState::diagnostics_parallelism(&self) -> usize` (crate) — T8 and T9 consume them.

- [ ] **Step 1: Write the failing tests**

In `options.rs` tests:

```rust
    #[test]
    fn diagnostics_parallelism_defaults_to_cores_and_is_narrowable() {
        use std::num::NonZeroUsize;

        let cores = std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1);
        assert_eq!(ServerOptions::default().diagnostics_parallelism(), cores);

        let narrowed = ServerOptions::default()
            .with_diagnostics_parallelism(NonZeroUsize::new(2)?);
        assert_eq!(narrowed.diagnostics_parallelism(), 2);
        assert_eq!(
            narrowed
                .with_diagnostics_parallelism(NonZeroUsize::new(1)?)
                .diagnostics_parallelism(),
            1
        );
    }
```

(`?` is fine in a `-> Option<()>` test; if the module's tests return `()`, use `.expect("constant is nonzero")` — tests may unwrap per `clippy.toml`.)

- [ ] **Step 2: Run — expect compile failure** (no such method).

- [ ] **Step 3: Implement**

```rust
use std::num::NonZeroUsize;

pub struct ServerOptions {
    pub(crate) workspace_diagnostics: WorkspaceDiagnostics,
    pub(crate) diagnostics_parallelism: Option<NonZeroUsize>,
}

impl ServerOptions {
    /// Narrows how many documents the batch diagnostics pipeline works on
    /// at once. Defaults to the machine's CPU core count.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    ///
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_diagnostics_parallelism(NonZeroUsize::new(2).expect("nonzero"));
    /// ```
    #[must_use]
    pub fn with_diagnostics_parallelism(mut self, width: NonZeroUsize) -> Self {
        self.diagnostics_parallelism = Some(width);
        self
    }

    pub(crate) fn diagnostics_parallelism(&self) -> usize {
        self.diagnostics_parallelism
            .map_or_else(default_parallelism, NonZeroUsize::get)
    }
}

/// The crate-wide default width: every CPU core, at least one.
fn default_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(1)
}
```

In `ServerState` (`state/mod.rs`): add field `diagnostics_parallelism: usize`, set it in `with_options` from `options.diagnostics_parallelism()`, and add:

```rust
    /// How many documents the batch diagnostics pipeline may work on at
    /// once (`ServerOptions::with_diagnostics_parallelism`, defaulting to
    /// the CPU core count).
    pub(crate) fn diagnostics_parallelism(&self) -> usize {
        self.diagnostics_parallelism
    }
```

- [ ] **Step 4: Verify**

Run: `cargo test --workspace diagnostics_parallelism`
Expected: 1 passed.
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: 247 passed; exit 0.

- [ ] **Step 5: Checkpoint** — report `src/server/options.rs`, `src/server/state/mod.rs`. No git.

---

### Task 6: The bounded fan-out engine

**Files:**
- Create: `src/workspace/parallel.rs` (+ `pub(crate) mod parallel;` in `src/workspace/mod.rs`)
- Test: inline in the new file.

**Interfaces:**
- Produces: `pub(crate) async fn for_each_bounded<T, R, E, F, Fut>(items: Vec<T>, width: usize, f: F) -> Result<Vec<R>, E> where F: Fn(T) -> Fut, Fut: Future<Output = Result<R, E>>` — T8 and T9 consume it.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::{Semaphore, mpsc};

    use super::for_each_bounded;

    async fn ok(value: u32) -> Result<u32, &'static str> {
        tokio::task::yield_now().await;
        Ok(value)
    }

    #[tokio::test]
    async fn results_return_in_input_order_regardless_of_completion() {
        // Later items finish first; order must still follow input.
        let items = vec![0u32, 1, 2, 3];
        let results = for_each_bounded(items, 4, |item| async move {
            for _ in 0..(4 - item) {
                tokio::task::yield_now().await;
            }
            ok(item)
        })
        .await;
        assert_eq!(results, Ok(vec![0, 1, 2, 3]));
    }

    #[tokio::test]
    async fn width_bounds_concurrent_items() {
        let permits = Semaphore::new(0);
        let mut started = Vec::new();
        let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
        let items = vec!['a', 'b', 'c'];
        let handle = tokio::spawn(for_each_bounded(items, 2, |item| {
            let entered_tx = entered_tx.clone();
            async move {
                entered_tx.send(item).expect("channel open");
                // Block until the test releases the cohort: item 'a' and
                // 'b' enter, 'c' must not while both are parked.
                permits.acquire().await.expect("semaphore open").forget();
                Ok(item)
            }
        }));
        entered_rx.recv().await;
        entered_rx.recv().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(250), entered_rx.recv())
                .await
                .is_err(),
            "the third item must wait for a width slot"
        );
        permits.add_permits(1);
        permits.add_permits(10);
        let results = handle.await.expect("task joins");
        assert_eq!(results, Ok(vec!['a', 'b', 'c']));
        drop(started);
    }
}
```

- [ ] **Step 2: Run — expect compile failure** (`for_each_bounded` undefined).

- [ ] **Step 3: Implement**

```rust
//! One owner of batch parallelism: runs a fallible per-item future over
//! `items` with at most `width` in flight, returning results in input
//! order. Errors keep the caller's first-error-in-input-order semantics:
//! every item runs to completion, then the first `Err` (by input order)
//! propagates — for `workspace/diagnostic` that reproduces the serial
//! loop's all-or-nothing outcome, including `CONTENT_MODIFIED`.

use futures::stream::{StreamExt as _, iter};

pub(crate) async fn for_each_bounded<T, R, E, F, Fut>(
    items: Vec<T>,
    width: usize,
    f: F,
) -> Result<Vec<R>, E>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = Result<R, E>>,
{
    let mut indexed: Vec<(usize, Result<R, E>)> = iter(items.into_iter().enumerate())
        .map(|(index, item)| {
            let future = f(item);
            async move { (index, future.await) }
        })
        .buffer_unordered(width.max(1))
        .collect()
        .await;
    indexed.sort_unstable_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}
```

Register the module in `src/workspace/mod.rs` (`pub(crate) mod parallel;`) in the existing module list's alphabetical slot.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace for_each_bounded`
Expected: 2 passed.
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: 249 passed; exit 0.

- [ ] **Step 5: Checkpoint** — report `src/workspace/parallel.rs`, `src/workspace/mod.rs`. No git.

---

### Task 7: Incremental refresh (mtime+size gate, path-first matching)

**Files:**
- Modify: `src/documents/matcher.rs:168-175`
- Modify: `src/server/state/mod.rs:34-38` (`DocumentEntry`)
- Modify: `src/server/state/workspace.rs:79-127`
- Test: matcher inline tests + workspace.rs inline tests (or sibling `tests.rs` if the module uses one — follow the existing layout).

**Interfaces:**
- Produces: `DocumentMatchers::find_path(&self, path: &Path) -> Option<Arc<DocumentMatcher>>`; `DocumentEntry.stamp: Option<FileStamp>` where `type FileStamp = (SystemTime, u64)` — T8 consumes both.

- [ ] **Step 1: Path-first matching**

In `matcher.rs`, refactor:

```rust
    pub(crate) fn find_url(&self, url: &Url) -> Option<Arc<DocumentMatcher>> {
        url.to_file_path().ok().and_then(|p| self.find_path(&p))
    }

    pub(crate) fn find_path(&self, path: &Path) -> Option<Arc<DocumentMatcher>> {
        self.globsets
            .iter()
            .find(|(globset, _)| globset.is_match(path))
            .map(|(_, matcher)| Arc::clone(matcher))
    }
```

(`find`'s `find_url` delegation is unchanged.) Add `use std::path::Path;` at the top. Unit test (mirror the existing find_url tests' style): a glob matching `**/*.demo` finds a `Path::new("/tmp/x/demo.demo")` and rejects `Path::new("/tmp/x/demo.txt")`.

- [ ] **Step 2: The gate helper + failing tests**

In `state/mod.rs`:

```rust
/// Filesystem stamp used to skip re-reading unchanged workspace files:
/// (modification time, size in bytes). Any doubt re-reads.
pub(crate) type FileStamp = (std::time::SystemTime, u64);
```

`DocumentEntry` gains `stamp: Option<FileStamp>` (all existing constructors set `stamp: None` — only refresh sets it). In `state/workspace.rs`:

```rust
fn file_stamp(path: &std::path::Path) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// Conservative gate: only an exact stamp match on an already-tracked
/// Workspace-origin document skips the re-read; missing stamps or any
/// difference re-reads.
fn stamp_unchanged(entry_stamp: Option<FileStamp>, disk_stamp: Option<FileStamp>) -> bool {
    matches!((entry_stamp, disk_stamp), (Some(a), Some(b)) if a == b)
}
```

Tests (inline where the module's tests live):

```rust
    #[test]
    fn stamp_gate_is_conservative() {
        use std::time::{Duration, SystemTime};

        let now = SystemTime::now();
        let stamp = (now, 12);
        assert!(stamp_unchanged(Some(stamp), Some(stamp)));
        assert!(!stamp_unchanged(
            Some(stamp),
            Some((now + Duration::from_secs(1), 12))
        ));
        assert!(!stamp_unchanged(Some(stamp), Some((now, 13))));
        assert!(!stamp_unchanged(None, Some(stamp)));
        assert!(!stamp_unchanged(Some(stamp), None));
        assert!(!stamp_unchanged(None, None));
    }
```

Plus an integration test through `refresh_workspace_documents` (temp workspace via `crate::testing::temp_workspace`, matching matcher): first refresh inserts; rewrite one file with different-length content and `refresh` again — the changed file's `state.document(&url)` text reflects the new content and the untouched file remains tracked (the same assertions the existing refresh tests use; follow their fixture style).

- [ ] **Step 3: Wire the gate into the refresh loop**

Replace the loop body in `refresh_workspace_documents`:

```rust
        for path in walker.files()? {
            let Some(matcher) = self.matchers.find_path(&path) else {
                continue;
            };
            let uri = path_to_url(&path)?;
            urls.push(uri.clone());
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                continue;
            }
            let stamp = file_stamp(&path);
            if let Some(entry) = self.documents.get(&uri)
                && entry.origin == DocumentOrigin::Workspace
                && stamp_unchanged(entry.stamp, stamp)
            {
                continue;
            }
            drop(entry-guard per the borrow checker: use `is_some_and` on a fresh `get` if the let-chain borrow conflicts);

            let language = matcher
                .lang_strings()
                .first()
                .cloned()
                .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
            // arch-lint: allow(no-sync-io) reason="workspace scanning is a synchronous batch pass over the ignore crate by design"
            let text = std::fs::read_to_string(&path)?;
            self.insert_document(uri.clone(), text, 0, language, DocumentOrigin::Workspace);
            if let Some(mut entry) = self.documents.get_mut(&uri) {
                entry.stamp = stamp;
            }
        }
```

The `let Some(entry) = … && …` let-chain holds a `DashMap` read guard across the `continue` — allowed, but restructure to `if self.documents.get(&uri).is_some_and(|e| e.origin == DocumentOrigin::Workspace && stamp_unchanged(e.stamp, stamp)) { continue; }` if the borrow outlives the second `get_mut`; the semantics are: skip when tracked-as-Workspace and stamps match exactly.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace stamp && cargo test --workspace find_path`
Expected: new tests pass.
Run: `cargo test --workspace && cargo test --workspace --no-default-features`
Expected: 252 / 218 passed, 0 failed (existing refresh/watched-files tests unaffected — unstamped docs always re-read).
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 5: Checkpoint** — report `src/documents/matcher.rs`, `src/server/state/mod.rs`, `src/server/state/workspace.rs`. No git.

---

### Task 8: Parallel refresh + parallel workspace diagnostics

**Files:**
- Modify: `src/server/state/workspace.rs:79-127` — async refresh, bounded-parallel loads
- Modify: `src/workspace/diagnostics.rs:380-440` — parallel diagnostics loop
- Test: `src/workspace/diagnostics.rs` tests + a structural parallel test.

**Interfaces:**
- Consumes: `for_each_bounded` (T6), `state.diagnostics_parallelism()` (T5), the gate (T7).
- Produces: `ServerState::refresh_workspace_documents(&self) -> impl Future<Output = ServerResult<Vec<Url>>>` (async now — one caller, updated here).

- [ ] **Step 1: Async parallel refresh**

`refresh_workspace_documents` becomes `async fn`. The per-file work (gate + read + insert + stamp) moves into `tokio::task::spawn_blocking` per changed file, fanned out through the engine:

```rust
    pub(crate) async fn refresh_workspace_documents(&self) -> ServerResult<Vec<Url>> {
        if !self.workspace_diagnostics.enabled() {
            return Ok(self.document_urls());
        }
        let roots = self.workspace_roots();
        if roots.is_empty() {
            return Ok(self.document_urls());
        }
        let walker = WorkspaceWalker::new(&roots, WorkspaceWalkConfig::default())?;
        let mut urls = Vec::new();
        let mut loads = Vec::new();

        for path in walker.files()? {
            let Some(matcher) = self.matchers.find_path(&path) else {
                continue;
            };
            let uri = path_to_url(&path)?;
            urls.push(uri.clone());
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                continue;
            }
            loads.push((path, uri, matcher));
        }

        let state = self.clone();
        let width = state.diagnostics_parallelism();
        for_each_bounded(loads, width, move |(path, uri, matcher)| {
            let state = state.clone();
            async move {
                let gate = tokio::task::spawn_blocking({
                    let path = path.clone();
                    move || file_stamp(&path)
                })
                .await
                .ok()
                .flatten();
                if state
                    .documents
                    .get(&uri)
                    .is_some_and(|entry| entry.origin == DocumentOrigin::Workspace
                        && stamp_unchanged(entry.stamp, gate))
                {
                    return Ok(());
                }
                let language = matcher
                    .lang_strings()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
                let text = tokio::task::spawn_blocking({
                    let path = path.clone();
                    move || std::fs::read_to_string(&path)
                })
                .await
                .map_err(std::io::Error::from)??;
                state.insert_document(
                    uri.clone(),
                    text,
                    0,
                    language,
                    DocumentOrigin::Workspace,
                );
                if let Some(mut entry) = state.documents.get_mut(&uri) {
                    entry.stamp = gate;
                }
                Ok(())
            }
        })
        .await?;

        let urls: HashSet<_> = urls.into_iter().collect();
        self.documents.retain(|url, entry| {
            entry.origin == DocumentOrigin::Open
                || !url_is_in_roots(url, &roots)
                || urls.contains(url)
        });

        let mut urls: Vec<_> = urls.into_iter().collect();
        urls.sort();
        Ok(urls)
    }
```

Notes: `spawn_blocking` moves the file read AND the eager tree-sitter parse (inside `insert_document`) off the executor — the async-spawn-blocking rule's bounded admission is the engine width. `file_stamp` probing rides the same `spawn_blocking` (metadata is IO). The sync `std::fs` arch-lint allow comment moves onto the inner closures' reads. Its single caller (`workspace_diagnostic_items`) adds `.await`:

```rust
    let urls = state
        .refresh_workspace_documents()
        .await
        .map_err(ResponseError::from)?;
```

- [ ] **Step 2: Parallel diagnostics loop**

Replace the serial `for url in urls` loop body with the engine (per-item work exactly as today — snapshot, handler, staleness probe, response conversion, sink push — moved into the closure):

```rust
    let width = state.diagnostics_parallelism();
    let mut sink = WorkspaceReportSink::default();
    let state_for_items = state.clone();
    for_each_bounded(urls, width, move |url| {
        let server = Arc::clone(&server);
        let state = state_for_items.clone();
        let identifier = identifier.clone();
        let previous = previous_result_ids.get(&url).cloned();
        async move {
            let Some(doc) = state.document(&url) else {
                return Ok(());
            };
            let version = doc.version();
            let mut result = server
                .document_diagnostics(
                    state.clone(),
                    document_diagnostic_params(url.clone(), identifier, previous),
                )
                .await
                .map_err(ResponseError::from)?;
            if state.document_version(&url).is_some_and(|v| v != version) {
                return Err(ResponseError::new(
                    ErrorCode::CONTENT_MODIFIED,
                    "document was modified during processing",
                ));
            }
            <crate::lsp_requests::DocumentDiagnosticsRequest as Request>::modify_response(
                &state, &doc, &mut result,
            );
            let mut sink = WorkspaceReportSink::default();
            push_workspace_reports_from_document_result(&state, url, result, &mut sink);
            Ok(sink)
        }
    })
    .await?
    .into_iter()
    .for_each(|item_sink| {
        for report in item_sink.reports {
            let uri = workspace_report_uri(&report).clone();
            let replace = !matches!(
                report,
                WorkspaceDocumentDiagnosticReport::Unchanged(_)
            );
            push_workspace_report(&mut sink, report, replace);
        }
        let _ = uri; // see note
    });
```

IMPORTANT — sink merge semantics: the per-item closure returns its own `WorkspaceReportSink`; the outer merge folds item sinks in **result order** (the engine already restores input order) using `push_workspace_report` with the same `replace` flags the current code passes (`true` for the document's own report, `false`-for-related comes from `push_related_reports` internals — keep those flags EXACTLY as the current code computes them; if `push_workspace_reports_from_document_result` currently pushes related reports with `replace=false`, the fold must preserve that, so fold at the report level with the flags baked in — simplest correct fold: make the closure return `Vec<(WorkspaceDocumentDiagnosticReport, bool)>` pairs and fold with `push_workspace_report(&mut sink, report, replace)`). Adjust the sketch above to that pair-based fold; do not lose a flag.

The final `items.sort_by(...)` stays.

- [ ] **Step 3: Structural parallel test**

In `src/workspace/diagnostics.rs` tests, add a gated-server test mirroring the robustness pattern: a `Server` whose `document_diagnostics` signals entry and parks on a `tokio::sync::Barrier`/watch; workspace with 3 files, `with_diagnostics_parallelism(3)` — all three handlers enter before any releases (deterministic via counting channel + bounded timeout); then release and assert the report has 3 documents. And the width-1 counterpart: third entry only after first release.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace`
Expected: 254 passed, 0 failed (2 new structural tests; existing workspace-diagnostics wire/unit tests unmodified and green).
Run: `cargo test --workspace --no-default-features && cargo test --workspace --all-features`
Expected: 220 / 254 passed.
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 5: Checkpoint** — report `src/server/state/workspace.rs`, `src/workspace/diagnostics.rs`. No git.

---

### Task 9: Oneshot streaming + parallel

**Files:**
- Modify: `src/server/with_state/mod.rs` — `#[derive(Clone)]` on `LanguageServerWithState`
- Modify: `src/oneshot/server.rs` — expose a clone-based parallel driver
- Modify: `src/oneshot/workspace_diagnostics.rs:210-286` — streaming rewrite
- Test: existing oneshot tests + one structural parallel test.

**Interfaces:**
- Consumes: `for_each_bounded` (T6), `S::server_options().diagnostics_parallelism()` (T5).
- Produces: unchanged public signature `pub async fn workspace_diagnostics<S>(server: S, config: WorkspaceDiagnosticConfig) -> ServerResult<WorkspaceDiagnosticReport>`.

- [ ] **Step 1: Clone the wrapper**

On `LanguageServerWithState` (all fields are `Arc<T>`/`ServerState`-style cheap handles — verify each field is `Clone`; if any is not, make it so or note the blocker): add `#[derive(Clone)]`. Clones share the interior-mutable document store; each clone's `&mut self` methods run concurrently without interference (DashMap-backed).

- [ ] **Step 2: Streaming rewrite**

Replace `workspace_diagnostics`' discovery+open+diagnose block (no more all-texts `Vec<WorkspaceDocument>`; `discover_documents` dies, `workspace_document`'s read moves into the per-item task):

```rust
pub async fn workspace_diagnostics<S>(
    server: S,
    config: WorkspaceDiagnosticConfig,
) -> ServerResult<WorkspaceDiagnosticReport>
where
    S: Server + Send + Sync + 'static,
{
    let walker = WorkspaceWalker::new(&config.roots, config.walk)?;
    let matchers = DocumentMatchers::new(S::server_document_matchers());
    let width = S::server_options().diagnostics_parallelism();

    let mut paths = Vec::new();
    for path in walker.files()? {
        if matchers.find_path(&path).is_some() {
            paths.push(path);
        }
    }
    paths.sort();
    let roots = walker.roots().to_vec();

    let mut bootstrap = OneshotServer::new(server);
    bootstrap.initialize_workspace(&roots).await?;

    let results = for_each_bounded(paths, width, |path| {
        let mut server = bootstrap.clone();
        let matchers = &matchers;
        async move {
            let uri = path_to_url(&path)?;
            let Some(matcher) = matchers.find_path(&path) else {
                return Ok(None);
            };
            let language_id = matcher
                .lang_strings()
                .first()
                .cloned()
                .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
            let text = tokio::task::spawn_blocking({
                let path = path.clone();
                move || std::fs::read_to_string(&path)
            })
            .await
            .map_err(ServerError::from)??;
            let document = OneshotDocument {
                uri: uri.clone(),
                language_id,
                version: 1,
                text,
            };
            server.open_document(&document)?;
            let report = server.document_diagnostics(&document).await?;
            Ok(Some(DocumentDiagnostics {
                uri,
                version: document.version,
                report,
            }))
        }
    })
    .await?;

    let documents = results.into_iter().flatten().collect();
    Ok(WorkspaceDiagnosticReport { documents })
}
```

`OneshotServer` needs to be `Clone` (derive on it too — it wraps the now-Clone inner). The closure captures `matchers` by reference from the enclosing scope; if the borrow outlives the engine's future (it does not — the engine is awaited before `matchers` drops), keep the reference, else clone `DocumentMatchers` into the closure (it is an `Arc`-carrying cheap clone). Ordering: `paths.sort()` before the engine preserves the old deterministic output order (the old code sorted documents by path; results keep input order).

- [ ] **Step 3: Structural test + existing tests**

Existing oneshot doctests and unit tests (LongLineServer etc.) must pass unmodified. Add one structural test: a `Server` with gated `document_diagnostics` (entry channel + barrier), 3-file temp workspace, assert all three enter concurrently under the default width (bounded timeout; CI runners have ≥2 cores — use `with_diagnostics_parallelism(NonZeroUsize::new(3)…)` on the test server's `server_options` so the test does not depend on the machine's core count).

- [ ] **Step 4: Verify**

Run: `cargo test --workspace oneshot`
Expected: all pass incl. 1 new.
Run: `cargo test --workspace && cargo test --workspace --no-default-features && cargo test --workspace --all-features`
Expected: 255 / 221 / 255 passed, 0 failed.
Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: exit 0.

- [ ] **Step 5: Checkpoint** — report `src/server/with_state/mod.rs`, `src/oneshot/server.rs`, `src/oneshot/workspace_diagnostics.rs`. No git.

---

### Task 10: Criterion bench harness

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]` criterion; `[dev-dependencies]`; `[[bench]]`)
- Create: `benches/oneshot_diagnostics.rs`

**Interfaces:**
- Produces: `cargo bench --bench oneshot_diagnostics` (on-demand; NOT in the CI battery — `--all-targets` builds it, so it must compile and lint clean in all three feature configurations).

- [ ] **Step 1: Manifest**

`[workspace.dependencies]`: `criterion = "0.8"` (alphabetical slot). `[dev-dependencies]`: `criterion = { workspace = true }`. `[[bench]]`:

```toml
[[bench]]
name = "oneshot_diagnostics"
harness = false
```

- [ ] **Step 2: The bench**

```rust
//! On-demand wall-clock benchmark of the batch diagnostics pipeline over
//! a synthetic workspace. Run with `cargo bench --bench oneshot_diagnostics`;
//! not part of the CI battery.

use std::{num::NonZeroUsize, path::PathBuf, time::Duration};

use async_language_server::server::{DocumentMatcher, Server, ServerOptions, ServerState};
use async_language_server::oneshot::{WorkspaceDiagnosticConfig, workspace_diagnostics};
use async_lsp::lsp_types::{DocumentDiagnosticReportResult, FullDocumentDiagnosticReport};
use criterion::{Criterion, criterion_group, criterion_main};
use tokio::runtime::Runtime;

/// A CPU-bound stand-in for a real validator: burns a fixed budget per
/// document so the bench measures pipeline throughput, not IO.
struct BurnServer;

impl Server for BurnServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("bench").with_glob("**/*.bench")]
    }

    fn server_options() -> ServerOptions {
        ServerOptions::default().with_diagnostics_parallelism(
            NonZeroUsize::new(WIDTH).expect("constant is nonzero"),
        )
    }

    async fn document_diagnostics(
        &self,
        _state: ServerState,
        params: async_lsp::lsp_types::DocumentDiagnosticParams,
    ) -> async_language_server::server::ServerResult<DocumentDiagnosticReportResult> {
        black_box_burn(BURN_ITERATIONS);
        Ok(DocumentDiagnosticReportResult::Report(
            async_lsp::lsp_types::DocumentDiagnosticReport::Full(FullDocumentDiagnosticReport {
                result_id: None,
                items: Vec::new(),
            }),
        ))
    }
}

const FILES: usize = 256;
const WIDTH: usize = 4; // fixed so runs are comparable across machines
const BURN_ITERATIONS: usize = 20_000;

fn black_box_burn(iterations: usize) {
    let mut acc = 0u64;
    for i in 0..iterations {
        acc = acc.wrapping_add(i as u64).rotate_left(1);
    }
    std::hint::black_box(acc);
}

fn make_workspace(files: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "als-bench-{}-{files}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("bench dir");
    for i in 0..files {
        std::fs::write(
            dir.join(format!("file{i}.bench")),
            format!("line one {i}\nline two\nline three\n"),
        )
        .expect("bench file");
    }
    dir
}

fn bench_oneshot(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let root = make_workspace(FILES);
    let mut group = c.benchmark_group("oneshot_diagnostics");
    group.measurement_time(Duration::from_secs(10));
    group.bench_function("parallel", |b| {
        b.iter(|| {
            let report = runtime
                .block_on(workspace_diagnostics(
                    BurnServer,
                    WorkspaceDiagnosticConfig::new(&root),
                ))
                .expect("bench run");
            std::hint::black_box(report);
        })
    });
    group.finish();
    std::fs::remove_dir_all(&root).ok();
}

criterion_group!(benches, bench_oneshot);
criterion_main!(benches);
```

Adjust imports to the crate's actual public paths (`DocumentMatcher`/`Server`/`ServerState` re-exports under `async_language_server::server::*`, oneshot under `async_language_server::oneshot::*`); match method names against the real `Server` trait (`server_document_matchers`/`server_options` associated fns, `document_diagnostics` signature) — mirror `examples/minimal.rs`. Note: `expect` in benches is outside `clippy.toml`'s test allowance — if `expect_used` denies it, restructure the two fallible setup calls (`Runtime::new`, dir creation) into a `criterion::Criterion`-compatible fallible main… simplest compliant form: use `Criterion::configure_from_args` with a bench function that constructs the workspace via `std::fs` results threaded through `Option`/early-return closures — if deny still fires, add `[[bench]]` code to `tests` allowance ONLY via `clippy.toml` is FORBIDDEN; instead make setup infallible-by-construction where possible and keep the rest `Result`-threaded. Verify with clippy before reporting.

- [ ] **Step 3: Verify**

Run: `cargo build --workspace --all-targets && cargo build --workspace --all-targets --no-default-features && cargo build --workspace --all-targets --all-features`
Expected: exit 0 ×3 (bench target compiles everywhere).
Run: `cargo bench --bench oneshot_diagnostics -- --warm-up-time 1 --measurement-time 2`
Expected: runs, prints statistics, exit 0 (a smoke run, not a recorded measurement).
Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`
Expected: 255 passed; exit 0.

- [ ] **Step 4: Checkpoint** — report `Cargo.toml`, `Cargo.lock`, `benches/oneshot_diagnostics.rs`. No git.

---

### Task 11: Docs sync

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `.claude/rules/structure.md`, `.claude/rules/testing.md`, `.claude/rules/tech.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Apply the factual updates**

- `CLAUDE.md` (architecture section): "concurrency limit of 8" → core-derived limit; mention batch parallelism default.
- `README.md`: one sentence in the feature list — heavy lifting under the hood: core-count parallel batch diagnostics, incremental workspace refresh, `ServerOptions::with_diagnostics_parallelism`.
- `.claude/rules/structure.md`: `serve()` paragraph limit wording; new "Batch engine" note under Diagnostics surfaces (`src/workspace/parallel.rs`, `for_each_bounded`, width from `ServerOptions`, `CONTENT_MODIFIED` all-or-nothing preserved); incremental refresh stamp on `DocumentEntry`.
- `.claude/rules/testing.md`: the concurrency tripwire paragraph — test renamed to `at_most_limit_requests_run_concurrently`, limit is `available_parallelism()`; the structural parallel-gate pattern (channels + bounded timeouts, never wall-clock asserts) joins the harness conventions; criterion bench is on-demand, outside the battery.
- `.claude/rules/tech.md`: verification battery unchanged; add "criterion benches run on demand (`cargo bench --bench oneshot_diagnostics`), not in CI".

- [ ] **Step 2: Sweep**

Run: `grep -rn "MAX_CONCURRENT_REQUESTS\|concurrency limit of 8\|at_most_eight" README.md CLAUDE.md .claude/rules/`
Expected: no output after the edits. (Wrap-aware: also grep the bare token `at_most` and `eight` and inspect each hit.)

- [ ] **Step 3: Verify**

Run: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
Expected: exit 0 (README is compiled into the docs).

- [ ] **Step 4: Checkpoint** — report the five files. No git.

---

### Task 12: Final verification battery

**Files:** none (runs the gate).

- [ ] **Step 1: The full battery**

```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo test --workspace --no-default-features
cargo test --workspace --all-features
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Expected: exit 0 each; test counts = baseline + new tests (report exact numbers; approximately 255 / 221 / 255).

- [ ] **Step 2: Spec grep criteria**

Run: `grep -rn "MAX_CONCURRENT_REQUESTS" src/ macros/ examples/ benches/ tests/` → no output.
Run: `grep -rn "text_contents()" src/server/state/documents.rs` → no output.
Confirm `src/server/tests/` changed only in `robustness.rs` (Task 1's rename).

- [ ] **Step 3: Report checkpoints**

1. `src/server/serve.rs`, `src/server/tests/robustness.rs`
2. `src/workspace/diagnostics.rs` (merge)
3. `src/server/state/mod.rs`, `src/server/with_state/mod.rs`, `macros/src/dispatch.rs`
4. `src/server/state/documents.rs` (parse path)
5. `src/server/options.rs`, `src/server/state/mod.rs` (knob)
6. `src/workspace/parallel.rs`, `src/workspace/mod.rs`
7. `src/documents/matcher.rs`, `src/server/state/mod.rs`, `src/server/state/workspace.rs`
8. `src/server/state/workspace.rs`, `src/workspace/diagnostics.rs` (parallel)
9. `src/server/with_state/mod.rs`, `src/oneshot/server.rs`, `src/oneshot/workspace_diagnostics.rs`
10. `Cargo.toml`, `Cargo.lock`, `benches/oneshot_diagnostics.rs`
11. `README.md`, `CLAUDE.md`, `.claude/rules/structure.md`, `.claude/rules/testing.md`, `.claude/rules/tech.md`

Owner commits; the commit message notes the behavior-visible changes (request limit 8 → cores) per product.md.

---

## Self-review notes

- Spec coverage: components 1→T1, 2→T5+T6+T8 (engine, knob, workspace surface) +T9 (oneshot), 3→T7(+T8's async), 4→T2, 5→T3, 6→T4, 7→T10; docs →T11; decisions 1-7 all reflected (decision 4 = no parser-reuse task; decision 6 = no async-io anywhere; decision 5 = flat ServerOptions untouched beyond the one knob).
- Placeholder scan: the two flagged implementer-adaptation points (T2's test construction details; T10's public-path alignment to real trait signatures) name exact fallbacks, not open TBDs.
- Type consistency: `for_each_bounded` signature identical in T6/T8/T9; `diagnostics_parallelism()` flows options→state identically in T5/T8/T9/T10; `FileStamp`/`stamp` consistent across T7/T8; `WorkspaceReportSink` in T2/T8.
- Ordering: every task leaves the tree green; T8 requires T5-T7; T9 requires T6 (+T5 for the knob); T10 requires T8+T9 shapes.
