# Walk-cache design — Q2 granularity, Q3 error policy, Q4 test seam (research)

Branch `feature/walk-cache` design cycle, 2026-09-16. Companion to
`2026-09-16-walk-cache-design-q1.md` (invalidator set). Scope: local code and
rules analysis only, read-only; no web research. Per question:
evidence → conclusion → recommendation. Code claims are [measured] (read in
this pass, `file:line`); judgment claims are [inferred].

---

## Q2 — What the cache should store

### Evidence

**What the per-poll list phase does today** (`refresh_workspace_documents`,
`src/server/state/workspace.rs:76-125`): after `walker.files()`, for every
walked path it runs

1. `matchers.find_path(&path)` — linear scan of the matchers' compiled
   `GlobSet`s (`src/documents/matcher.rs:221-226`); `continue` on no match
   (`workspace.rs:91-93`);
2. `path_to_url(&path)` — `Url::from_file_path`, an allocating conversion
   (`src/workspace/walker.rs:102-106`);
3. an `Open`-origin probe in the documents map (`workspace.rs:96-102`), then
   the path is queued for loading.

With raw `PathBuf`s in the cache, steps 1–2 re-run for every cached file on
every poll. With `(PathBuf, Url, Arc<DocumentMatcher>)` triples they
disappear: the list phase becomes a clone of cached triples plus the existing
dedup and sort (`workspace.rs:115,122-123`). The glob-match cost is the
CPU-dominated part of that phase — O(files × matchers) per poll — [inferred;
no benchmark was run for this note].

**Matcher lifetime: startup-static.** `DocumentMatchers` is built exactly
once, in `ServerState::with_options` from `T::server_document_matchers()`
(`src/server/state/mod.rs:122`), and stored as a plain field (`:26`); grep
found no setter and no reassignment anywhere in `src/` [measured]. Its
internals are `Arc`-cloned (`matcher.rs:157-160`) and `find_path` returns
`Arc<DocumentMatcher>` (`matcher.rs:221-226`). So an `Arc<DocumentMatcher>`
held in the cache is valid for the server's lifetime — there is no matcher
invalidation coupling to design for.

**Precedent:** `Document` already stores `Option<Arc<DocumentMatcher>>`
(`src/server/state/documents.rs:51,58-72`) — the identical sharing pattern is
in production today, and the matcher's per-matcher compiled-query cache
(`matcher.rs:47,134-151`) only benefits from more `Arc` sharing. The triple
introduces no new lifetime class.

**Correctness of the cached `Url`:** `path_to_url` is a pure function of the
path string; the `Url` does not depend on the file existing. Recreating a
file at the same path yields the same `Url`.

**Store only matched files.** The unmatched-path skip (`workspace.rs:91-93`)
should happen once at cache-write time, so the cached list holds triples
only — no per-poll `find_path` at all.

**Two subtleties to preserve verbatim:**

- The walker canonicalizes roots internally (`walker.rs:43-50`), so walked
  paths (and hence cached paths) are canonical, while `url_is_in_roots` in
  the retention predicate compares against the *raw* roots from
  `workspace_roots()` (`workspace.rs:81,116-119`). The cache must not change
  this asymmetry — feed `url_is_in_roots` the same raw roots as today.
- Nested roots duplicate files (walker test, `walker.rs:138-157`); the
  `HashSet` dedup (`workspace.rs:115`) absorbs that and is unchanged by
  triples.

**Coupling note for the Q1 agent:** matchers need no invalidator (static),
but the *roots* are walk inputs and mutate on folder events
(`workspace.rs:23-46`) — folder add/remove must be in the invalidator set.

### Conclusion

Cache the walked list as `(PathBuf, Url, Arc<DocumentMatcher>)` triples of
*matched* files only. Raw `PathBuf`s leave the expensive per-poll work (glob
matching, URL construction) in place and cache only the directory walk.
Matcher safety is settled by the startup-static construction; no invalidation
coupling exists for the matcher half of the triple.

### Recommendation

Store `Vec<(PathBuf, Url, Arc<DocumentMatcher>)>` (matched entries only).
Keep the `oneshot` path direct-walking (`src/oneshot/workspace_diagnostics.rs:214-226`)
— it is a single pass; a cache there is dead weight.

---

## Q3 — Walk-error and load-error policy under a cache

### Evidence

**The real walk-error surface is smaller than it looks** [measured]:

- Per-entry errors inside `files()` are already skip + warn + continue
  (`walker.rs:76-79`; pinned by `files_skips_unreadable_entries`,
  `walker.rs:199-230`).
