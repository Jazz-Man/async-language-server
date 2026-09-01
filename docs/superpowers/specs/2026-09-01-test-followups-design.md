# Test follow-ups: link-resolve routing, RangeExt flavor parity, conversion doc honesty

Date: 2026-09-01. Branch: `feature/test-followups` (from
`feature/testing-cycle`). Closes the three follow-ups the testing cycle's
final review registered and the rm-socket stage-2 spec listed as out of
scope (`docs/superpowers/specs/2026-09-01-rm-socket-stage2-design.md`).

## Verified API facts (design inputs)

Confirmed against the pinned dependency sources (`async-lsp` 0.2.4,
`lsp-types` 0.95.1, per `Cargo.lock`):

- `DocumentLink` has exactly four fields — `range: Range`,
  `target: Option<Url>`, `tooltip: Option<String>`, `data: Option<Value>`
  (`lsp-types/src/document_link.rs:47-67`). No field names the link's
  source document; `target` is the link's destination.
- `DocumentLinkParams` carries `text_document: TextDocumentIdentifier`
  (`document_link.rs:34-43`): the plain request names its document, the
  resolve request cannot.
- async-lsp binds `"documentLink/resolve"` to the trait method
  `document_link_resolve(params: DocumentLink) -> DocumentLink`, with the
  macro-generated `method_not_found` default every request method gets
  (`omni_trait_generated.rs:49`, `omni_trait.rs:108-117`); only
  `initialize` is required.
- `CompletionItem` and `CodeAction` (the sibling resolve params types)
  likewise carry no source-document URI.

## Owner decisions

- RangeExt out-of-range positions: error, matching the bytes and LSP
  flavors — not clamping.
- documentLink/resolve: the sole-document heuristic, exactly like the
  sibling resolves.
- Behavior compatibility is not a constraint (owner's call); where the
  fork rule asks for a breaking-surface note, it stays one line in the
  commit message.

## Change 1 — documentLink/resolve routes through the resolve macro

`src/server/with_state/mod.rs`: move the
`document_link_resolve => link_resolve @ crate::requests::DocumentLinkResolve`
line out of the `implement_methods!` table into its own
`implement_resolve_method!` entry beside `completion_item_resolve` and
`code_action_resolve`. Conversion then flows through
`convert_resolve_item` against the sole tracked document (skipped when the
server tracks zero or more than one). The staleness check keyed on the
link target's version disappears — it was keyed on the wrong document, and
the source document's identity is not recoverable from the params.

`src/requests/document_link_resolve.rs`: delete the `extract_url` override
(the trait default returns `None`) and leave the same explanatory comment
`completion_resolve.rs` carries; `modify_params`/`modify_response` stay as
they are.

New W0 tests in a `#[cfg(test)] mod tests` block beside the impl,
mirroring `completion_resolve.rs` on the `"🙂abc"` + UTF-16 fixture:

1. `resolve_range_converts_against_the_sole_tracked_document` — outgoing:
   the UTF-8 byte position 4 becomes UTF-16 unit 2;
2. `resolve_range_passes_through_without_a_document` — a `None` snapshot
   leaves the range unchanged;
3. `resolve_echo_round_trip_is_identity` — incoming then outgoing returns
   the original position.

No existing test pins the old target-keyed behavior (verified by search),
so the move breaks nothing.

## Change 2 — tree-sitter RangeExt flavors error on out-of-range positions

`src/text_utils/range_ext/tree_sitter.rs`:

- `split_at`: after the scan and the end-of-text check, a position that
  was not found returns `Err(RangeError::PositionOutOfRange)` instead of
  silently degenerating to `at_byte = start_byte`;
- `sub`: the same for `from`/`to` — which also removes the silent
  inverted-range case (`from` found, `to` beyond the text, `to_byte`
  falling back below `from_byte`).

The trait's `# Errors` sections already promise `PositionOutOfRange`; with
this change they are true for all three flavors. No internal call site
uses the tree-sitter `split_at`/`sub` (verified), so the blast radius is
downstream only.

New tests in `tree_sitter_tests.rs`, mirroring the bytes and LSP
out-of-range pins:

1. `split_at_beyond_the_text_returns_position_out_of_range` — a column
   past the end of the text, and separately a row past the end;
2. `sub_positions_beyond_the_text_return_position_out_of_range`;
3. `split_at_mismatched_text_length_returns_text_range_mismatch` — the
   missing direct pin for this call site's text-length check (the existing
   mismatch test covers `sub_delimited` only).

## Change 3 — conversion.rs states the real criterion

- Delete `modify_incoming_diagnostic` and `modify_outgoing_diagnostic`
  (pure direction delegates). The three call sites
  (`src/requests/code_action.rs` ×2,
  `src/requests/document_diagnostics.rs` ×1) call
  `convert_diagnostic(..., Direction::Incoming/Outgoing)` directly.
- Rewrite the module doc: `convert_*` helpers are
  direction-parameterized; `modify_*` remains only for fixed-direction
  composites that mix per-document and per-URL conversion — no pure
  direction pins survive, so the stated criterion and the code agree
  again.
- Remove the delegate pair's entry from `.dupes-ignore.toml` (the one
  whose reason names the "named incoming/outgoing wrappers over the
  direction-generic convert_diagnostic"); `cargo dupes check` stays at
  exit 0 with no new entries.
- No test changes: the W0 tests for `code_action` and
  `document_diagnostics` exercise these paths, and the behavior is
  identical through the direct call.

## Constraints

- Full battery green in all three feature configurations after every
  task; `cargo dupes check` exit 0 after Change 3.
- Git read-only for agents; the owner commits (three logical commits, one
  per change). English artifacts; LSP-first navigation; no lint
  suppression.

## Out of scope

The remaining registered follow-ups (termination fixture migration to
`temp_workspace`, the product.md plural, the DocumentRead illustrative
marker), the async-lsp #30 watch, and the parked symbols work.
