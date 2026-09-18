//! Method dispatch over the wire. Four producers answer `-32601`: the
//! router for a name nothing registered under it ("No such method ..."),
//! the async-lsp trait default for a registered `lsp_types` method the
//! impl does not override ("No such method: ...", with a colon), the
//! dispatch engine's trait default for a wired method without a [`Server`]
//! implementation ("LSP method '...' has not been implemented"), and the
//! capability gate for an implemented method the capabilities do not
//! advertise ("... is not advertised in the server capabilities"). The
//! tests below pin the producers, not just the code.

use async_lsp::lsp_types::{
    ClientCapabilities, Hover, HoverParams, OneOf, ServerCapabilities, WorkDoneProgressOptions,
    WorkspaceSymbol, WorkspaceSymbolOptions, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use serde_json::{Value, json};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::server::testing::{
    EchoServer, bounded, did_open, echo_hover, hover_params, spawn_wire_server,
};
use crate::server::{Server, ServerResult, ServerState};

/// Advertises the full all-request fixture so [`wired_methods_dispatch`]
/// drives every row through an open gate: each method reaches the engine
/// and answers a result or the engine's trait default — never the
/// capability gate. Serves `hover` so the one implemented-and-advertised
/// pin stays positive (`Ok(None)`; `result: null` is still a result).
#[derive(Clone)]
struct AllMethodsServer;

impl Server for AllMethodsServer {
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(crate::testing::all_request_capabilities())
    }

    fn hover(
        &self,
        _state: ServerState,
        _params: HoverParams,
    ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
        std::future::ready(Ok(None))
    }
}

#[tokio::test]
async fn unknown_methods_answer_method_not_found() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // A method name no handler is registered under answers the router
    // default: -32601, message "No such method ...". Every client-to-server
    // request `lsp_types` defines IS registered — the 48 dispatch rows,
    // workspace/diagnostic and initialize by the wrapper, shutdown by
    // the trait default — so only a synthetic name outside `lsp_types`
    // reaches this reply. The empty params are deliberate: deserialization
    // lives inside each registered handler, so the router default fires
    // before params validation, and even garbage params cannot turn this
    // unknown-method reply into the invalid-params error.
    let response = client
        .request(10, "textDocument/nonexistent", json!({}))
        .await;
    assert_eq!(response["error"]["code"], -32601);
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("No such method")),
        "the router, not the dispatch engine, must answer: {response}",
    );

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn wired_methods_dispatch() {
    let (mut client, server) = spawn_wire_server(AllMethodsServer);
    client.initialize_client(&["utf-16"]).await;

    // All 48 `lsp_dispatch!` rows (see `with_state`), by wire name. A
    // trait method without a row is the one gap the compiler cannot see:
    // deleting the row drops the impl override, and the method then
    // answers with one of the two "No such method" producers — the router
    // for an unregistered name, async-lsp's trait default for a registered
    // one (with a colon). Neither can fire while the row exists: the
    // engine then answers with a result or its own trait default ("LSP
    // method '...' has not been implemented"). The prefix check below
    // deliberately catches both No-such variants. Params stay minimally
    // valid so every request clears params validation inside its
    // registered handler and reaches the engine.
    for (id, (method, _, params)) in wired_requests().enumerate() {
        let response = client
            .request(i64::try_from(id).expect("small id") + 100, method, params)
            .await;
        if method == "textDocument/hover" {
            assert!(
                response.get("error").is_none(),
                "hover is implemented and must succeed: {response}",
            );
        } else {
            let message = response["error"]["message"].as_str().unwrap_or_default();
            assert!(
                !message.starts_with("No such method"),
                "no dispatch row for {method}: a No-such-method producer answered",
            );
        }
    }

    drop(client);
    let _ = bounded(server).await;
}

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
            _state: ServerState,
            params: HoverParams,
        ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
            let entered = Arc::clone(&self.entered);
            async move {
                entered.fetch_add(1, Ordering::Relaxed);
                Ok(echo_hover(params.text_document_position_params.position))
            }
        }
    }

    let entered = Arc::new(AtomicUsize::new(0));
    let (mut client, server) = spawn_wire_server(CountingServer {
        entered: Arc::clone(&entered),
    });
    let result = client.initialize_client(&["utf-8"]).await;
    assert!(
        result["capabilities"]["hoverProvider"].is_null(),
        "precondition: the fixture advertises nothing",
    );

    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///gate.test", "body"),
        )
        .await;
    let response = client
        .request(
            2,
            "textDocument/hover",
            hover_params("file:///gate.test", 0),
        )
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

    drop(client);
    let _ = bounded(server).await;
}

