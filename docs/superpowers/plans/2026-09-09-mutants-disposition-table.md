# Mutant Survivor Disposition Table

Produced by Plan B Task 1 (timeout verification + disposition analysis). Owner-reviewed gate:
only rows approved here proceed to implementation (Tasks 2–5).

**Inventory.** Baseline: `.superpowers/mutants-baseline-2026-09-09/` (149 missed, 12 timeout).
All 12 timeouts were re-verified in isolation with scoped sweeps (`make mutants FILE=<file>`,
clean auto-derived 20 s timeout, nothing concurrent):

- **3 real timeouts** (join the inventory): `DocumentReader::read` ×3 in
  `src/documents/document.rs` — each livelocks the read loop; rows are in cluster 4.
- **9 Miri-session artifacts** (completed as MISSED on re-run, not caught): initialize.rs ×2,
  state/workspace.rs ×4, state/documents.rs ×3. These were *masked* as timeouts by the
  concurrent Miri session; in isolation they run to completion and are **ordinary missed
  mutants**, so they are dispositioned here (flagged `artifact` in the row) — otherwise Task 6's
  full-sweep reconciliation would find nine unexplained survivors. The owner may strike these
  nine rows at this gate; they are grouped at the end of their clusters so that is a clean cut.

**Total rows: 161** = 149 baseline misses + 3 real timeouts + 9 verified artifacts.

**Tally: (a) 0 · (b) 120 · (c) 20 · (d) 21** (lsp.rs:94 b→d 2026-09-09; documents.rs
:207:39, :572*, :574* b→d 2026-09-10 — all owner-ratified equivalent mutants).
Core inventory (152, artifacts excluded): 111 b + 20 c + 21 d.

- **(a) is zero, deliberately.** Testing rule: a type must remove a representable invalid state
  or separate a genuinely confusable pair. Every survivor here is either a value-space behavior
  (arithmetic/comparison/boolean guard inside an already-typed surface — a test is the only
  thing that can see it) or an accessor/projection with no invalid state to remove. Candidates
  examined and rejected: a `Generation` newtype for the diagnostics staleness counter (both
  compared values would share the type — the mutant lives inside the constructor, not in a
  confusable exchange), and a non-empty-string type for `Encoding::as_str`/matcher names (the
  mutants are constant replacements inside a match/field projection, not invalid inputs).
- **Why so many (c) survive nextest:** nextest does not run doctests; the battery's separate
  `cargo test --doc` legs do. Every (c) doctest was hand-traced against its mutant
  (assertion vs. mutated behavior) and verified present and passing on both feature legs
  (all-features and no-default-features, 2026-09-09).
- **Row → patch mapping for kill-verification:** baseline `outcomes.json` `diff_path` per
  mutant (same-line mutants carry `_001`/`_002` suffixes in `diff/`).
- **Test names below are proposals**; implementing tasks may rename but must keep the pinned
  behavior and tier. Tier marks: W0 = inline/sibling unit test; wire = `src/server/tests`
  harness (only where W0 cannot observe).

---

## 1. text_utils conversions — 16 rows (14 b, 2 c, 0 d)

Files: `src/text_utils/encoding.rs`, `src/text_utils/position.rs`.
`Encoding`'s public docs doctest pins `as_str`; `from_lsp` and every `From` impl are unpinned.
LSP: `Encoding::from_lsp` callers are exactly the two `From` impls (encoding.rs:102, :108);
the position `From` impls have no inferable in-crate callers — they are public conversion
surface delegating to doctest-pinned inherent methods.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `Encoding::as_str` @ encoding.rs:48 | `-> &str with ""` | c | Wire-representation contract is pinned by the type-level doctest (`assert_eq!(Encoding::UTF8.as_str(), "utf-8")`, encoding.rs:15); a constant replacement breaks it | doctest `Encoding` (encoding.rs:10) |
| `Encoding::as_str` @ encoding.rs:48 | `-> &str with "xyzzy"` | c | Same assertion catches it | doctest `Encoding` (encoding.rs:10) |
| `Encoding::from_lsp` @ encoding.rs:62 | `-> Self with Default::default()` | b | Documented `# Panics` contract + per-kind mapping ("Creates an encoding from its lsp_types counterpart") is load-bearing: mis-mapping silently re-labels client positions | W0 `from_lsp_round_trips_all_supported_kinds_and_panics_on_unknown` (new, encoding.rs tests): all three kinds + panic on `utf-7` |
| `Encoding::from_lsp` @ encoding.rs:63 | `== with !=` (UTF8 arm) | b | Same mapping contract; UTF8 input must not fall through to UTF16 | same test |
| `Encoding::from_lsp` @ encoding.rs:65 | `== with !=` (UTF16 arm) | b | Same | same test |
| `Encoding::from_lsp` @ encoding.rs:67 | `== with !=` (UTF32 arm) | b | Same | same test |
| `From<&Encoding> for Encoding` @ encoding.rs:95 | `-> Self with Default::default()` | b | Public conversion impl (api-impl-into surface); delegation must not decay to the default encoding | same test, `Encoding::from(&enc)` asserts |
| `From<&PositionEncodingKind>` @ encoding.rs:101 | `-> Self with Default::default()` | b | Delegates to `from_lsp`; impl must not bypass it | same test via `.into()`/`From::from` |
| `From<PositionEncodingKind>` @ encoding.rs:107 | `-> Self with Default::default()` | b | Same, owned form | same test |
| `From<&Position> for Position` @ position.rs:50 | `-> Self with Default::default()` | b | Public conversion impl over the UTF-8-invariant `Position`; must project, not zero | W0 `from_impls_round_trip_asymmetric_positions` (new, position.rs tests): `Position { line: 3, col: 7 }` through every impl |
| `From<&LspPosition> for Position` @ position.rs:56 | `-> Self with Default::default()` | b | Same | same test |
| `From<&Position> for LspPosition` @ position.rs:68 | `-> Self with Default::default()` | b | Same | same test |
| `From<TsPoint> for Position` @ position.rs:106 | `-> Self with Default::default()` | b | Same (tree-sitter flavor) | same test, `#[cfg(feature = "tree-sitter")]` section |
| `From<&TsPoint> for Position` @ position.rs:113 | `-> Self with Default::default()` | b | Same | same test |
| `From<Position> for TsPoint` @ position.rs:120 | `-> Self with Default::default()` | b | Same | same test |
| `From<&Position> for TsPoint` @ position.rs:127 | `-> Self with Default::default()` | b | Same | same test |

Dupes note for Task 2: the two new "From impls project the inherent converter" tests are
deliberately parallel across modules (encoding.rs / position.rs); if `make dupes` fires, carry
one reasoned `.dupes-ignore.toml` entry (different types, same pin shape).

## 2. RangeExt — 37 rows (19 b, 18 c, 0 d)

Files: `src/text_utils/range_ext/{bytes,lsp,tree_sitter}.rs`.
Pattern across all three flavors: the existing tests kill 89+60+28 mutants but use ranges
starting at 0 and positions at range start — every survivor is exactly a non-zero-start,
mid-text, or boundary asymmetry. (c) rows: nextest never runs the trait doctests; the battery's
`cargo test --doc` legs do, and each traced assertion kills its mutant.

