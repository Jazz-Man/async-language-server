//! Method dispatch over the wire: methods the crate does not wire answer
//! `-32601`, after params validation.

use serde_json::json;

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
