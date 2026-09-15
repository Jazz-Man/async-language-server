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
fn inventory_covers_exactly_the_non_resolve_dispatch_rows() {
    use crate::server::inventory::METHOD_NAMES;

    let resolve = [
        "completion_resolve",
        "code_action_resolve",
        "link_resolve",
        "code_lens_resolve",
        "inlay_hint_resolve",
        "workspace_symbol_resolve",
    ];
    for (_, trait_method, _) in wired_requests() {
        let present = METHOD_NAMES.contains(&trait_method);
        assert_eq!(
            present,
            !resolve.contains(&trait_method),
            "{trait_method}: exactly the non-resolve rows are inventoried",
        );
    }
}