- `files()`' body contains no `Err` path at all (`walker.rs:56-88`) — its
  `ServerResult` return is de facto infallible.
- The only genuine `Err` on the walk path is `WorkspaceWalker::new`'s
  `fs::canonicalize` over the roots (`walker.rs:43-50`), reached from
  `workspace.rs:86`. So "walk error" today means: one root failed to
  canonicalize → the whole refresh aborts.

**Per-file load failure, traced with a cached list** (file deleted on disk
between polls) [measured path]:

1. The cached triple survives — glob matching is pure string work, no fs
   access (`matcher.rs:221-226`).
2. `load_workspace_document`: `file_stamp` returns `None` (metadata fails,
   `workspace.rs:142-146`); `stamp_unchanged(Some(_), None)` is `false`
   (`workspace.rs:198-200`, pinned by `stamp_gate_is_conservative`,
   `workspace.rs:217-234`) → proceed to read.
3. `std::fs::read_to_string` fails → `ServerError::Io` via `#[from]`
   (`workspace.rs:177-188`; `src/error.rs:114-115`).
4. `for_each_bounded` runs every item, then propagates the first error in
   input order (`src/workspace/parallel.rs:3-6,21-40`).
5. `refresh_workspace_documents` propagates (`workspace.rs:109-113`) →
   `workspace_diagnostic_items` maps it into a `ResponseError`
   (`src/workspace/diagnostics.rs:403-406`) → **the entire
   `workspace/diagnostic` poll fails**.

Today (no cache) the same failure exists but self-heals: the next walk omits
the deleted file, so at most one poll fails. With a cached list the deleted
file persists, and all-or-nothing turns the cache into a poisonable state —
every poll fails until the entry is invalidated. And per the Q1 sibling
research, no `didChangeWatchedFiles` registration is ever sent today
(`2026-09-16-walk-cache-design-q1.md:17-19`), so deletes are frequently
*un-signaled*: the skip policy is the required backstop, not an optional
nicety.

**Rules reconciliation.** No live tension — the two rules govern different
layers:

- `error-handling.md` (api-dir-enumeration) governs *enumeration-shaped
  passes*: "a stream of fallible entries is not one failure — skip an
  unreadable entry, trace it, and continue". The walk already complies
  (`walker.rs:76-79`). The load pass is the same shape — refreshing the
  workspace view — and today is the one place in that pass that still
  aborts.
- `structure.md`'s "all-or-nothing outcome, including `CONTENT_MODIFIED`, is
  preserved" describes `for_each_bounded`'s *error-ordering* semantics and
  its diagnostics-items consumer (`parallel.rs:3-6`). That consumer keeps
  all-or-nothing regardless of what the loads pass does.

Moving the loads pass from "propagate" to "skip" changes which side of the
line it sits on — from the batch-engine side to the enumeration side, where
error-handling.md says it belongs. `for_each_bounded` itself is untouched.

**Divergent consumer, kept as is:** `oneshot` load errors also abort today
(`src/oneshot/workspace_diagnostics.rs:230-281`, `results?`), and its
`# Errors` doc promises exactly that (`:202-205`). A CLI batch run wants a
hard failure; the LSP poll wants liveness. Leave oneshot unchanged.

**Keep-stale precedent** [measured]: watched-files `CHANGED` keeps the
last-known snapshot when a re-read fails (`documents.rs:385-391`), while
`DELETED` drops the document (`documents.rs:366-373`).

### Conclusions

**(a) On walk error** (`WorkspaceWalker::new` canonicalize failure):
propagate, as today. The cache is written only after a successful walk, so a
failed walk leaves the previous cache untouched — "keep old cache" is the
automatic outcome; there is no partial state to clear. [Inferred] A
resilience option exists — serve the stale cache on constructor failure —
but it changes an error edge for an abnormal state (root deleted without a
folder event); defer it unless the owner asks.

