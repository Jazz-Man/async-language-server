# Research: `Document` handle conversion, compile cache, tokens eviction, parallel walker

Date: 2026-09-06. Read-only feasibility research ahead of speccing. All paths relative to the
crate root; vendored dependency sources cited from `~/.cargo/registry/src/`. Inferences are
labeled `[Inference]`; everything else was read directly from the cited source.

---

## Q1. Full `Document` usage inventory

### (a) Construction sites — exactly two in the crate, zero downstream-reachable

1. `src/server/state/documents.rs:61` (`ServerState::insert_document`) — sets all seven fields:
   `uri`, `text: Rope::from(text)`, `version`, `language`, `matcher` (resolved via
   `self.matchers.find`, :56), `tree_sitter_lang` (:36-39) and `tree_sitter_tree`
   (parsed fresh, :44-54). Reached from: `handle_document_open` (:82), `handle_document_close`
   keep-as-workspace path (:116), `recover_failed_incremental_update` (:233),
   `handle_watched_files_change` Created/Changed (:344), and
   `load_workspace_document` (`src/server/state/workspace.rs:188`).
2. `src/server/with_state/mod.rs:64` (`read_document_from_disk`) — `uri: url.clone()`,
   `text: Rope::from(text)`, `version: 0`, `language: String::new()`, `matcher: None`,
   no grammar/tree. Per-request conversion fallback.

`OneshotDocument` (`src/oneshot/server.rs:83-88`) is a separate struct (uri/language_id/
version/text as owned values), not a `Document`; it feeds `did_open` (:45-52).

Under a handle design both sites construct `DocumentInner` instead; neither is downstream-
reachable (see Q2).

### (b) Field access sites

All direct field reads/writes live in two files — every other consumer goes through the
public accessors (`url`, `text`, `version`, `language`, `matched_name`, `text_reader`,
`text_contents`, `text_bytes`, `node_*`, `query`) or takes `&Document`:

- `src/server/state/documents.rs` — `doc.version` (:134), `doc.text`
  (:163, :175-183, read via `doc.text()` at :201, :297), `doc.tree_sitter_tree`
  (:161, :194, :202, :251, :307, :431), `doc.tree_sitter_lang` (:306, and :391 in
  `doc_parser`), `doc.uri` (:208), `doc.matcher` (:286), `entry.document.language`
  (:102, :340).
- `src/server/state/workspace.rs:66` — `entry.document.uri.clone()`;
  :75 — `entry.document.version()` (accessor).

`&Document` consumers (unchanged by the handle): every conversion hook in
`src/lsp_requests/conversion.rs` (`document: &Document` at :44, :57, :73, :83, :94, :106,
:119, :134, :150, :162, :171, :188, :213, :235, :249, :263), the macro-stamped hook
signature `&crate::server::Document` (`macros/src/request.rs:125,140`), the dispatch
engine's `conversion_document` (`src/server/with_state/mod.rs:47-52`),
`src/lsp_requests/symbol.rs:31,56` (`HashMap<Url, Option<Document>>` per-request disk
cache), `src/workspace/diagnostics.rs:408-431` (`doc.version()` + passes `&doc` to
`modify_response`), and the three examples (`examples/minimal.rs:45`,
`examples/tree_sitter.rs:53`, `examples/batch_diagnostics.rs:63`) plus tests — all public
accessors only.

### (c) Move sites

`Document` moves only as a return value / map value: `document()`
(`src/server/state/mod.rs:79`), `documents()` (:89), `sole_document()` (:110),
`conversion_document` (`src/server/with_state/mod.rs:47`), `read_document_from_disk` (:57),
and `symbol.rs`'s `HashMap<Url, Option<Document>>`. `DocumentEntry.document`
(`src/server/state/mod.rs:41`) is the only struct field holding one. No code moves a field
out of a `Document` by value — `src/workspace/diagnostics.rs:369`'s `previous.uri` moves out
of an lsp_types `PreviousResultId` (params), not a `Document`. Nothing keeps a `Document`
in a long-lived struct beyond `DocumentEntry`. Under the handle these all keep compiling.

