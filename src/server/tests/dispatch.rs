//! Method dispatch over the wire: methods the crate does not wire answer
//! `-32601`, after params validation; wired methods are answered by the
//! dispatch engine, never the router default.

use serde_json::{Value, json};

use crate::server::testing::{EchoServer, bounded, spawn_wire_server};

#[tokio::test]
async fn unwired_methods_return_method_not_found() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // One parametrized test over the future surface: methods the crate
    // does not wire must answer -32601 no matter how many are added.
    // Params are minimal-but-valid for each method: the router validates
    // params before dispatch, so garbage params would answer -32602 and
    // never reach the not-implemented path this test pins.
    let unwired = [
        (
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": "file:///tmp/wire.txt" } }),
        ),
        ("workspace/symbol", json!({ "query": "" })),
        (
            "textDocument/inlayHint",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 0 }
                }
            }),
        ),
    ];
    for (id, (method, params)) in unwired.into_iter().enumerate() {
        let response = client
            .request(i64::try_from(id).expect("small id") + 10, method, params)
            .await;
        assert_eq!(response["error"]["code"], -32601, "method {method}");
    }

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn wired_methods_dispatch() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // All 48 `lsp_dispatch!` rows (see `with_state`), by wire name. A
    // `Server` method without a row is the one gap the compiler cannot
    // see, and the two -32601 populations differ by producer only: the
    // router answers an unwired method itself ("No such method: ..."),
    // while a wired one always reaches the dispatch engine — whose answer
    // is a result or the trait default ("LSP method '...' has not been
    // implemented"). A router-default message therefore pins a missing
    // row. Params stay minimally valid so every request clears router-side
    // params validation and reaches the engine.
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
                "no dispatch row for {method}: the router, not the engine, answered"
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