// The resolve family gates on its provider's resolve option, and an
// advertised provider dispatches.
#[tokio::test]
async fn resolve_gates_follow_the_providers_resolve_option() {
    // The polarity rides a const parameter: `server_capabilities` is an
    // associated fn with no `self`, so an instance field could never be
    // read there (the plan's second transcription bug). True builds the
    // provider WITHOUT `resolve_provider` — the plan carried these bodies
    // inverted under `resolve_options`, so the name states the polarity
    // it actually drives.
    #[derive(Clone)]
    struct SymbolServer<const WITHOUT_RESOLVE: bool>;

    impl<const WITHOUT_RESOLVE: bool> Server for SymbolServer<WITHOUT_RESOLVE> {
        fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
            // `WorkspaceSymbolOptions` derives no `Default` in pinned
            // lsp-types 0.95.1: both arms construct it fully.
            let provider = if WITHOUT_RESOLVE {
                WorkspaceSymbolOptions {
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    resolve_provider: None,
                }
            } else {
                WorkspaceSymbolOptions {
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    resolve_provider: Some(true),
                }
            };
            Some(ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Right(provider)),
                ..ServerCapabilities::default()
            })
        }

        fn symbol(
            &self,
            _state: ServerState,
            _params: WorkspaceSymbolParams,
        ) -> impl Future<Output = ServerResult<Option<WorkspaceSymbolResponse>>> + Send {
            // An empty but real result set: the reply must be visibly the
            // handler's, never one of the -32601 producers.
            std::future::ready(Ok(Some(WorkspaceSymbolResponse::Nested(vec![]))))
        }

        fn workspace_symbol_resolve(
            &self,
            _state: ServerState,
            params: WorkspaceSymbol,
        ) -> impl Future<Output = ServerResult<WorkspaceSymbol>> + Send {
            std::future::ready(Ok(params))
        }
    }

    // Advertised WITHOUT resolve_provider: symbol dispatches, resolve is
    // gated off.
    let (mut client, server) = spawn_wire_server(SymbolServer::<true>);
    client.initialize_client(&["utf-8"]).await;
    let symbol = client
        .request(2, "workspace/symbol", json!({ "query": "" }))
        .await;
    assert!(
        symbol.get("result").is_some(),
        "symbol dispatches: {symbol:?}",
    );
    let resolve = client
        .request(3, "workspaceSymbol/resolve", resolve_symbol_params())
        .await;
    assert_eq!(
        resolve["error"]["code"].as_i64(),
        Some(-32601),
        "resolve without the provider's resolve option is gated: {resolve:?}",
    );

    drop(client);
    let _ = bounded(server).await;

    // Advertised WITH resolve_provider: both dispatch.
    let (mut client, server) = spawn_wire_server(SymbolServer::<false>);
    client.initialize_client(&["utf-8"]).await;
    let resolve = client
        .request(2, "workspaceSymbol/resolve", resolve_symbol_params())
        .await;
    assert!(
        resolve.get("result").is_some(),
        "resolve dispatches under the advertised resolve option: {resolve:?}",
    );

    drop(client);
    let _ = bounded(server).await;
}

/// Minimally valid `workspaceSymbol/resolve` params: a [`WorkspaceSymbol`]
/// whose `location` carries a full `Location` (the untagged `OneOf`'s
/// first variant).
fn resolve_symbol_params() -> Value {
    json!({ "name": "x", "kind": 1, "location": {
        "uri": "file:///x.test", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
    }})
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

    let (mut client, server) = spawn_wire_server(BareServer);
    client.initialize_client(&["utf-8"]).await;
    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///trio.test", "body"),
        )
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

    drop(client);
    let _ = bounded(server).await;
}

