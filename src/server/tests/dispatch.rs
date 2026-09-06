//! Method dispatch over the wire. Three producers answer `-32601`: the
//! router for a name nothing registered under it ("No such method ..."),
//! the async-lsp trait default for a registered `lsp_types` method the
//! impl does not override ("No such method: ...", with a colon), and the
//! dispatch engine's trait default for a wired method without a `Server`
//! implementation ("LSP method '...' has not been implemented"). The tests
//! below pin the producers, not just the code.

use serde_json::{Value, json};

use crate::server::testing::{EchoServer, bounded, spawn_wire_server};

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
    // before params validation and even garbage params cannot turn this
    // into -32602.
    let response = client
        .request(10, "textDocument/nonexistent", json!({}))
        .await;
    assert_eq!(response["error"]["code"], -32601);
    assert!(
        response["error"]["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("No such method")),
        "the router, not the dispatch engine, must answer: {response}"
    );

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn wired_methods_dispatch() {
    let (mut client, server) = spawn_wire_server(EchoServer);
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
    for (id, (method, params)) in wired_requests().enumerate() {
        let response = client
            .request(i64::try_from(id).expect("small id") + 100, method, params)
            .await;
        if method == "textDocument/hover" {
            assert!(
                response.get("error").is_none(),
                "hover is implemented and must succeed: {response}"
            );
        } else {
            let message = response["error"]["message"].as_str().unwrap_or_default();
            assert!(
                !message.starts_with("No such method"),
                "no dispatch row for {method}: a No-such-method producer answered"
            );
        }
    }

    drop(client);
    let _ = bounded(server).await;
}

/// Wire name and raw JSON params for every wired request, in `lsp_dispatch!`
/// table order. One row per method, at the wire format itself: the matrix is
/// data, not code, and a row that stops parsing fails the test loudly at the
/// `expect` instead of degrading to a params error.
fn wired_requests() -> impl Iterator<Item = (&'static str, Value)> {
    [
        ("textDocument/hover", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/declaration", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/definition", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/references", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "context": { "includeDeclaration": false } }"#),
        ("textDocument/documentLink", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/rename", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "newName": "n" }"#),
        ("textDocument/prepareRename", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/formatting", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/rangeFormatting", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/implementation", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/typeDefinition", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/documentHighlight", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/onTypeFormatting", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 }, "ch": "{", "options": { "tabSize": 4, "insertSpaces": true } }"#),
        ("textDocument/foldingRange", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/linkedEditingRange", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/codeLens", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/willSaveWaitUntil", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "reason": 1 }"#),
        ("textDocument/documentColor", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/colorPresentation", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "color": { "red": 0.0, "green": 0.0, "blue": 0.0, "alpha": 0.0 }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/prepareCallHierarchy", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/prepareTypeHierarchy", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/moniker", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("workspace/willCreateFiles", r#"{ "files": [ { "uri": "file:///tmp/wire.txt" } ] }"#),
        ("workspace/willRenameFiles", r#"{ "files": [ { "oldUri": "file:///tmp/wire.txt", "newUri": "file:///tmp/wire-2.txt" } ] }"#),
        ("workspace/willDeleteFiles", r#"{ "files": [ { "uri": "file:///tmp/wire.txt" } ] }"#),
        ("textDocument/inlayHint", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/documentSymbol", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("workspace/executeCommand", r#"{ "command": "echo" }"#),
        ("textDocument/semanticTokens/full", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/semanticTokens/range", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("textDocument/semanticTokens/full/delta", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "previousResultId": "0" }"#),
        ("textDocument/completion", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("textDocument/codeAction", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "context": { "diagnostics": [] } }"#),
        ("textDocument/diagnostic", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" } }"#),
        ("textDocument/selectionRange", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "positions": [ { "line": 0, "character": 0 } ] }"#),
        ("textDocument/inlineValue", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "context": { "frameId": 0, "stoppedLocation": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("callHierarchy/incomingCalls", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("callHierarchy/outgoingCalls", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("typeHierarchy/supertypes", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("typeHierarchy/subtypes", r#"{ "item": { "name": "f", "kind": 12, "uri": "file:///tmp/wire.txt", "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "selectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } } }"#),
        ("workspace/symbol", r#"{ "query": "" }"#),
        ("textDocument/signatureHelp", r#"{ "textDocument": { "uri": "file:///tmp/wire.txt" }, "position": { "line": 0, "character": 0 } }"#),
        ("completionItem/resolve", r#"{ "label": "l" }"#),
        ("codeAction/resolve", r#"{ "title": "t" }"#),
        ("documentLink/resolve", r#"{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("codeLens/resolve", r#"{ "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } } }"#),
        ("inlayHint/resolve", r#"{ "position": { "line": 0, "character": 0 }, "label": "l" }"#),
        ("workspaceSymbol/resolve", r#"{ "name": "f", "kind": 12, "location": { "uri": "file:///tmp/wire.txt" } }"#),
    ]
    .into_iter()
    .map(|(method, params)| (method, serde_json::from_str(params).expect("fixture parses")))
}
