# Error Handling

This rule is normative: it states how errors must be handled, independent of
where the code lives today. `ServerError` — the crate's single typed error of
the server/protocol domain, behind the `ServerResult<T>` alias — is
constructed, mapped, and observed according to the sections below. Leaf
utilities without protocol semantics carry their own narrow error types
defined in the same `src/error.rs` (currently `RangeError`).

## The typed error

- Stay on `thiserror` (`err-thiserror-lib`). This is a library crate; `anyhow`
  is for applications and does not appear here.
- Preserve `source()` chains (`err-source-chain`). Never stringify an error
  into a variant: a `String` field loses the cause chain and leaves callers
  nothing to walk or downcast.
- The catch-all slot, when one is needed, keeps the boxed error as both the
  `Display` message and the `source()` chain node. Do NOT use
  `#[error(transparent)]` there: it forwards the `source()` *call* into the
  inner error (which usually has no source of its own), silently breaking the
  chain — verified empirically against thiserror 2.0.20:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    // ...typed variants...
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}
```

- Use `#[from]` only where the conversion loses no context: it implies
  `#[source]` and permits nothing in the variant beyond the source (and a
  backtrace). When an identifier must ride along, keep the chain with
  `#[source]` and build the variant through `map_err` or a named constructor:

```rust
#[error("failed to read document {path}")]
DocumentRead {
    path: PathBuf,
    #[source]
    error: std::io::Error,
}
```

- Keep `ServerError` as the single enum of the server/protocol domain. Leaf
  utilities without protocol semantics (such as `RangeExt` carrying
  `RangeError`) get their own narrow error type, defined in the same
  `src/error.rs` file; do not fold unrelated situations into `ServerError`
  merely to standardize the name, and do not scatter error types across
  modules.

## One boundary, both directions

- Exactly one `From<ServerError>` conversion turns a domain error into a wire
  error (`ResponseError`); domain code constructs no `ResponseError` at all.
  All domain code — trait impls, state, walkers — returns `Err(ServerError)`
  and stays protocol-neutral (`err-edge-mapping`).
- The wire adapter — `LanguageServerWithState` and the workspace-diagnostics
  request layer — is itself the boundary and constructs protocol-native
  `ResponseError` values directly (the staleness `CONTENT_MODIFIED` replies,
  the workspace-diagnostics-disabled `METHOD_NOT_FOUND` reply). Domain code
  below it stays `ServerError`-only.
- Clientless entry points (oneshot-style batch runs) convert the other way —
  wire error into `ServerError::rpc(code, message)`, preserving code and
  message. Every new entry point follows the same discipline: convert at the
  edge, keep the core neutral.

## No swallowed failures

- No `Result` on a path that can actually fail is dropped — propagate it with
  `?` or trace it. A bare `let _ =` on a fallible call is a bug unless the
  failure is impossible by construction.
- Fire-and-forget client requests log their failure through `tracing`, never
  drop it silently; when an error value is in hand, include it in the event,
  not just a message:

```rust
tracing::warn!("request failed: {error}");
```

- A stream of fallible entries is not one failure: a workspace walk skips an
  unreadable entry, traces it, and continues (`api-dir-enumeration`). One
  permission-denied directory must not abort the whole scan.

## No panics on external input

- Client-supplied values — position encodings, tree-sitter queries, globs,
  document text — are untrusted. Convert them through fallible paths or filter
  them out during negotiation and matching; never `panic!`, `expect`, or
  `unreachable!` on them (`api-parse-dont-validate`,
  `err-result-over-panic`). An unrecognized client encoding is filtered during
  negotiation, not panicked on during conversion.
- `expect` is for invariants whose violation means a bug, and its message must
  state the contract (`err-expect-bugs-only`):

```rust
let parser = doc_parser(doc).expect("has tree - must have parser");
```

  A caller-supplied value is never an invariant.

## Docs and Display

- `# Errors` on every public fallible item, `# Panics` on every public
  panicking item (`doc-errors-section`, `doc-panics-section`).
- Display messages start lowercase, carry no trailing punctuation, and include
  the discriminating values (`err-lowercase-msg`). When a Display string
  changes, update any doctest asserting it.

---
_No failure is stringified, swallowed, or panicked on: errors stay typed and
chained until one boundary converts them._