### bytes.rs — 25 rows (7 b, 18 c)

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `ByteRange::split_at` @ bytes.rs:12 | `- with +` (len calc) | b | Trait contract: `PositionOutOfRange` "if `at` lies beyond the end of the range" (mod.rs:78); `end + start` deflates the bound for any non-zero start | W0 `split_at_rejects_at_beyond_range_length` (bytes_tests.rs): range `5..10`, at 6 → `Err`; doctest's `0..7` ranges mask this |
| `ByteRange::sub` @ bytes.rs:31 | `- with +` (len calc) | b | Same contract via mod.rs:117 ("beyond the end of the range") | W0 `sub_bounds_are_relative_to_range_length` (bytes_tests.rs): range `5..10`, from/to 6 → `Err` |
| `ByteRange::sub` @ bytes.rs:32 | `> with ==` (`from > len`) | b | Boundary: `from == len` is legal (end-relative) and `from = len + 1` must still error | same test: from = len (5) → `Ok` — the `==` mutant errors on this legal boundary; from = 6 (to = 6) → `Err` pins the other side |
| `ByteRange::sub` @ bytes.rs:32 | `> with >=` (`from > len`) | b | Boundary: `>=` widens the bound to reject the legal end-relative `from == len` | same test: from = 5 → `Ok` (the `>=` mutant errors there) |
| `ByteRange::sub` @ bytes.rs:32 | `> with >=` (`to > len`) | b | Boundary: `to == len` selects the whole tail and must stay `Ok` | same test: to = 5 → `Ok` (the `>=` mutant errors there) |
| `ByteRange::sub_delimited` @ bytes.rs:46 | `- with +` (check_text_length arg) | b | `TextRangeMismatch` contract (mod.rs:143–146): text must be the exact range text; all doctest ranges start at 0 so `end + start == end` masks it | W0 `sub_delimited_requires_exact_text_length` (bytes_tests.rs): range `5..12`, 7-byte text → `Ok`; 6-byte → `Err(TextRangeMismatch)` |
| `ByteRange::sub_delimited` @ bytes.rs:51 | `== with !=` (`offset == 0`) | c | Doctest `("/two") == (None, Some(1..4))` fails when the empty-left case flips to `Some(0..0)` | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:56 | `>= with <` (last-char delim) | c | Doctest `("one/") == (Some(0..3), None)`: flipped branch yields `Some(4..4)` | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:56 | `+ with -` (same site) | c | Same `("one/")` assertion catches `offset - 1` taking the else branch | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:56 | `+ with *` (same site) | c | Same | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:59 | `+ with -` (`offset + 1` arg) | c | Doctest `(Some(0..3), Some(4..7))`: right part starting at 2 includes the delimiter | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:59 | `+ with *` (same arg) | c | Same assertion: right part starting at 3 includes the delimiter | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited` @ bytes.rs:62 | `delete !` (`!text.is_empty()`) | c | Doctest `("one") == (Some(0..3), None)` and `("") == (None, None)` both fail when the branches swap | doctest `RangeExt::sub_delimited` (mod.rs:129) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((None, None, None))` | c | Tri doctest `(Some(0..3), Some(4..7), Some(8..13))` fails | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((None, None, Some(Default)))` | c | Same expected triple fails | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((None, Some(Default), None))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((None, Some(Default), Some(Default)))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((Some(Default), None, None))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((Some(Default), None, Some(Default)))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((Some(Default), Some(Default), None))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:75 | `Ok((Some(Default), Some(Default), Some(Default)))` | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:82 | `- with +` (check_text_length arg) | b | Same exact-text-length contract; doctest ranges start at 0 so it masks `end + start` | W0 `sub_delimited_tri_requires_exact_text_length` (bytes_tests.rs): non-zero-start range, mismatched text → `Err` |
| `ByteRange::sub_delimited_tri` @ bytes.rs:82 | `- with /` (same site) | c | Doctest ranges start at 0: `end / 0` panics inside the doctest → fails it | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:93 | `+ with -` (`delim0_offset + 1`) | c | Remainder text off-by-one → `TextRangeMismatch` inside the doctest's `.expect` | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |
| `ByteRange::sub_delimited_tri` @ bytes.rs:93 | `+ with *` (same site) | c | Same | doctest `RangeExt::sub_delimited_tri` (mod.rs:160) |

### lsp.rs — 2 rows (2 b, 0 c)

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `LspRange::sub` @ lsp.rs:76 | `+ with -` (`start.line + from.line`) | b | mod.rs:110–121 contract: relative positions resolved against the range's start; trait doctest is byte-flavored only, existing lsp_tests use `from.line == 0` | W0 `sub_converts_relative_positions_across_lines` (lsp_tests.rs): start line 5, `from` (2, 3) → absolute line 7 |
| `LspRange::sub` @ lsp.rs:94 | `&& with \|\|` (from bounds check) | d | **revised (b) → (d) by owner decision 2026-09-09: accept as equivalent mutant.** The from-check is unreachable in the original — after the `from > to` → `StartAfterEnd` gate, the absolute mapping is monotone (line-major `Ord`; the `line == 0` character split cannot invert order), so `from_absolute ≤ to_absolute ≤ end`, and `from_absolute ≥ start` holds unconditionally. The De Morgan mutant fires only when both bounds are violated — jointly impossible. Evidence: monotonicity proof (Task 2 report, FLAGGED ROW), brute force 115,200 inputs (0 differences), reviewer re-derivation from source, and the full 268-test suite passing under the applied baseline patch. The check stays as a defense-in-depth invariant; this mutant is a permanent, documented survivor in every sweep | — |

### tree_sitter.rs — 10 rows (10 b, 0 c) — all gated `#[cfg(feature = "tree-sitter")]`

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `TsRange::split_at` @ tree_sitter.rs:12 | `- with +` (check_text_length arg) | b | Exact-text-length contract (mod.rs:183–186); existing tree_sitter_tests use `start_byte == 0` | W0 `split_at_validates_text_length_on_nonzero_start_ranges` (tree_sitter_tests.rs): `start_byte: 5` range, mismatched text → `Err` |
| `TsRange::split_at` @ tree_sitter.rs:16 | `== with !=` (`at.row == 0`) | b | Relative-position contract: row-0 columns offset from the range's start column | W0 `split_at_offsets_columns_by_range_start_column` : start_point.column 3, `at` (0, 1) → column 4 |
| `TsRange::sub` @ tree_sitter.rs:115 | `+ with -` (`start_point.row + from.row`) | b | Same relative-position contract, rows | W0 `sub_offsets_rows_by_range_start` : start row 2, `from` (1, 0) → row 3 |
| `TsRange::sub` @ tree_sitter.rs:141 | `&& with \|\|` (from-hit in scan loop) | b | Byte-offset resolution must find the actual `from` position, not the first iteration | W0 `sub_resolves_from_mid_text` : `from` mid-text → start_byte correct |
| `TsRange::sub` @ tree_sitter.rs:163 | `&& with \|\|` (from end-of-text check) | b | "a position that is nowhere in the text is out of range" (fn comment/mod.rs:117) | W0 `sub_rejects_end_of_text_position_mismatches` : from beyond text → `Err` |
| `TsRange::sub` @ tree_sitter.rs:163 | `== with !=` (row part) | b | Same end-of-text exactness | same test: exact-end position → `Ok`, one row off → `Err` |
| `TsRange`::sub @ tree_sitter.rs:163 | `== with !=` (col part) | b | Same | same test, column dimension |
| `TsRange::sub` @ tree_sitter.rs:170 | `== with !=` (to end-of-text check) | b | Same, for `to` | same test |
| `TsRange::sub_delimited_tri` @ tree_sitter.rs:275 | `- with +` (check_text_length arg) | b | Exact-text-length contract, non-zero start | W0 `sub_delimited_tri_validates_text_length_on_nonzero_start_ranges` |
| `TsRange::sub_delimited_tri` @ tree_sitter.rs:280 | `- with +` (`remainder.start_byte - self.start_byte`) | b | Remainder slicing must be relative to the range start | W0 `sub_delimited_tri_slices_remainder_from_nonzero_start` |

