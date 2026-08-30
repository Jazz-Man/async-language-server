# Refactoring Cycle 1 — error-model compliance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring error handling into compliance with `.claude/rules/error-handling.md`, make the clippy gate green at the root, and fix the four owner-approved behavior gaps — per spec `docs/superpowers/specs/2026-08-29-refactoring-cycle1-design.md`.

**Architecture:** Six small TDD tasks over the existing two-layer crate: the error type is reworked in one module (`src/error.rs`, renamed from `result.rs`, hard break), three behavior fixes land in walker / encoding negotiation / `didChange`, and observability is added under the `tracing` feature. No public surface change except `ServerError` itself (breaking, flagged).

**Tech Stack:** Rust 2024, thiserror 2.0.20, async-lsp 0.2.4, lsp-types 0.95.1 (verified: `PositionEncodingKind::new` is `pub const fn`).

## Global Constraints

- Spec decisions D1–D7 bind every task (two-cycle split, hard break, root-cause C1, `#[non_exhaustive]` now, M7 dropped, error-module organization + rename, no file reorganization).
- `.claude/rules/error-handling.md` is normative for all touched code: typed variants, preserved `source()` chains, one wire boundary, no swallowed failures, no panics on external input, lowercase Display.
- rust-skills acceptance rules: `err-thiserror-lib`, `err-source-chain`, `err-lowercase-msg`, `err-edge-mapping`, `api-dir-enumeration`, `api-parse-dont-validate`, `err-expect-bugs-only`, `doc-errors-section`, `test-arrange-act-assert`, `test-no-tautology`.
- **No new lint `#[allow]`s anywhere** (tech.md). Existing allows in touched files stay unless this plan removes one explicitly (only `clippy::unused_async` in `server_trait.rs`).
- **No git operations in tasks.** The agent never runs git write commands; commits, their content, and their timing are entirely the user's. Tasks end at verified-green, nothing else.
- All code comments and commit messages in English. Tests inline per module, real temp workspaces with millisecond-unique names.
- Feature gates: every task must compile under `--no-default-features`; task 6's cfg pairs exist exactly for that.
- Full battery gates the cycle's end (task 7), not each task; per-task minimum is the affected tests plus `cargo build --all-targets`.

---

### Task 1: C1 — clippy green at the root

**Files:**
- Modify: `src/server_trait.rs` (`method_not_implemented` at the bottom; module allow at line 3)
- Modify: `examples/minimal.rs:39-67`
- Modify: `examples/tree_sitter.rs:46-71`
- Modify: `src/oneshot/workspace_diagnostics.rs:323-332`

**Interfaces:**
- Consumes: trait signatures `fn … -> impl Future<Output = …> + Send` (RPITIT — unchanged).
- Produces: `method_not_implemented` returning `std::future::Ready<Result<T, ServerError>>` — still satisfies every trait default body unchanged.

- [ ] **Step 1: Verify the failure**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 3 errors `unused async for async trait impl function with no '.await' statements` at `examples/minimal.rs`, `examples/tree_sitter.rs`, `src/oneshot/workspace_diagnostics.rs`, plus the crate-internal variant on `method_not_implemented` once its module allow is removed (step 3).

- [ ] **Step 2: Make `method_not_implemented` a plain function**

In `src/server_trait.rs`, replace:

```rust
async fn method_not_implemented<T>(name: &'static str) -> Result<T, ServerError> {
    Err(ServerError::rpc(
        ErrorCode::METHOD_NOT_FOUND,
        format!("LSP method '{name}' has not been implemented"),
    ))
}
```

with:

```rust
fn method_not_implemented<T>(name: &'static str) -> std::future::Ready<Result<T, ServerError>> {
    std::future::ready(Err(ServerError::rpc(
        ErrorCode::METHOD_NOT_FOUND,
        format!("LSP method '{name}' has not been implemented"),
    )))
}
```

Trait default bodies are untouched: `Ready<Result<T, E>>` implements `Future<Output = Result<T, E>>` and is `Send` when `T, E` are, so it satisfies every `-> impl Future … + Send` default.

- [ ] **Step 3: Remove the mask**

Delete line 3 of `src/server_trait.rs`: `#![allow(clippy::unused_async)]`. The other three module allows (`unused_imports`, `unused_variables`, `must_use_candidate`) stay — Cycle 2.