**(b) On per-file load error with a cached list:** skip + warn (with the
error value, per error-handling.md's snippet) + exclude the url from the
returned list. The poll stays alive; the stale `Workspace` document is
dropped by the existing retention predicate (`workspace.rs:116-120`),
matching the watched-`DELETED` precedent; the cache entry stays so a
transiently unreadable file self-heals next poll (one stamp probe + failed
read per poll — negligible).

**Semantic change this forces (flag):** `structure.md`'s diagnostics section
should gain one sentence — per-file load failures during a workspace refresh
are traced and skipped; the poll survives. The batch engine's contract
sentence stays true; the loads consumer simply stops feeding it errors, while
the items consumer keeps all-or-nothing (`CONTENT_MODIFIED`).

**Nuance flagged** [inferred]: excluding the url means the client keeps its
previously reported diagnostics for that uri (a `workspace/diagnostic`
response has no "cleared" signal for an absent item). The alternative —
reporting the stale tracked snapshot's diagnostics — shows diagnostics for a
possibly-ghost file, which is worse. A one-poll diagnostic gap for a
transiently unreadable file is the accepted cost.

### Recommendation

(a) Propagate walk errors; cache untouched by construction. (b) Skip + warn
+ exclude on per-file load errors; document the divergence from oneshot
(liveness vs batch-hard-fail) in the change and in `structure.md`.

---

## Q4 — Test seam: is the black-box route enough?

### Evidence

**The planned pins are black-box observable** over a real temp workspace —
exactly the existing suite's idiom (`temp_workspace` +
`advertise_workspace_diagnostics` + direct `refresh_workspace_documents`
calls, e.g. `src/server/state/tests.rs:98-210`):

1. *cache hit*: write a file, refresh, refresh again → the new file must be
   absent from the returned urls;
2. *invalidation*: fire the invalidation signal, refresh → the file appears.

**Collision point already in the suite** [measured]:
`workspace_refresh_evicts_tokens_of_dropped_documents`
(`state/tests.rs:748-794`) deletes a file on disk, refreshes again, and
expects it gone. With a cache this test passes only if the delete invalidated
the cache or Q3(b)'s skip policy excludes the entry. It becomes the feature's
first regression gate and needs no new machinery.

**"Walk ran at most once" is not observable through urls.** An
implementation that re-walks every poll *and uses the result* fails pin 1.
An implementation that re-walks and *discards* the result is behaviorally
identical to a cache hit in every observable way — a perf-only regression.
No url sequence distinguishes them; only a counter inside the walk would.

**Repo seam idioms are structural, not instrumental** [measured]:
`run_over_streams` drives the real middleware stack over duplex pipes;
`crate::testing` builds real temp workspaces (`src/testing.rs:131`); and
where tests do inject observation, they do it at the `Server`-trait level
(`GatedDiagnosticsServer`'s channels + semaphore, `diagnostics.rs:692-721`;
`for_each_bounded`'s own tests, `parallel.rs:69-99`). No counter-injection
seam into framework internals exists anywhere in the suite — the only
`AtomicU64`s in `src/` are production state (`diagnostics.rs:23,38`).

**testing.md constraints:** assert on what entered, never on elapsed time;
choose the lowest tier that can express the assertion; a test exists only for
behavior no type can express. Perf claims have a home outside tests:
`make bench` exists on demand for measuring the batch pipeline
(`tech.md`), and `make mutants` runs outside the battery.

**Options:** (i) injectable counter in `WorkspaceWalker` — public prod-API
pollution for a mechanism pin; (ii) a seam-fn field on `ServerState` — a prod
field only tests populate, same pollution, softer shape; (iii) no seam.

### Conclusion

Black-box suffices for every *contract* pin: cached-list staleness (pin 1),
invalidation behavior (pin 2), and the deleted-file collision (the existing
`state/tests.rs:748` test). The cache-correctness mutants die there too. The
only unobservable claim — "the walk ran at most once" — is mechanism, not
contract: it belongs to `make bench` (measurement) and, if a
walk-but-discard-class mutant ever survives `make mutants`, to the
equivalent-mutant basket of the disposition table (perf-only, invisible by
design), per testing.md's own baskets.

### Recommendation

**No seam.** Keep `WorkspaceWalker`'s API untouched; pin the cache contract
black-box at the state tier. If the owner ever insists on a walk-count pin,
the least-polluting route would be a `#[cfg(test)]`-only hook on the state —
but the recommendation is against it: a test whose only statement is the
count is a test-pleasing test by testing.md's definition.

---

## Cross-cutting note for the Q1 (invalidator) agent

- The invalidator set must cover workspace-folder changes
  (`workspace.rs:23-46`) — roots are walk inputs — and, where the client
  actually pushes them, watched-files / file-operation events
  (`documents.rs:351-430`).
- Given no watcher registration exists today (Q1 file), the Q3(b) skip
  policy is the *only* defense for un-signaled deletes; design the
  invalidator set assuming it will miss events, not that it won't.