## 3. tree-sitter navigation — 12 rows (12 b, 0 c, 0 d)

File: `src/tree_sitter_utils.rs`. Module contract: "All conversions between tree-sitter and
LSP coordinates assume UTF-8 positions" (module docs) + per-fn traversal docs ("first child",
"first ancestor", "depth-first"). Only `ts_range_contains_ts_point` has a test today; the
converters and all `find_*` are unpinned. Tests are feature-gated by the module itself; the
json grammar is a dev-dependency (established fixture path).

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `ts_point_to_lsp_position` @ tree_sitter_utils.rs:24 | `-> LspPosition with Default::default()` | b | UTF-8 boundary conversion is the documented contract ("assumes ... UTF-8 encoding specifically") | W0 `ts_point_and_range_convert_to_lsp_coordinates` (inline tests): `p(1, 5)` → `{line: 1, character: 5}` |
| `ts_range_to_lsp_range` @ tree_sitter_utils.rs:38 | `-> LspRange with Default::default()` | b | Same contract, range form | same test, both endpoints |
| `ts_range_contains_lsp_position` @ tree_sitter_utils.rs:57 | `-> bool with true` | b | Documented inclusive-bounds check bridging LSP positions into tree-sitter space; callers rely on it for hit-testing | W0 `lsp_position_containment_matches_ts_point_containment` : inside → `true`, outside → `false` |
| `ts_range_contains_lsp_position` @ tree_sitter_utils.rs:57 | `-> bool with false` | b | Same | same test (inside arm kills it) |
| `find_child` @ tree_sitter_utils.rs:92 | `-> Option<Node> with None` | b | "Finds the first child node that matches the given predicate" — public navigation API for downstream servers | W0 `find_child_ancestor_descendant_traverse_as_documented` (inline tests, json grammar): direct-child match found, non-matching tree → `None` |
| `find_ancestor` @ tree_sitter_utils.rs:102 | `-> Option<Node> with None` | b | "Finds the first ancestor node..." | same test: grandparent matches → `Some`; root-only → `None` |
| `find_descendant` @ tree_sitter_utils.rs:122 | `-> Option<Node> with None` | b | "search descendants" contract (doc comment; traversal order is breadth-first — the doc's original "depth-first" wording was a doc/code discrepancy fixed 2026-09-09, owner-approved) | same test: first-match membership asserted (BFS and DFS preorder agree on every scenario's first match) |
| `find_nearest` @ tree_sitter_utils.rs:150 | `-> Option<Node> with None` | b | Documented 1–4 priority order (node → child → descendant → ancestor) | W0 `find_nearest_prefers_node_then_child_then_descendant_then_ancestor` |
| `find_nearest_inner` @ tree_sitter_utils.rs:164 | `-> Option<Node> with None` | b | The whole nearest-node resolution; deleting it silently answers `None` to every downstream navigation query | same test (each priority arm returns `Some`) |
| `find_nearest_inner` @ tree_sitter_utils.rs:164 | `&& with \|\|` @178 (child filter) | b | A position-containing child that fails the predicate must not shadow a matching descendant | same test: child contains position but fails predicate; descendant passes → descendant returned |
| `find_nearest_inner` @ tree_sitter_utils.rs:164 | `&& with \|\|` @182 (descendant filter) | b | Same for the descendant filter | same test |
| `find_nearest_inner` @ tree_sitter_utils.rs:164 | `&& with \|\|` @189 (ancestor filter) | b | Ancestor must satisfy position AND predicate | same test: inner ancestor contains position but fails predicate; outer passes → outer returned |

## 4. Document accessors & reader — 22 rows (12 b, 0 c, 10 d)