- [ ] **Step 4: Fix `examples/minimal.rs`**

Replace the impl (the `#[allow(clippy::cast_possible_truncation)]` above it stays):

```rust
    #[allow(clippy::cast_possible_truncation)]
    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send {
        let Some(document) = state.document(&params.text_document.uri) else {
            return std::future::ready(Ok(full_report(Vec::new())));
        };

        let mut items = Vec::new();
        for (line, text) in document.text_contents().lines().enumerate() {
            let length = text.len();
            if length > MAX_LINE_BYTES {
                items.push(Diagnostic {
                    range: Range::new(
                        Position::new(line as u32, 0),
                        Position::new(line as u32, length as u32),
                    ),
                    message: format!(
                        "line is {length} bytes long, over the {MAX_LINE_BYTES}-byte limit"
                    ),
                    ..Diagnostic::default()
                });
            }
        }

        std::future::ready(Ok(full_report(items)))
    }
```

- [ ] **Step 5: Fix `examples/tree_sitter.rs`**

Replace the impl:

```rust
    fn document_diagnostics(
        &self,
        state: ServerState,
        params: DocumentDiagnosticParams,
    ) -> impl std::future::Future<Output = ServerResult<DocumentDiagnosticReportResult>> + Send {
        let Some(document) = state.document(&params.text_document.uri) else {
            return std::future::ready(Ok(full_report(Vec::new())));
        };

        let mut items = Vec::new();
        if document.has_syntax_tree() {
            // The tree is parsed and incrementally updated by the crate;
            // query it for parser ERROR nodes.
            for capture in document.query("(ERROR) @error").into_iter().flatten() {
                items.push(Diagnostic {
                    range: capture.range,
                    message: "syntax error".to_owned(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    ..Diagnostic::default()
                });
            }
        }

        std::future::ready(Ok(full_report(items)))
    }
```

- [ ] **Step 6: Fix the oneshot test server** (`src/oneshot/workspace_diagnostics.rs:323`)

```rust
        fn document_diagnostics(
            &self,
            state: ServerState,
            _params: DocumentDiagnosticParams,
        ) -> impl std::future::Future<Output = ServerResult<async_lsp::lsp_types::DocumentDiagnosticReportResult>> + Send {
            std::future::ready(Ok(full_report(vec![diagnostic(format!(
                "{} documents",
                state.documents().len()
            ))])))
        }
```

- [ ] **Step 7: Verify green**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clippy exits 0; all tests pass (87 + 12, default features).

---

### Task 2: `ServerError` hard break + module rename to `error.rs`

**Files:**
- Rename: `src/result.rs` → `src/error.rs` (filesystem `mv`, not git)
- Modify: `src/lib.rs:11` (`mod result;` → `mod error;`) and `src/lib.rs:41` (`pub use crate::result::…` → `pub use crate::error::…`)
- Modify import paths in: `src/serve.rs:13`, `src/server_trait.rs:21`, `src/transport.rs:18`, `src/workspace_walker.rs:9`, `src/server_state.rs:22`, `src/oneshot/server.rs:17`, `src/oneshot/workspace_diagnostics.rs:10` — every `result::` becomes `error::`
- Modify: `src/transport.rs:57-59` (typed `TcpConnect`)
- Modify: `src/workspace_walker.rs:66` (intermediate boxed form; Task 3 replaces it) and `:89-96` (`InvalidFilePath`)

**Interfaces:**
- Produces (breaking): `ServerError::{TcpConnect{port,error}, InvalidFilePath{path}, Rpc{code,message}, Lsp, Io, Other}`; `ServerError::rpc(code, message)` stays; `unknown()` and all string `From`s are gone; `From<BoxDynError>` now targets `Other`.
- Public re-export path `async_language_server::server::{ServerError, ServerResult, ServerErrorCode}` is unchanged.
- **This is the cycle's breaking change** — the user's commit message for this task should name it (product.md).

- [ ] **Step 1: Write the failing tests** — append to the end of the (renamed in step 3) module:

```rust
#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use async_lsp::{ErrorCode, ResponseError};

    use super::ServerError;

    #[test]
    fn tcp_connect_preserves_its_source() {
        let error = ServerError::TcpConnect {
            port: 9999,
            error: std::io::Error::other("connection refused"),
        };

        assert_eq!(error.to_string(), "failed to connect to port 9999");
        assert_eq!(error.source().unwrap().to_string(), "connection refused");
    }

    #[test]
    fn other_preserves_its_boxed_source() {
        let error = ServerError::Other(Box::new(std::io::Error::other("boom")));

        assert_eq!(error.to_string(), "boom");
        assert_eq!(error.source().unwrap().to_string(), "boom");
    }

    #[test]
    fn rpc_errors_map_to_their_own_code() {
        let response =
            ResponseError::from(ServerError::rpc(ErrorCode::METHOD_NOT_FOUND, "nope"));

        assert_eq!(response.code, ErrorCode::METHOD_NOT_FOUND);
        assert_eq!(response.message, "nope");
    }

    #[test]
    fn other_errors_map_to_internal_error() {
        let response =
            ResponseError::from(ServerError::Other(Box::new(std::io::Error::other("boom"))));

        assert_eq!(response.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(response.message, "boom");
    }

    #[test]
    fn io_errors_preserve_their_source() {
        let error = ServerError::Io(std::io::Error::other("disk gone"));

        assert_eq!(error.to_string(), "disk gone");
        assert_eq!(error.source().unwrap().to_string(), "disk gone");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib error::` → Expected: compile errors (variants/paths don't exist yet).

- [ ] **Step 3: Rename the module and rewrite it**

Run: `mv src/result.rs src/error.rs`. Update `src/lib.rs` and the seven import sites listed in Files. Replace the whole content of `src/error.rs` with:

```rust
#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use async_lsp::ResponseError;
use thiserror::Error;

type BoxDynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub use async_lsp::ErrorCode as ServerErrorCode;

/// Convenience `Result` alias for operations that can fail with a [`ServerError`].
pub type ServerResult<T> = Result<T, ServerError>;

/// An error that can occur while running a language server.
///
/// # Examples
///
/// ```
/// use async_language_server::server::ServerError;
///
/// let error = ServerError::TcpConnect {
///     port: 9999,
///     error: std::io::Error::other("connection refused"),
/// };
/// assert_eq!(error.to_string(), "failed to connect to port 9999");
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerError {
    /// Failed to connect a socket to the given TCP port.
    #[error("failed to connect to port {port}")]
    TcpConnect {
        /// The port that was being connected to.
        port: u16,
        /// The underlying connect error.
        #[source]
        error: std::io::Error,
    },
    /// A file path could not be represented as a `file://` URL.
    #[error("invalid file path '{path}'")]
    InvalidFilePath {
        /// The path that could not be converted.
        path: PathBuf,
    },
    /// JSON-RPC error sent to or received from the client.
    #[error("json-rpc error {code}: {message}")]
    Rpc {
        /// The JSON-RPC error code.
        code: ServerErrorCode,
        /// The JSON-RPC error message.
        message: String,
    },
    /// Error raised by the underlying async-lsp machinery.
    #[error("{0}")]
    Lsp(#[from] async_lsp::Error),
    /// I/O error raised by a transport or a file read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// An error that does not fit any other variant; the boxed error
    /// provides the `Display` message and is exposed as the `source()` node.
    #[error("{0}")]
    Other(#[from] BoxDynError),
}

impl ServerError {
    /// Creates a JSON-RPC error with the given code and message.
    #[must_use]
    pub fn rpc(code: ServerErrorCode, message: impl ToString) -> Self {
        ServerError::Rpc {
            code,
            message: message.to_string(),
        }
    }
}

impl From<ServerError> for ResponseError {
    fn from(value: ServerError) -> Self {
        match value {
            ServerError::Rpc { code, message } => ResponseError::new(code, message),
            other => ResponseError::new(ServerErrorCode::INTERNAL_ERROR, other.to_string()),
        }
    }
}
```

- [ ] **Step 4: Fix the call sites**

`src/transport.rs:57-59`:

```rust
            let stream = TcpStream::connect(addr)
                .await
                .map_err(|error| ServerError::TcpConnect { port, error })?;
```

`src/workspace_walker.rs:66` (intermediate — Task 3 replaces it):

```rust
                let entry =
                    entry.map_err(|error| ServerError::Other(Box::new(error)))?;
```

`src/workspace_walker.rs:89-96`:

```rust
pub(crate) fn path_to_url(path: &Path) -> ServerResult<Url> {
    Url::from_file_path(path).map_err(|()| ServerError::InvalidFilePath {
        path: path.to_path_buf(),
    })
}
```

- [ ] **Step 5: Verify**

Run: `cargo build --all-targets && cargo test`
Expected: builds; new error tests pass; all existing tests pass. (`ServerError::from("…")` sites must be gone — compiler enforces the hard break.)

---

### Task 3: Walker resilience — skip-and-trace per entry

**Files:**
- Modify: `src/workspace_walker.rs:65-70`
- Test: inline `#[cfg(test)] mod tests` (new) in `src/workspace_walker.rs`

**Interfaces:**
- Consumes: `ServerError::Other` from Task 2 (the intermediate `?` disappears).
- Produces: `WorkspaceWalker::files()` that never fails on a single unreadable entry.

- [ ] **Step 1: Write the failing test** — append to `src/workspace_walker.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{WorkspaceWalkConfig, WorkspaceWalker};

    // One unreadable entry must not abort the scan; this test is unix-only
    // because the failure is injected with filesystem permissions.
    #[test]
    #[cfg(unix)]
    fn files_skips_unreadable_entries() {
        use std::os::unix::fs::PermissionsExt;

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-walker-skip-{millis}"));
        fs::create_dir_all(root.join("bad")).expect("bad dir can be created");
        fs::write(root.join("good.test"), "good").expect("good file can be written");
        fs::set_permissions(root.join("bad"), fs::Permissions::from_mode(0o000))
            .expect("permissions can be restricted");

        let walker = WorkspaceWalker::new(&[root.clone()], Default::default())
            .expect("walker can be created");
        let files = walker.files().expect("walk succeeds despite unreadable entry");

        assert!(files.iter().any(|file| file.ends_with("good.test")));

        fs::set_permissions(root.join("bad"), fs::Permissions::from_mode(0o755))
            .expect("permissions can be restored");
        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib workspace_walker`
Expected: FAIL — `files()` returns `Err` on the unreadable entry today (scan aborted).

- [ ] **Step 3: Implement skip-and-trace**

Replace the entry loop in `files()`:

```rust
            for entry in builder.build() {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        #[cfg(feature = "tracing")]
                        tracing::warn!("skipping unreadable workspace entry: {error}");
                        #[cfg(not(feature = "tracing"))]
                        drop(error);
                        continue;
                    }
                };
                if entry.file_type().is_some_and(|ty| ty.is_file()) {
                    files.push(entry.into_path());
                }
            }
```

(The `#[cfg]`-pair keeps `error` consumed under `--no-default-features`; without the `tracing` feature there is no observation channel, and the skip — the behavior — still happens.)

- [ ] **Step 4: Verify**

Run: `cargo test --lib workspace_walker && cargo test --no-default-features --lib workspace_walker`
Expected: PASS in both feature configurations.

---

### Task 4: Encoding negotiation without panic

**Files:**
- Modify: `src/text_utils/encoding.rs` (new `try_from_lsp` + new test module)
- Modify: `src/server_with_state.rs:181-185` (negotiation filter) + test

**Interfaces:**
- Produces: `Encoding::try_from_lsp(&PositionEncodingKind) -> Option<Encoding>` (public — it is the documented way to convert client-supplied kinds fallibly).
- `from_lsp` and its `From` impls stay (internal known-kind conversion; no external input reaches the panic branch after this task).

- [ ] **Step 1: Write the failing tests**

In `src/text_utils/encoding.rs`, append:

```rust
#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::PositionEncodingKind;

    use super::{Encoding, LspPositionEncoding};

    #[test]
    fn try_from_lsp_returns_none_for_unknown_kinds() {
        assert_eq!(Encoding::try_from_lsp(&PositionEncodingKind::new("utf-7")), None);
        assert_eq!(Encoding::try_from_lsp(&LspPositionEncoding::UTF8), Some(Encoding::UTF8));
        assert_eq!(Encoding::try_from_lsp(&LspPositionEncoding::UTF16), Some(Encoding::UTF16));
        assert_eq!(Encoding::try_from_lsp(&LspPositionEncoding::UTF32), Some(Encoding::UTF32));
    }
}
```

In `src/server_with_state.rs` tests, add:

```rust
    #[test]
    fn initialize_ignores_unknown_client_encodings() {
        let root = temp_workspace("unknown-encoding");
        let mut server = LanguageServerWithState::new(ClientSocket::new_closed(), TestServer);

        let mut params = initialize_params(&root);
        params.capabilities.general = Some(GeneralClientCapabilities {
            position_encodings: Some(vec![
                PositionEncodingKind::new("utf-7"),
                PositionEncodingKind::UTF16,
            ]),
            ..Default::default()
        });

        let result = futures::executor::block_on(server.initialize(params))
            .expect("server can initialize");

        assert_eq!(
            result.capabilities.position_encoding,
            Some(PositionEncodingKind::UTF16)
        );

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
```

(Extend the test-module imports with `GeneralClientCapabilities` and `PositionEncodingKind`.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test try_from_lsp initialize_ignores_unknown`
Expected: compile error (`try_from_lsp` missing) and, once added, the integration test panics through `from_lsp` before the filter exists.

- [ ] **Step 3: Implement `try_from_lsp`**

In `src/text_utils/encoding.rs`, after `from_lsp`:

```rust
    /// Creates an encoding from its `lsp_types` counterpart, if it is one of
    /// the supported kinds (UTF-8, UTF-16, UTF-32).
    ///
    /// Returns `None` for any other kind: client capabilities can carry
    /// values this crate does not know, and negotiation ignores them
    /// instead of failing.
    #[must_use]
    pub fn try_from_lsp(encoding: &LspPositionEncoding) -> Option<Self> {
        if encoding == &LspPositionEncoding::UTF8 {
            Some(Self::UTF8)
        } else if encoding == &LspPositionEncoding::UTF16 {
            Some(Self::UTF16)
        } else if encoding == &LspPositionEncoding::UTF32 {
            Some(Self::UTF32)
        } else {
            None
        }
    }
```

- [ ] **Step 4: Filter during negotiation**

In `src/server_with_state.rs` (initialize, step 3 of the negotiation), replace:

```rust
            let client_available_encodings: Vec<Encoding> = client_available_encodings
                .into_iter()
                .map(Into::into)
                .collect();
```

with:

```rust
            let client_available_encodings: Vec<Encoding> = client_available_encodings
                .into_iter()
                .filter_map(|kind| Encoding::try_from_lsp(&kind))
                .collect();
```

- [ ] **Step 5: Verify**

Run: `cargo test initialize_ignores_unknown try_from_lsp && cargo test --no-default-features`
Expected: PASS — no panic, negotiated UTF-16.

---

### Task 5: `didChange` fallback keeps the document

**Files:**
- Modify: `src/server_state.rs:494-511`
- Test: inline in `src/server_state.rs` tests

**Interfaces:**
- Consumes: nothing new.
- Produces: behavior change — a failed incremental update whose disk re-read also fails now keeps the last-known text instead of removing the document.

- [ ] **Step 1: Write the failing test** — append to `src/server_state.rs` tests:

```rust
    #[test]
    fn failed_incremental_change_keeps_document_when_reread_fails() {
        let root = temp_workspace("keep-last-known");
        let uri = {
            let path = root.join("missing.test");
            Url::from_file_path(path).expect("path can be converted to a URL")
        };
        let mut state = ServerState::new::<TestServer>(ClientSocket::new_closed());
        open_document(&mut state, uri.clone(), "original");

        let _ = state.handle_document_change::<TestServer>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                // An out-of-bounds LINE makes the incremental application
                // fail (columns are clamped during encoding conversion, so
                // out-of-bounds columns do not); the file does not exist on
                // disk, so the re-read fails too.
                range: Some(Range::new(
                    Position::new(50, 0),
                    Position::new(50, 1),
                )),
                range_length: None,
                text: "x".into(),
            }],
        });

        let document = state.document(&uri).expect("document stays tracked");
        assert_eq!(document.text_contents(), "original");

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
```

(Extend the test-module imports with `VersionedTextDocumentIdentifier`, `TextDocumentContentChangeEvent`, `Position`, `Range` as needed.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib server_state::tests::failed_incremental_change`
Expected: FAIL — `state.document(&uri)` is `None` today (the document is removed).

- [ ] **Step 3: Implement keep-last-known**

Replace the fallback block in `handle_document_change`:

```rust
        // If the incremental update failed, we will re-insert the entire file instead
        // Note: we must first drop the document reference to prevent a deadlock
        if incremental_update_failed {
            let uri = doc.uri.clone();
            let version = doc.version();
            let language = doc.language.clone();

            drop(entry);

            // NOTE: We must read the contents of the file synchronously
            // as the fallback here, since notification handlers are actually
            // synchronous both according to LSP spec and the async-lsp crate.
            // The re-read intentionally replaces the in-memory text, whose
            // edits were only partially applied; this discards unsaved
            // editor changes, which is the accepted trade-off of the
            // synchronous-handler constraint.
            if let Ok(text) = std::fs::read_to_string(uri.path()) {
                self.insert_document::<T>(uri, text, version, language, DocumentOrigin::Open);
            } else {
                // Keeping the last-known (possibly partially edited) text is
                // better than dropping the document: the editor still
                // considers it open, and handlers keep resolving it.
                #[cfg(feature = "tracing")]
                tracing::warn!(
                    "did_change: incremental update failed and '{}' could not be re-read; keeping last-known text",
                    uri
                );
            }
        }
```

- [ ] **Step 4: Verify**

Run: `cargo test --lib server_state && cargo test --no-default-features --lib server_state`
Expected: PASS — new test green, existing change/close tests unchanged.

---

### Task 6: Tracing on invalid queries and fire-and-forget requests

**Files:**
- Modify: `src/document.rs:169-199` (`query`) — doc comment + error branch
- Modify: `src/workspace_diagnostics.rs:276-289` (`register_configuration`) and `:333-344` (`refresh_diagnostics`)

**Interfaces:** none (observability only).

- [ ] **Step 1: `Document::query` logs invalid queries**

Replace the `Query::new` line and update the doc comment's last sentence to "…or when the query string fails to compile (logged under the `tracing` feature).":

```rust
        let query = match Query::new(lang, query.as_ref()) {
            Ok(query) => query,
            Err(error) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("invalid tree-sitter query '{}': {error}", query.as_ref());
                #[cfg(not(feature = "tracing"))]
                drop(error);
                return None;
            }
        };
```

- [ ] **Step 2: Fire-and-forget requests log failures**

In `register_configuration`, replace the `spawn` block:

```rust
    spawn(async move {
        let result = state
            .client()
            .request::<RegisterCapability>(RegistrationParams {
                registrations: vec![Registration {
                    id: "async-language-server.workspaceDiagnostics.configuration".into(),
                    method: "workspace/didChangeConfiguration".into(),
                    register_options: Some(serde_json::json!({
                        "section": setting.key.section(),
                    })),
                }],
            })
            .await;
        #[cfg(feature = "tracing")]
        if let Err(error) = &result {
            tracing::warn!("workspace diagnostics capability registration failed: {error}");
        }
        #[cfg(not(feature = "tracing"))]
        drop(result);
    });
```

(`refresh_diagnostics` uses `WorkspaceDiagnosticRefresh` and the message "workspace diagnostic refresh request failed: {error}". The `drop(result)` exists only in `not(tracing)` builds, where no observation channel exists — the request's side effect has already run.)

- [ ] **Step 3: Verify all three configurations compile and pass**

Run: `cargo test && cargo test --no-default-features && cargo test --all-features`
Expected: PASS everywhere — the cfg pairs are exactly what the featureless build checks.

---

### Task 7: Full battery and cycle close

**Files:** none (verification only).

- [ ] **Step 1: Run the battery**

```bash
cargo build --all-targets
cargo test
cargo test --no-default-features
cargo test --all-features
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Expected: all green, clippy included. Any failure → follow `no-workarounds` + `superpowers:systematic-debugging` (invoke both skills, find the root cause); never suppress.

- [ ] **Step 2: Rule-compliance pass**

Check every file touched in this cycle against `.claude/rules/error-handling.md`: typed variants only, `source()` chains intact, `ResponseError` constructed only in the `From` impl, no bare `let _ =` on fallible calls, no panics on external input, lowercase Display.

- [ ] **Step 3: Report**

Report battery results and rule-compliance outcome. Commits and their messages are the user's; the breaking change to name is Task 2's. Cycle 2 (lint-allow shedding, README, doc wording) remains for its own brainstorm.