### (d) Mutation sites (in place under the DashMap guard)

- `handle_document_change` (`src/server/state/documents.rs:124-218`): `entry.origin`
  (:132, DocumentEntry field, not Document), `doc.version` (:134),
  `tree.edit(&edit)` via `doc.tree_sitter_tree.as_mut()` (:161-172),
  `doc.text.try_remove`/`try_insert` (:175-183), `doc.tree_sitter_tree = updated_tree`
  (:202 after re-parse).
- `recover_failed_incremental_update` — `doc.tree_sitter_tree` rewrite (:251).
- `handle_document_save` (:258-311): `doc.text = text` (:281),
  `doc.matcher.clone_from(&matcher)` (:286), `doc.tree_sitter_lang = …` and
  `doc.tree_sitter_tree = …` (:306-307).
- `replace_full_text` (`src/server/state/documents.rs:425-437`) —
  `doc.tree_sitter_tree` (:431) and `doc.text` (:436).
- `load_workspace_document` — `entry.stamp` (`src/server/state/workspace.rs:190`,
  DocumentEntry field, not Document).

Every mutation happens under `documents.get_mut` (documents.rs:128, :248, :263) — the
DashMap write guard is the current "exclusive access" mechanism a COW write path must
reproduce.

## Q2. Visibility blast radius

`Document`'s fields are `pub(crate)`, not `pub` (`src/documents/document.rs:41-49`), on a
`pub struct` with `#[derive(Debug, Clone)]` (:39). There is no public constructor; the only
two construction literals are crate-internal (Q1a). The public surface is: accessor methods
(:52-207), `AsRef<Rope>` (:209-213), `DocumentReader` (:218-256, private fields),
`DocumentQueryCapture` (:264-271). Exported via `src/documents/mod.rs:6` and re-exported
through `src/server/mod.rs:32` (`pub use crate::documents::{Document, DocumentReader}`).

Downstream servers can only obtain a `Document` from `state.document()`/`documents()` and
read it through accessors — they cannot construct, match-destructure, or mutate one.
**Conclusion: the handle conversion is NOT breaking for downstream users** provided the
accessor signatures stay identical (`url() -> &Url`, `text() -> &Rope`,
`version() -> i32`, `language() -> &str`, `matched_name() -> Option<&str>`,
`text_reader() -> DocumentReader<'_>`, the `node_*`/`query` family). `text_reader` borrows
`&self` and returns a reader over the rope; behind an `Arc<DocumentInner>` the borrow path
is unchanged as long as the reader keeps borrowing the handle's lifetime.

## Q3. The didChange COW allocation question

### What a per-batch naive `Inner` rebuild would clone today

`didChange` mutates only `version`, `text` (rope edits), and `tree_sitter_tree`
(documents.rs:134, :175-183, :202). A flat
`Arc::make_mut`-style copy of the whole `DocumentInner` per change batch would therefore
needlessly clone:

| field | clone cost |
|---|---|
| `uri: Url` | heap alloc + string copy |
| `language: String` | heap alloc + string copy |
| `matcher: Option<Arc<DocumentMatcher>>` | Arc refcount bump (cheap) |
| `tree_sitter_lang: Option<Language>` | `ts_language_copy` — pointer copy for native grammars, wasm refcount (see Q4) |
| `tree_sitter_tree: Option<Tree>` | `ts_tree_copy` — one small `TSTree` alloc + root-subtree refcount (`tree-sitter-0.26.13/src/tree.c:22-25`) |
| `text: Rope` | `#[derive(Clone)]` over `root: Arc<Node>` — single Arc bump (`ropey-1.6.1/src/rope.rs:81-84`) |