File: `src/documents/document.rs` (19 baseline + 3 real timeouts).
Disposition split: pure projections of the construction-immutable snapshot (`DocumentMeta`,
matcher handle, tree/lang `Option`s) are (d) — the exact class D6's example blesses
("accessor of a construction-immutable handle; no behavioral contract exists"); computed or
navigation behavior is (b).

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `Document::text_bytes` @ document.rs:166 | `-> Vec<u8> with vec![]` | b | Documented: "Returns the full text of the document, as bytes" | W0 `text_bytes_returns_the_document_bytes` (inline tests): multi-byte doc → exact bytes |
| `Document::text_bytes` @ document.rs:166 | `-> Vec<u8> with vec![0]` | b | Same | same test |
| `Document::text_bytes` @ document.rs:166 | `-> Vec<u8> with vec![1]` | b | Same | same test |
| `Document::language` @ document.rs:181 | `-> &str with ""` | d | Accessor of `DocumentMeta.language`, construction-immutable by design (struct doc: "Construction-immutable identity"); no invalid state to remove, pin would restate the projection | — |
| `Document::language` @ document.rs:181 | `-> &str with "xyzzy"` | d | Same | — |
| `Document::matched_name` @ document.rs:191 | `-> Option<&str> with None` | d | Accessor projecting the matcher handle attached at match time (matcher.rs:53–55 contract lives on `DocumentMatcher::name`, tested there) | — |
| `Document::matched_name` @ document.rs:191 | `-> Option<&str> with Some("")` | d | Same | — |
| `Document::matched_name` @ document.rs:191 | `-> Option<&str> with Some("xyzzy")` | d | Same | — |
| `Document::has_syntax_language` @ document.rs:200 | `-> bool with true` | d | Projection of `tree_sitter_lang.is_some()`; grammar attachment is pinned at the matcher layer (`lang_grammar_rides_along_with_the_matcher`) | — |
| `Document::has_syntax_language` @ document.rs:200 | `-> bool with false` | d | Same | — |
| `Document::has_syntax_tree` @ document.rs:206 | `-> bool with true` | d | Projection of `tree_sitter_tree.is_some()`; tree/text coherence is pinned by cluster 11's didChange pins, not by the flag itself | — |
| `Document::has_syntax_tree` @ document.rs:206 | `-> bool with false` | d | Same | — |
| `Document::node_text` @ document.rs:216 | `-> String with String::new()` | b | Documented: "Returns the UTF-8 text of a Node" (with `# Panics` bounds contract) — the query-consumer primitive | W0 `node_text_returns_the_node_slice` (inline tests, gated): node at a known position → exact slice |
| `Document::node_text` @ document.rs:216 | `-> String with "xyzzy".into()` | b | Same | same test |
| `Document::node_at_root` @ document.rs:222 | `-> Option<Node> with None` | b | "Returns a Node at the root of the syntax tree, if one exists" — navigation entry point | W0 `node_accessors_resolve_positions_in_parsed_documents` (gated): parsed doc → `Some`; grammarless → `None` |
| `Document::node_at_position` @ document.rs:231 | `-> Option<Node> with None` | b | "Returns a Node at the given LSP position, if one exists" — the hover/completion primitive for downstream servers | same test: valid position → `Some`; **note (owner-approved 2026-09-09):** the original cell's "out-of-tree → `None`" arm was wrong — tree-sitter clamps points past the tree extent to `Some`(document); `None` is reachable only through a grammarless root. The test pins the clamp (valid positions kill the `→ None` mutant); the None arm is covered by the grammarless fixture |
| `Document::node_at_position_named` @ document.rs:239 | `-> Option<Node> with None` | b | Documented named-only variant | same test: named node at an anonymous-token position — same clamp note as above |
| `AsRef<Rope> for Document` @ document.rs:327 | `-> &Rope with Box::leak(...)` | d | One-line projection of the snapshot rope ("cheap handle" struct contract); the projection is the whole behavior | — |
| `DocumentReader::read` @ document.rs:342 | `-> Result<usize> with Ok(1)` | b | Violates `std::io::Read`'s `Ok(0)`-means-EOF contract → callers loop forever (observed: 20 s sweep timeout, re-verified in isolation) | W0: reshape `reader_preserves_unread_chunk_bytes` into a bounded read loop — EOF must be reached, bytes asserted |
| `DocumentReader::read` @ document.rs:348 | `< with <=` (`written < buf.len()`) | b | Off-by-one keeps the fill loop spinning at `written == buf.len()` (observed hang, re-verified) | same bounded loop: 1-byte buffer reads terminate with `b"hello"` |
| `DocumentReader::read` @ document.rs:363 | `+= with *=` (`current_offset += len`) | b | Offset stuck at 0 re-reads the first bytes forever (observed hang, re-verified) | same bounded loop: byte sequence assertion fails the repeat |
| `DocumentReader::read` @ document.rs:359 | `- with +` (`buf.len() - written`) | b | Mutated `min` bound overruns the buffer on the second pass of a multi-chunk fill (panic = detection) | W0 `read_fills_multi_chunk_buffers_across_chunks` (inline tests): 8-byte read over a two-chunk rope → `Ok(8)`, exact bytes |

## 5. workspace/diagnostics state machine — 17 rows (17 b, 0 c, 0 d)