/// Wire name, trait method, and raw JSON params for every wired request, in
/// `lsp_dispatch!` table order. One row per method, at the wire format
/// itself: the matrix is data, not code, and a row that stops parsing fails
/// the test loudly at the `expect` instead of degrading to a params error.
fn wired_requests() -> impl Iterator<Item = (&'static str, &'static str, Value)> {
    [
        ("textDocument/hover", "hover", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/declaration", "declaration", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/definition", "definition", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/references", "references", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "context": { "includeDeclaration": false } }"#),
        ("textDocument/documentLink", "link", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/rename", "rename", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "newName": "n" }"#),
        ("textDocument/prepareRename", "rename_prepare", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/formatting", "document_format", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/rangeFormatting", "document_range_format", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/implementation", "implementation", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/typeDefinition", "type_definition", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/documentHighlight", "document_highlight", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/onTypeFormatting", "on_type_formatting", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "ch": "{", "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/foldingRange", "folding_range", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/linkedEditingRange", "linked_editing_range", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/codeLens", "code_lens", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/willSaveWaitUntil", "will_save_wait_until", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "reason": 1 }"#),
        ("textDocument/documentColor", "document_color", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/colorPresentation", "color_presentation", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "color": { "red": 0.0, "green": 0.0, "blue": 0.0, "alpha": 0.0 }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/prepareCallHierarchy", "prepare_call_hierarchy", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/prepareTypeHierarchy", "prepare_type_hierarchy", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/moniker", "moniker", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("workspace/willCreateFiles", "will_create_files", r#"{ "files": [ { "uri": "file:///tmp/wire.txt" } ] }"#),
        ("workspace/willRenameFiles", "will_rename_files", r#"{ "files": [ { "oldUri": "file:///tmp/wire.txt", "newUri": "file:///tmp/wire-2.txt" } ] }"#),
        ("workspace/willDeleteFiles", "will_delete_files", r#"{ "files": [ { "uri": "file:///tmp/wire.txt" } ] }"#),
        ("textDocument/inlayHint", "inlay_hint", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/documentSymbol", "document_symbol", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("workspace/executeCommand", "execute_command", r#"{ "command": "echo" }"#),
        ("textDocument/semanticTokens/full", "semantic_tokens_full", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/semanticTokens/range", "semantic_tokens_range", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/semanticTokens/full/delta", "semantic_tokens_full_delta", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "previousResultId": "0" }"#),
        ("textDocument/completion", "completion", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/codeAction", "code_action", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "context": { "diagnostics": [] } }"#),
        ("textDocument/diagnostic", "document_diagnostics", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/selectionRange", "selection_range", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "positions": [ { "line": 0, "character": 0 } ] }"#),
        ("textDocument/inlineValue", "inline_value", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "context": { "frameId": 0, "stoppedLocation": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("callHierarchy/incomingCalls", "incoming_calls", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("callHierarchy/outgoingCalls", "outgoing_calls", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("typeHierarchy/supertypes", "supertypes", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("typeHierarchy/subtypes", "subtypes", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("workspace/symbol", "symbol", r#"{ "query": "" }"#),
        ("textDocument/signatureHelp", "signature_help", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("completionItem/resolve", "completion_resolve", r#"{ "label": "l" }"#),
        ("codeAction/resolve", "code_action_resolve", r#"{ "title": "t" }"#),
        ("documentLink/resolve", "link_resolve", r#"{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("codeLens/resolve", "code_lens_resolve", r#"{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("inlayHint/resolve", "inlay_hint_resolve", r#"{ "position": { "line": 0, "character": 0 }, "label": "l" }"#),
        ("workspaceSymbol/resolve", "workspace_symbol_resolve", r#"{ "name": "f", "kind": 12, "location": { "uri": "file:///tmp/wire.txt" } }"#),
    ]
    .into_iter()
    .map(|(method, trait_method, params)| {
        (method, trait_method, serde_json::from_str(params).expect("fixture parses"))
    })
}

#[test]
fn inventory_covers_every_dispatch_row() {
    use crate::server::inventory::METHOD_NAMES;

    // The inventory spans the whole table since the resolve family joined
    // it (it gates dispatch on its provider's resolve option): every wired
    // request must have its inventory row, or the dispatch gate and the
    // default-warning have no opinion about it.
    for (_, trait_method, _) in wired_requests() {
        assert!(
            METHOD_NAMES.contains(&trait_method),
            "{trait_method}: every dispatch row is inventoried",
        );
    }
}