So a naive per-batch rebuild costs **two string allocations** (uri, language) plus the small
tree alloc, where today's in-place edit costs zero allocations for meta. (The re-parse in
the tree-sitter path already allocates a whole new `Tree` per edited batch, so the tree
clone itself is not the new cost; the strings are.)

### The split mitigation

`DocumentInner { meta: Arc<DocumentMeta { uri, language }>, text: Rope, version: i32,
matcher: …, grammar: …, tree }` — or strictly as proposed, meta holding
`{uri, language, matcher, grammar}` — reduces a didChange COW to: meta Arc bump + Rope Arc
bump + `i32` copy + `Option<Tree>` clone (one small alloc). Both string allocations
disappear from the hot path.

**Evidence that `uri` and `language` are never rewritten in place**: the only writes to
`uri`/`language` are at construction (documents.rs:62, :65; with_state/mod.rs:65, :68).
Every path that needs "different" uri/language content builds a *new* document via
`insert_document` instead (didClose :116, recovery :233, watched-files :344, refresh
workspace.rs:188). `[Inference]` from exhaustive read of `src/server/state/` and
`src/server/with_state/`: no other assignment exists.

**Counter-evidence for a four-field meta**: `matcher` and `tree_sitter_lang` ARE mutated in
place on an existing document — by `handle_document_save` (documents.rs:286, :306). Under
an `Arc<DocumentMeta>` containing them, every `didSave` pays an `Arc::make_mut`-style meta
clone (two string copies) — acceptable because saves are rare relative to keystrokes, but
avoidable entirely: `matcher`/`grammar` are only ever rewritten *together with the text*
(didSave :281-307), so an alternative split `meta = {uri, language}` (truly immutable) +
mutable half `{text, version, tree, matcher, grammar}` needs no meta COW at all.

**Judge: the split removes the regression.** A didChange batch then clones two Arcs and an
i32 — same order as today's clone in `state.document()` but *cheaper than today's
snapshot*, and zero string allocations on the hot path.

### Correctness constraint the design must carry (load-bearing)

`state.document()`'s contract is snapshot semantics: "the document exactly as it was at the
time of calling" (`src/server/state/mod.rs:70-76`). Consumers rely on it:
`src/workspace/diagnostics.rs:408-425` holds `doc` across
`server.document_diagnostics(..).await` and detects staleness by version probe; the
staleness/`CONTENT_MODIFIED` protocol in the dispatch engine
(`macros/src/dispatch.rs:159-173`) probes versions clone-free around a snapshot. Therefore
the handle must be **COW-on-write under the DashMap guard** (`Arc::make_mut` or manual
shared-check + clone) — never interior mutability (`Mutex`/`RwLock` inside `Document`),
which would turn snapshots into live views and silently break staleness detection.
`[Inference]` about the failure mode, but the snapshot contract itself is documented at
state/mod.rs:70-76.

## Q4. `tree_sitter::Query` / `Language` lifetime semantics (tree-sitter 0.26.13, vendored)

- `Language` is `#[repr(transparent)] pub struct Language(*const ffi::TSLanguage)` with
  `#[derive(Debug, PartialEq, Eq, Hash)]` (`binding_rust/lib.rs:59-62`). So it is a
  pointer wrapper, usable directly as a `HashMap`/identity key, and pointer equality is
  grammar identity.
- `impl Clone for Language` calls `ffi::ts_language_copy`; `Drop` calls
  `ts_language_delete` (lib.rs:686-694). In C, `ts_language_copy` returns the same pointer
  and only retains WASM languages (`src/language.c:6-11`); `ts_language_delete` likewise
  only releases WASM (`language.c:13-17`). **A `Language` clone is free for native
  grammars.**
- `Query::new(&Language, &str)` borrows the `Language` only for the call
  (lib.rs:2344-2348); the Rust `Query` struct stores only
  `ptr: NonNull<TSQuery>` + owned boxes of names/predicates (lib.rs:326-334) — the Rust
  wrapper does *not* keep the `Language` alive.
