//! Lifecycle gating over the wire: the initialize handshake, its
//! exclusivity, and the shutdown wall, all through the real
//! `LifecycleLayer`.

use serde_json::json;

use crate::server::testing::{EchoServer, bounded, spawn_wire_server};

#[tokio::test]
async fn initialize_negotiates_position_encoding_end_to_end() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    // The client prefers utf-16, but also offers utf-8: the server's
    // preference order must pick utf-8 through the real JSON round trip.
    let result = client.initialize_client(&["utf-16", "utf-8"]).await;

    assert_eq!(result["capabilities"]["positionEncoding"], "utf-8");

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn requests_before_initialize_are_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    let response = client
        .request(
            1,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;

    assert_eq!(response["error"]["code"], -32002); // ServerNotInitialized

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn double_initialize_is_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    let response = client
        .request(
            2,
            "initialize",
            json!({"processId": null, "capabilities": {}}),
        )
        .await;

    assert_eq!(response["error"]["code"], -32600); // InvalidRequest

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn requests_after_shutdown_are_rejected() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;
    let shutdown = client.request(2, "shutdown", json!(null)).await;
    assert!(shutdown.get("result").is_some());

    let response = client
        .request(
            3,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await;

    assert_eq!(response["error"]["code"], -32600); // InvalidRequest

    drop(client);
    let _ = bounded(server).await;
}