File: `src/workspace/diagnostics.rs`. Contract (structure.md): "the setting is read from
client configuration (`initializationOptions`, `workspace/configuration` requests,
`didChangeConfiguration`, dynamic registration — each gated on client capabilities". The
gate functions and generation counter are directly constructible atomics (W0); the
fire-and-forget client requests are observable only on the wire.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `can_request_configuration` @ diagnostics.rs:82 | `-> bool with true` | b | Capability gating is the documented contract; `true` interrogates clients that never opted into `workspace.configuration` | W0 `request_configuration_requires_client_capability_and_setting` (inline tests): flag off → `false`; flag on + Configurable → `true` |
| `can_request_configuration` @ diagnostics.rs:82 | `&& with \|\|` | b | A client without the capability but with any `Configurable` setting must not pass | same test: flag off + Configurable → `false` |
| `can_register_configuration` @ diagnostics.rs:86 | `-> bool with false` | b | Dynamic registration is a named mechanism of the contract | W0 `register_configuration_requires_dynamic_registration_support`: flag on + Configurable → `true` |
| `can_refresh` @ diagnostics.rs:93 | `-> bool with true` | b | `workspace/diagnostic/refresh` may only be sent to clients that support it (refresh_support gate) | W0 `refresh_gate_tracks_client_refresh_support`: flag off → `false`, on → `true` |
| `can_refresh` @ diagnostics.rs:93 | `-> bool with false` | b | Refresh must still fire for supporting clients | same test (on arm) |
| `next_generation` @ diagnostics.rs:125 | `-> u64 with 0` | b | Generation staleness guard: responses captured against an older generation must be dropped (structure.md staleness design) | W0 `next_generation_is_monotonic`: 1 then 2; stale compare drops |
| `next_generation` @ diagnostics.rs:125 | `-> u64 with 1` | b | Same | same test |
| `next_generation` @ diagnostics.rs:125 | `+ with *` | b | `fetch_add(1) + 1` → `* 1` returns the pre-increment value; breaks monotonicity | same test |
| `current_generation` @ diagnostics.rs:129 | `-> u64 with 0` | b | A fresh response (generation == current) must not be dropped as stale | W0 `stale_generation_drops_the_response`: capture → advance → compare drops; capture → compare applies |
| `current_generation` @ diagnostics.rs:129 | `-> u64 with 1` | b | A stale response (generation < current) must not apply | same test |
| `disable_workspace_diagnostics` @ diagnostics.rs:168 | `with ()` | b | structure.md: exposure follows `ServerOptions::with_workspace_diagnostics` — `Disabled` must advertise `workspaceDiagnostics: false` even if the implementor set a diagnostic provider | W0 `disabled_options_force_workspace_diagnostics_capability_off` (inline tests via `configure_capabilities`): provider Some + Disabled → flag false |
| `register_configuration` @ diagnostics.rs:261 | `with ()` | b | Dynamic registration mechanism (contract sentence above); observable only as a client-bound request | wire `initialized_registers_did_change_configuration_when_supported` (src/server/tests): client with dynamic registration receives the registration request |
| `request_configuration` @ diagnostics.rs:288 | `!= with ==` (staleness check) | b | Inverts the staleness guard: applies stale, drops fresh | wire `configuration_reply_applies_only_if_generation_current`: first reply applies, superseded reply ignored |
| `apply_enabled` @ diagnostics.rs:322 | `&& with \|\|` | b | Refresh must fire only when the flag actually flipped AND diagnostics are supported | wire `refresh_fires_only_on_change_and_only_when_supported`: same-value set → no refresh |
| `refresh_diagnostics` @ diagnostics.rs:330 | `with ()` | b | Refresh mechanism: clients are told to re-pull after an enable change | wire `refresh_fires_only_on_change_and_only_when_supported` (fire arm) |
| `refresh_diagnostics` @ diagnostics.rs:330 | `delete !` | b | Inverted gate: refresh sent to non-supporting clients, withheld from supporting ones | same wire test (both arms) |
| `push_related_reports` @ diagnostics.rs:546 | `with ()` | b | structure.md: workspace diagnostics "merges related-document reports" | W0 `related_reports_merge_with_replace_false` (inline tests): sink gains related entries, `replace == false` |

## 6. oneshot — 11 rows (9 b, 0 c, 2 d)

Files: `src/oneshot/{server,workspace_diagnostics}.rs`. Contract (structure.md): oneshot
"runs a `Server` over files on disk with no LSP client or transport — CLI-style batch
diagnostics". The `workspace_diagnostics` doctest drives the full flow but its fixture is
pure ASCII and never calls `is_empty`, which is exactly why the rows below survive it.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `initialize_params` @ oneshot/server.rs:89 | `delete field process_id` | d | No consumer on the clientless path: `process_id` is transport-layer data (async-lsp `ClientProcessMonitorLayer`); LSP shows zero readers in `src/server/` — behaviorally inert here | — |
| `initialize_params` @ oneshot/server.rs:90 | `delete field capabilities` | b | Deletes the UTF-8 preference → negotiation falls back to the LSP default UTF-16 → CLI report columns silently change unit for non-ASCII text; the crate invariant is UTF-8 | W0 `oneshot_reports_byte_offsets_for_non_ascii_documents` (oneshot tests): doc with 🙂 before the diagnostic column → character equals the byte offset |
| `initialize_params` @ oneshot/server.rs:97 | `delete field workspace_folders` | d | Inert on this path: initialize's folder tracking feeds transport-side workspace roots (refresh / workspace-diagnostic requests); the oneshot walker drives files directly and never reads them | — |
| `initialize_params` @ oneshot/server.rs:91 | `delete field general` | b | Same UTF-8 negotiation contract as :90 | same test |
| `initialize_params` @ oneshot/server.rs:92 | `delete field position_encodings` | b | Same | same test |
| `WorkspaceDiagnosticReport::is_empty` @ oneshot/workspace_diagnostics.rs:70 | `-> bool with true` | b | Documented: "true if no document reported diagnostics" — a CLI exit-code style predicate | W0 `is_empty_reflects_document_and_report_contents` (inline tests): empty + non-empty documents |
| `WorkspaceDiagnosticReport::is_empty` @ oneshot/workspace_diagnostics.rs:70 | `-> bool with false` | b | Same | same test (empty arm) |
| `DocumentDiagnostics::is_empty` @ oneshot/workspace_diagnostics.rs:89 | `-> bool with true` | b | Documented per-document predicate | same test |
| `DocumentDiagnostics::is_empty` @ oneshot/workspace_diagnostics.rs:89 | `-> bool with false` | b | Same | same test |
| `diagnostics_from_report_kind` @ oneshot/workspace_diagnostics.rs:285 | `-> &[Diagnostic] with Vec::leak(vec![])` | b | `diagnostics()` must surface Full-kind items (doctest only pins the main-items leg, related reports unexercised) | W0 `diagnostics_collects_full_and_unchanged_kinds`: related Full → items; Unchanged → empty |
| `diagnostics_from_report_kind` @ oneshot/workspace_diagnostics.rs:285 | `-> &[Diagnostic] with Vec::leak(vec![Default::default()])` | b | Same | same test (Unchanged arm) |

## 7. server defaults & options — 7 rows (3 b, 0 c, 4 d)

Files: `src/server/{server_trait,options}.rs`. Trait-row expectation per plan Task 5:
"server-trait default rows expected (d) trait-mandated".

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `ConfigurationKey::item` @ options.rs:144 | `-> ConfigurationItem with Default::default()` | b | The configuration request must name the section (structure.md: the setting is read via `workspace/configuration`); a section-less item reads the wrong subtree | W0 `configuration_item_carries_the_section` (options.rs tests): `item().section == Some(section)` |
| `ConfigurationKey::section` @ options.rs:166 | `-> &str with ""` | b | Registration's `register_options.section` must match the key (diagnostics.rs:277) | same test |
| `ConfigurationKey::section` @ options.rs:166 | `-> &str with "xyzzy"` | b | Same | same test |
| `Server::server_info` @ server_trait.rs:28 | `-> Option<ServerInfo> with Some(Default::default())` | d | Trait-mandated default: module doc "All of the LSP methods in this trait are optional"; the default IS the contract ("None = no info"), a pin would restate the body | — |
| `Server::server_options` @ server_trait.rs:33 | `-> ServerOptions with Default::default()` | d | Behaviorally equivalent mutant: the body already is `ServerOptions::default()` | — |
| `Server::server_capabilities` @ server_trait.rs:43 | `-> Option<ServerCapabilities> with Some(Default::default())` | d | Trait-mandated default: "Returning `None` advertises only the crate's defaults" (fn doc) | — |
| `Server::server_document_matchers` @ server_trait.rs:51 | `-> Vec<DocumentMatcher> with vec![Default::default()]` | d | Trait-mandated default AND behaviorally equivalent: a default matcher has no globs/lang strings, and `DocumentMatchers::new` skips it (matcher.rs:171–197) | — |

## 8. state (mod) — 3 rows (3 b, 0 c, 0 d)

File: `src/server/state/mod.rs`.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `set_workspace_diagnostics_enabled` @ state/mod.rs:148 | `&& with \|\|` (`changed && !enabled`) | b | structure.md: workspace documents back workspace diagnostics — they are dropped when diagnostics are *disabled*, never when enabled; mutated, enabling purges them | W0 `enabling_workspace_diagnostics_keeps_workspace_documents` (state/tests.rs): enable → workspace docs kept; disable → purged |
| `get_position_encoding` @ state/mod.rs:154 | `-> Encoding with Default::default()` | b | The negotiated encoding drives every response conversion (the UTF-8 invariant plumbing); tests all run under the UTF-16 default so the mutant is masked today | W0 `position_encoding_setter_updates_negotiated_state`: set UTF8 → get UTF8 |
| `set_position_encoding` @ state/mod.rs:177 | `with ()` | b | Same; a dropped setter freezes negotiation at the default | same test |

## 9. lsp_requests conversions — 10 rows (10 b, 0 c, 0 d)

Files: `src/lsp_requests/{code_action,code_action_resolve,conversion}.rs`. Contract
(structure.md, the Request pattern): `modify_params` converts client→UTF-8 before the handler,
`modify_response` UTF-8→client after. Existing `conversion_tests!` rows cover the standard
hooks; every survivor is a custom hook or semantic-tokens helper the rows don't reach.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `code_action::convert_response` @ code_action.rs:26 | `with ()` | b | Outgoing hook of textDocument/codeAction: action diagnostics + edits must reach the client in the negotiated encoding; the existing W0 test pins only the incoming hook | W0 `code_action_outgoing_hook_converts_diagnostics_and_edits` (code_action.rs tests): action with diagnostic + edit → outgoing ranges converted |
| `code_action_resolve::convert_params` @ code_action_resolve.rs:20 | `with ()` | b | Resolve receives a client-encoded action; incoming conversion is the Request contract | W0 `code_action_resolve_hooks_convert_in_both_directions` (code_action_resolve.rs tests): incoming + outgoing over the UTF-16 fixture |
| `code_action_resolve::convert_response` @ code_action_resolve.rs:26 | `with ()` | b | Outgoing half of the same contract | same test |
| `code_action_resolve::convert_code_action` @ code_action_resolve.rs:32 | `with ()` | b | Shared engine of both hooks | same test (both directions) |
| `modify_outgoing_location_link` @ conversion.rs:270 | `with ()` | b | LocationLink ranges (origin selection, target range/selection) must convert; sole caller is the locations outgoing path (conversion.rs:315) | W0 `location_link_outgoing_converts_origin_and_target_ranges` (conversion.rs tests): link over the UTF-16 fixture |
| `convert_seeded_token_stream` @ conversion.rs:766 | `== with !=` (`delta_line == 0`) | b | Semantic-token delta encoding contract: same-line tokens accumulate, new-line tokens restart; flipped, the frames swap | W0 `seeded_token_stream_recomputes_deltas_across_lines` (conversion.rs tests): stream with a line-crossing token, asymmetric columns |
| `convert_seeded_token_stream` @ conversion.rs:782 | `== with !=` (target-line check) | b | Outgoing deltas must be relative to the previous *target* position on the same line | same test |
| `absolute_position` @ conversion.rs:968 | `-> Position with Default::default()` | b | Delta-fold seed for semanticTokens/full/delta: the inserted stream re-anchors at the cached prefix's end | W0 `absolute_position_folds_deltas`: `[(1,3),(0,5)]` → `{1, 8}`; empty → origin — **corrected 2026-09-09:** the original example `[(1,0),(0,5)]` → `{1, 5}` is a fixpoint under the `@974 == with !=` mutant (both branches agree); the discriminating input was required to make this row's kill real (B5 report note 1, reviewer-confirmed) |
| `absolute_position` @ conversion.rs:974 | `== with !=` (`delta_line == 0`) | b | Same fold's line-branch selection | same test (multi-line prefix) |
| `splice_semantic_tokens_cache` @ conversion.rs:1018 | `+ with *` (`start + delete_count`) | b | Cache coherence: edits splice by exact flat-array span (structure.md: the cache holds the server's UTF-8 data the client's result_id refers to) | W0 `splice_applies_edit_delete_counts_at_nonzero_offsets`: start 5, delete 3 → exact spliced stream |

## 10. matcher — 3 rows (3 b, 0 c, 0 d)

File: `src/documents/matcher.rs`. `lang_strings()` is `pub(crate)` with two production
callers (state/workspace.rs:173, oneshot/workspace_diagnostics.rs:240); both mask constant
returns with a name fallback, so only a direct W0 pin sees the mutation. Not wire-visible.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `DocumentMatcher::lang_strings` @ matcher.rs:112 | `-> &[String] with Vec::leak(vec![])` | b | Field contract: "Strings to match documents based on their language identifiers"; the getter must project the configured identifiers | W0 `lang_strings_returns_configured_identifiers` (matcher.rs tests): configured `["json"]` → `["json"]` |
| `DocumentMatcher::lang_strings` @ matcher.rs:112 | `-> &[String] with Vec::leak(vec![String::new()])` | b | Same | same test |
| `DocumentMatcher::lang_strings` @ matcher.rs:112 | `-> &[String] with Vec::leak(vec!["xyzzy"])` | b | Same | same test |

## 11. state documents — 14 rows (14 b, 0 c, 0 d)

File: `src/server/state/documents.rs` (11 baseline + 3 verified artifacts, marked).
Contract: document.rs struct docs — "the tree will be parsed using the initial contents, and
incrementally updated thereafter, transparently"; `finalize_edited_tree` doc — "so the
installed generation cannot diverge from the working text"; `query()`'s documented invariant
("every mutation writes text and tree together"). All pins are feature-gated W0 tests in
`state/tests.rs` asserting tree/text coherence after `didChange` (via `query`/node accessors).

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `handle_document_change` @ documents.rs:207 | `&& with \|\|` (finalize gate) | d | **revised (b) → (d) by owner decision 2026-09-10: equivalent mutant.** The gate mis-fire is absorbed: on success∧¬edited every change was a full-text replace whose `replace_full_text` already installed a fresh tree, so the extra finalize re-parses consistent text with a consistent hint; on failure∧edited the recovery path (`recover_failed_incremental_update`) unconditionally re-installs a tree — the finalized result never survives the call. Empirical: full all-features suite passes under the applied diff; residual divergence is a transient window observable only by racing a concurrent request (no deterministic test can reach it; lsp.rs:94 precedent) | — |
| `handle_document_change` @ documents.rs:207 | `delete !` — **artifact** (Miri-masked timeout; MISSED in isolated re-run) | b | `delete !` inverts: finalize only on failure → successful edits leave a stale tree | same test |
| `finalize_edited_tree` @ documents.rs:465 | `with ()` | b | The re-parse is what keeps the installed generation equal to the working text | same test |
| `parse_rope` @ documents.rs:486 | `== with !=` (EOF check) | b | Flipped, the chunk callback answers "" for every in-range offset → empty parse → stale/empty tree | same test (post-edit query non-empty) |
| `parse_rope` @ documents.rs:490 | `- with +` (`offset - chunk_start`) | b | Multi-chunk ropes slice the wrong region; every existing fixture is a single rope chunk | W0 `parse_rope_serves_multi_chunk_ropes` (gated): doc large enough for multiple rope chunks → correct tree |
| `replace_full_text` @ documents.rs:519 | `with ()` (non-tree-sitter flavor) | b | A range-less didChange must replace the whole text on the no-default leg too | W0 `full_text_did_change_replaces_document_text` (state/tests.rs; runs in both feature legs) |
| `tree_sitter_edit` @ documents.rs:553 | `-> Option<InputEdit> with None` | b | `None` skips `tree.edit` → stale tree while text advances | W0 `tree_sitter_edit_computes_new_end_from_inserted_text` (gated): asymmetric edits → tree coherent |
| `tree_sitter_edit` @ documents.rs:558 | `+ with -` (`start_byte + text.len()`) | b | InputEdit's new_end_byte must count the inserted bytes | same test (non-zero start + non-empty insert) |
| `tree_sitter_edit` @ documents.rs:558 | `+ with *` (same site) | b | Same | same test |
| `tree_sitter_edit` @ documents.rs:571 | `== with !=` (`ch == '\n'`) | d | **revised (b) → (d) by owner decision 2026-09-10: equivalent mutant.** The mutated fold feeds only `InputEdit.new_end_position`; its sole consumer is `Tree::edit`, whose edited tree serves only as the reuse hint for the immediate finalize reparse — nothing of the hint's points survives into the installed tree. Empirical: 100-pair reuse probe shows a byte-identical tree dump under the mutant while the byte-arithmetic mutant `:558 +→*` visibly corrupts (14 pairs + ERROR), which is also why `:558` stays killable. Note: the `+→-` siblings' kills ride debug-profile usize underflow panics | — |
| `tree_sitter_edit` @ documents.rs:572 | `+ with -` (`row + 1`) | b | Row advance per newline | same test |
| `tree_sitter_edit` @ documents.rs:572 | `+ with *` — **artifact** | d | Same new_end_position normalization as the :571 revision (owner decision 2026-09-10, equivalent mutant; byte-identical probe) | — |
| `tree_sitter_edit` @ documents.rs:574 | `+ with -` (`col_bytes + len_utf8`) | b | Column accumulation in bytes (UTF-8 invariant) | same test (insert with 🙂) |
| `tree_sitter_edit` @ documents.rs:574 | `+ with *` — **artifact** | d | Same new_end_position normalization as the :571 revision (owner decision 2026-09-10, equivalent mutant; byte-identical probe) | — |

## 12. state workspace — 6 rows (6 b, 0 c, 0 d)

File: `src/server/state/workspace.rs` (2 baseline + 4 verified artifacts, marked).
Contract (structure.md): refresh "walks roots, loads matching files as `Workspace`
documents"; retention keeps Open documents and everything outside the scanned roots;
folder removal drops that folder's workspace snapshots.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `document_urls` @ workspace.rs:59 | `-> Vec<Url> with vec![]` | b | Early-return paths of `refresh_workspace_documents` must still report currently tracked documents, not an empty batch | W0 `refresh_without_roots_reports_tracked_documents` (state/tests.rs): open doc + disabled/no-roots → url present |
| `remove_workspace_documents_in_roots` @ workspace.rs:137 | `== with !=` (origin check) | b | Retention predicate: Open kept, workspace snapshots inside removed roots dropped — mutated, fully inverted | W0 `removing_folder_roots_keeps_open_drops_workspace_documents`: open doc survives, workspace doc dropped |
| `refresh_workspace_documents` @ workspace.rs:118 | `\|\| with &&` — **artifact** | b | The freshly loaded set must be retained; mutated, refreshed workspace docs are dropped right after loading | W0 `refresh_retains_freshly_loaded_workspace_documents` |
| `remove_workspace_documents_in_roots` @ workspace.rs:132 | `with ()` — **artifact** | b | Same retention contract; body deletion keeps stale snapshots forever | same test as :137 row |
| `remove_workspace_documents_in_roots` @ workspace.rs:137 | `\|\| with &&` — **artifact** | b | Conjunctive retention drops Open documents inside the roots | same test (open-doc-survives arm) |
| `remove_workspace_documents_in_roots` @ workspace.rs:137 | `delete !` — **artifact** | b | Inverts root membership: workspace docs outside the removed roots get dropped | same test |

## 13. with_state initialize — 3 rows (3 b, 0 c, 0 d)

File: `src/server/with_state/initialize.rs` (1 baseline + 2 verified artifacts, marked).
Contract: the wrapper owns document-sync advertisement (initialize step 4: INCREMENTAL,
open_close, save) — compliant clients stop sending didOpen/didChange/didSave if any field
goes missing, silently disabling the whole sync layer. Direct wrapper drive (oneshot-style,
closed socket) observes the advertised capability without the wire tier.

| function@file | mutation | disp | reason | covering test / doctest |
|---|---|---|---|---|
| `initialize` @ initialize.rs:66 | `delete field save` | b | Advertised save capability must match the wrapper's actual notification handling | W0 `initialize_advertises_incremental_sync_with_open_close_and_save` (with_state/tests.rs): drive `initialize`, assert `textDocumentSync` = {change: Incremental, openClose: true, save.includeText: true} |
| `initialize` @ initialize.rs:64 | `delete field change` — **artifact** | b | `change: None` demotes sync to the LSP default (none) — clients stop sending incremental edits | same test (change arm) |
| `initialize` @ initialize.rs:65 | `delete field open_close` — **artifact** | b | `openClose: None` disables open/close notifications for compliant clients | same test (openClose arm) |

---

## Cluster subtotals

| # | cluster | rows | b | c | d |
|---|---|---|---|---|---|
| 1 | text_utils conversions | 16 | 14 | 2 | 0 |
| 2 | RangeExt | 37 | 19 | 18 | 0 |
| 3 | tree-sitter navigation | 12 | 12 | 0 | 0 |
| 4 | Document accessors & reader | 22 | 12 | 0 | 10 |
| 5 | workspace/diagnostics state machine | 17 | 17 | 0 | 0 |
| 6 | oneshot | 11 | 9 | 0 | 2 |
| 7 | server defaults & options | 7 | 3 | 0 | 4 |
| 8 | state (mod) | 3 | 3 | 0 | 0 |
| 9 | lsp_requests conversions | 10 | 10 | 0 | 0 |
| 10 | matcher | 3 | 3 | 0 | 0 |
| 11 | state documents | 14 | 14 | 0 | 0 |
| 12 | state workspace | 6 | 6 | 0 | 0 |
| 13 | with_state initialize | 3 | 3 | 0 | 0 |
| | **total** | **161** | **125** | **20** | **16** |

Of the 125 (b) rows: 121 W0, 4 wire (diagnostics.rs:261/288/322/330 — the fire-and-forget
client requests, observable only as client-bound messages). Of the 20 (c) rows: 2 encoding +
18 range_ext, all verified against both doctest legs.

## Self-review (Task 1 Step 3)

- 161 rows present: 149 baseline misses + 3 re-verified real timeouts + 9 artifacts (flagged,
  owner may strike → 152).
- Every row has exactly one disposition and a contract-cited reason; (d) rows quote or cite the
  governing sentence (module/fn docs, structure.md, trait-mandated defaults, D6's own accessor
  example) — none cites tool convenience.
- No (a) rows; the criterion and the two rejected candidates are recorded above.
- Cluster subtotals sum to 161; core inventory sums to 152.
- (c) doctests verified present and passing on `--all-features` and `--no-default-features`
  doc legs (2026-09-09); each traced assertion kills its mutant.
- Implementation distribution for Tasks 2–5: cluster 1–2 → Task 2; 3–4 → Task 3; 5, 8, 11–13 →
  Task 4; 6–7, 9–10 → Task 5 (adjusting for the plan's cluster grouping; the table is the
  contract, not the task boundaries).

---

## Final sweep outcome (Task 6, 2026-09-10) — close-out

Full `make mutants` on `feature/nextest` @ `1a3824f`, run alone (battery green first, nothing
concurrent, ~2 h). The mutant set is byte-identical to the baseline (859 total = 620 tested +
239 unviable; tested and unviable lists diff clean), so every comparison below is per-mutant
exact, not per-line approximate.

**Sweep: 574 caught · 43 missed · 3 timeouts · 239 unviable**
(baseline: 459 caught · 149 missed · 12 timeouts · 239 unviable; survivors 161 → 46).

**Acceptance verdict: END STATE NOT ACHIEVED.** Zero live (b) rows in clusters 1–10, all 17
(d) rows survived as dispositioned — but **22 of the 23 (b) rows in clusters 11–13 are still
live** (20 missed + 2 timeouts). Root cause: Task 4's issued brief covered only clusters 5 + 8
(~20 rows); the plan's "5, 8, 11–13 → Task 4" distribution never reached an implementation
task, so no covering test exists in `src/` for any of the 23 cluster-11–13 rows (the eight
table-proposed test names are all absent). The one kill in those clusters came from outside:

- `parse_rope` @ documents.rs:486 (`== with !=`, EOF check) — **killed** by Task 3's
  `node_accessors_resolve_positions_in_parsed_documents` /
  `node_text_returns_the_node_slice` (scenario log names them); no cluster-11-targeted test
  exists.

The 22 live rows (fix-loop scope; baseline `missed.txt`/`timeout.txt` identities preserved):

- `handle_document_change` :207 (`&& with ||`; `delete !` — **timeout**)
- `finalize_edited_tree` :465 `with ()`; `parse_rope` :490 (`- with +`);
  `replace_full_text` :519 `with ()`; `tree_sitter_edit` :553 `→ None`, :558 (`+ with -`,
  `+ with *`), :571 (`== with !=` — **timeout**), :572 (`+ with -`, `+ with *`),
  :574 (`+ with -`, `+ with *`) — all `src/server/state/documents.rs`
- `document_urls` :59; `refresh_workspace_documents` :118 (`|| with &&`);
  `remove_workspace_documents_in_roots` :132 `with ()`, :137 (`== with !=`, `|| with &&`,
  `delete !`) — all `src/server/state/workspace.rs`
- `initialize` :64 (`delete field change` — **timeout**), :65 (`delete field open_close`),
  :66 (`delete field save`) — all `src/server/with_state/initialize.rs`

Per-cluster outcome (killed = observed in the fresh sweep's `caught.txt`):

| # | cluster | (b) rows | (b) killed | (c) survived / 20 | (d) survived / 17 |
|---|---|---|---|---|---|
| 1 | text_utils conversions | 14 | 14 ✓ | 2 / 2 | 0 |
| 2 | RangeExt | 18 | 18 ✓ (lsp.rs:94 is (d)) | 5 / 18 | 1 / 1 |
| 3 | tree-sitter navigation | 12 | 12 ✓ | — | — |
| 4 | Document accessors & reader | 12 | 12 ✓ — incl. all 3 former `DocumentReader::read` real timeouts, now caught by the bounded read-loop tests | — | 10 / 10 |
| 5 | workspace/diagnostics state machine | 17 | 17 ✓ | — | — |
| 6 | oneshot | 9 | 9 ✓ | — | 2 / 2 |
| 7 | server defaults & options | 3 | 3 ✓ | — | 4 / 4 |
| 8 | state (mod) | 3 | 3 ✓ | — | — |
| 9 | lsp_requests conversions | 10 | 10 ✓ | — | — |
| 10 | matcher | 3 | 3 ✓ | — | — |
| 11 | state documents | 14 | **1** (:486, incidental) | — | — |
| 12 | state workspace | 6 | **0** | — | — |
| 13 | with_state initialize | 3 | **0** | — | — |
| | **total** | **124** | **102** | **7** | **17** |

Two observations beyond the verdict:

- **13 (c) rows were killed by the new W0 tests** — the (c) disposition ("survives nextest,
  killed by the doctest only") is obsolete for them: bytes.rs :51 (`== with !=`),
  :56 (`>= with <`), :59 ×2, :75 ×8, :82 (`- with /`). Scenario logs name Task 2's
  `sub_delimited_requires_exact_text_length` /
  `sub_delimited_tri_requires_exact_text_length` fixtures as killers (non-zero-start ranges
  assert exact part ranges, pinning what the doctests pinned). The 7 remaining (c)
  survivors are exactly: `Encoding::as_str` ×2 + bytes.rs :56 (`+ with -`, `+ with *`),
  :62 (`delete !`), :93 ×2 — the documented nextest/doctest blind spot, as dispositioned.
- The fresh sweep's only 3 timeouts are the anomaly rows flagged above (:207, :571,
  initialize :64) — two of them baseline artifacts that timed out again under full-sweep
  load instead of completing as they did in the isolated re-runs. No legitimate row timed
  out; the baseline's 12-timeout column is fully resolved (3 caught by Task 3's bounded
  read-loop tests, the rest accounted here).

Raw evidence: `/tmp/b6_mutants_full.log`, `mutants.out/` (this sweep's scratch);
diff inputs preserved in the Task 6 report
(`.superpowers/sdd/task-b6-report.md`).

## Final sweep outcome — acceptance run (2026-09-10, owner-executed, cycle close)

The acceptance sweep (full `make mutants`, nothing concurrent; the controller-side run
was killed by system sleep and re-executed by the owner in their terminal) closed the
cycle:

**859 mutants: 592 caught · 28 missed · 0 timeouts · 239 unviable.**
Fix-loop B7 (clusters 11–13) landed between the Task 6 reconciliation and this run; its
18 kills plus the 4 owner-ratified (b)→(d) revisions are reflected below. Survivors:
161 (baseline) → 46 (Task 6 sweep) → **28 (final)**, and all 28 map to table rows:

| class | count | rows |
|---|---|---|
| (c) — nextest/doctest blind spot, as dispositioned | 7 | `Encoding::as_str` @ encoding.rs:49 ×2; bytes.rs `sub_delimited` :56 ×2, :62; `sub_delimited_tri` :93 ×2 |
| (d) — accepted / ratified equivalent | 20 | Document accessors ×9 (`language` ×2, `matched_name` ×3, `has_syntax_language` ×2, `has_syntax_tree` ×2); oneshot `process_id`, `workspace_folders`; `Server` trait defaults ×4 (`server_info`, `server_options`, `server_capabilities`, `server_document_matchers`); `LspRange::sub` @ lsp.rs:94; documents.rs :207:39, :571, :572, :574 |
| (b) — killed, invisible to this sweep by cfg | 1 | `replace_full_text` @ documents.rs:519 — the mutated fn exists only under `#[cfg(not(feature = "tree-sitter"))]`; the all-features sweep cannot compile it. Kill verified on the `--no-default-features` leg (Task B7, typed assert 17 ms). A future no-default sweep leg would count it caught |

Zero (b) rows remain unexplained. One (d) row was killed incidentally since the Task 6
sweep: `AsRef<Rope> for Document` @ document.rs:327 (now in caught.txt) — final (d)
survivor count is 20 of 21.

Per-class reconciliation against the ratified table: every survivor's row and reason
hold as written; no survivor required a new argument. Viable-mutant sensitivity across
the cycle: 459/620 caught (baseline, ~74%) → 592/620 (~95%), with the kill evidence
carried per-row by the patch-loop replays (B2–B7) and this full-sweep run (B8).

**Cycle close.** The disposition table is complete and stable: 161 rows, tally
(a) 0 · (b) 120 · (c) 20 · (d) 21. The baseline snapshot
(`.superpowers/mutants-baseline-2026-09-09/`) served as the frozen inventory
throughout and is now deletable by the owner; `mutants.out/` remains scratch for the
next run. Future sweeps: scoped `make mutants FILE=<path>` for routine work; a full
sweep as the acceptance gate at disposition-cycle close.