- At the C level, however, `ts_query_new` stores
  `.language = ts_language_copy(language)` (`src/query.c:3050`) and `ts_query_delete`
  calls `ts_language_delete(self->language)` (`query.c:3220`). **The compiled query
  participates in the language's lifetime itself** (no-op retain/release for native
  grammars, real refcount for WASM ones).
- `unsafe impl Send + Sync for Query` (lib.rs:3901-3902) and for `Language`
  (:3886-3887) — a cache shared across threads inside a `DashMap` value is fine.
  `DocumentMatcher` already owns `Option<Language>` (`src/documents/matcher.rs:35`) and
  hands out clones (`matcher.rs:102-103`), so the cache key is available at lookup time.

**Conclusion: a compile cache keyed by grammar identity whose values own a `Language`
clone + compiled `Query` is sound without lifetime games** — and strictly more
conservative than required, since even a cache owning only the `Query` keeps the language
referenced at the C level. Owning the `Language` clone in the entry removes any reasoning
about native-vs-wasm retention. `[Inference]`: native `TSLanguage` pointers are static
data from grammar crates (the `tree_sitter_json::LANGUAGE.into()` pattern in
matcher.rs:260), which is also why pointer-identity keys work.

## Q5. Semantic-tokens cache eviction sites

The cache is `semantic_tokens_cache: Arc<DashMap<Url, CachedSemanticTokens>>`
(`src/server/state/mod.rs:32`), with `cached_semantic_tokens` (:170-174) and
`store_semantic_tokens` (:178-180). Writes: `conversion.rs:816` (full-result seeding) and
`conversion.rs:1051` (`splice_semantic_tokens_cache`, delta re-splice). Reads:
`conversion.rs:890` (delta base). **No code removes or clears cache entries anywhere in
`src/`** — verified by grep (`semantic_tokens_cache` appears only at the sites above; no
`.remove`/`.retain`/`.clear` touches it).

Lifecycle events that should evict (the document-map counterpart, with file:line):

Definite removals (document dropped from `documents`):
1. `didClose`, not kept as workspace — `documents.remove` (`src/server/state/documents.rs:110`)
2. `didClose`, disk re-read failed — `documents.remove` (documents.rs:118)
3. watched-files DELETED — `remove_if` (documents.rs:330-332)
4. `handle_files_renamed` → `remove_workspace_document_by_uri_string`
   (documents.rs:358-361, removal at :384-385)
5. `handle_files_deleted` → same helper (documents.rs:366-372, removal at :384-385)
6. workspace folders removed — `remove_workspace_documents_in_roots` retain
   (`src/server/state/workspace.rs:41`, :141-143)
7. workspace diagnostics disabled — `remove_workspace_documents` retain
   (`src/server/state/mod.rs:154` → workspace.rs:131-134)
8. `refresh_workspace_documents` retain-removal of unmatched/left-roots workspace docs
   (workspace.rs:120-124)

Staleness sites (same URL re-inserted/replaced — cached stream no longer corresponds to
the stored document; protocol-wise the client's `previous_result_id` may still reconcile,
so these are candidates rather than hard evictions):
9. `didClose` keep-as-workspace re-insert (documents.rs:116)
10. failed-incremental recovery re-insert / kept-text re-parse (documents.rs:214 → :233, :248-254)
11. `didSave` full-text replace + tree rewrite (documents.rs:281, :306-307)
12. watched-files Created/Changed re-insert (documents.rs:344)
13. refresh `load_workspace_document` re-insert (workspace.rs:188)

(Plain `didChange` also makes the cached stream diverge, but the delta protocol computes
against the previously *sent* stream — flagging it here for completeness, not as an
eviction candidate. `[Inference]` on protocol intent from the doc comments at
state/mod.rs:52-59 and conversion.rs:743.)

## Q6. Walker parallel API

Current shape (`src/workspace/walker.rs`): `WorkspaceWalker::files()` walks each root
serially with `WalkBuilder::build()` (:61-78) — the plain `Iterator`-based `Walk` — skips
error entries with a `tracing::warn!` + continue (:66-72), keeps files
(:74-76), then `files.sort()` (:80). Callers: `refresh_workspace_documents`
(`src/server/state/workspace.rs:90-94`, called from the async workspace/diagnostic path)
and `oneshot::workspace_diagnostics` (`src/oneshot/workspace_diagnostics.rs:218-222`).

Vendored `ignore` 0.4.33 parallel semantics (`src/walk.rs`):
- `WalkBuilder::build_parallel()` returns a `WalkParallel`, explicitly *not* an `Iterator`
  (:696-701). It is driven by `run(||
  entry_callback) -> WalkState` (:1424-1433) or the custom
  `ParallelVisitorBuilder`/`visit` API (:1452-1455), on an internal thread pool.
- Default thread count: `available_parallelism().min(12)` when unset (:1537-1543);
  configurable via `WalkBuilder::threads` (:773-776).
- Entries arrive as `Result<DirEntry, Error>` at the visitor from multiple threads; the
  docs prescribe per-thread accumulation merged after traversal (:1442-1451). Control via
  `WalkState::{Continue, Skip, Quit}` (:1325-1337).
- The builder's filter configuration (hidden, ignore files, gitignore family) is carried
  into `WalkParallel` through the built ignore root (:698-710) — `configure_walker`
  (walker.rs:85-94) carries over unchanged.

**Sorted-output guarantee**: `WalkParallel` promises no ordering; that is irrelevant here —
the sorted contract of `files()` is produced by the explicit `files.sort()` at
walker.rs:80 *after* collection, which a parallel visitor preserves by construction
(collect into a shared `Mutex<Vec<_>>`/channel, sort once at the end). The error-skip
behavior maps to logging + `WalkState::Continue`.

**Feasibility: yes, mechanically.** `files()` stays a synchronous `Vec`-returning fn —
`run()` blocks until traversal completes, and its callers already invoke the serial
`files()` synchronously today. The change is contained in `walker.rs` plus the error
delivery shape. Note the parallel walk duplicates the traversal engine the crate already
parallelizes one level up (`for_each_bounded` over per-file loads, workspace.rs:113-117);
whether walk-parallelism measurably helps depends on where time actually goes — the
criterion bench exists to check that (`benches/oneshot_diagnostics.rs`).

---

## Verdict

**A1' (Document → cheap-Clone handle, `state.document()` signature unchanged): FEASIBLE
AS-IS for the public surface, WITH THE META SPLIT for the hot path.** Fields are
`pub(crate)` with no public constructor, so nothing downstream breaks (Q2); all mutation
is confined to `src/server/state/documents.rs` under the DashMap guard (Q1d); uri/language
are construction-immutable, so a split `meta: Arc<{uri, language}>` makes a didChange COW
cost two Arc bumps + an i32 and removes the per-batch string-allocation regression (Q3).
`matcher`/`grammar` should ride the mutable half (or accept a didSave-time meta clone) —
they are rewritten in place by `handle_document_save` (documents.rs:286, :306). The design
must stay copy-on-write under the DashMap guard; interior mutability would break the
documented snapshot contract and the staleness protocol (Q3, load-bearing).

**Query compile cache: SOUND.** Key on `Language` (pointer `Eq`/`Hash`); value owns a
`Language` clone + `Query`; `Query: Send + Sync`; C-level retain/release covers the
lifetime; `DocumentMatcher` already supplies `Language` clones (Q4).

**Semantic-tokens eviction: needed and currently absent.** 8 definite lifecycle-removal
sites + 5 replacement/staleness sites enumerated (Q5); today the cache only ever grows.

**Parallel walker: FEASIBLE.** `build_parallel`'s callback/pool model fits `files()` with
a shared collector; the sorted contract lives in the existing post-collection
`sort()`; benefit unproven until benched (Q6).
