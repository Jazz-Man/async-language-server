//! Position-encoding conversion over the wire: UTF-16 round trips through
//! real serialization and incremental `didChange` application on the
//! edited document.

use serde_json::json;

use crate::server::testing::{EchoServer, bounded, did_open, spawn_wire_server};

#[tokio::test]
async fn utf16_positions_round_trip_through_real_serialization() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    // "a🙂b": the smiley is 1 UTF-16 unit pair (cols 1-2), so 'b' sits at
    // UTF-16 col 3 but UTF-8 byte 5. The handler must see UTF-8 and the
    // wire response must come back as UTF-16.
    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///tmp/wire.txt", "a🙂b"),
        )
        .await;

    let response = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 3 }
            }),
        )
        .await;

    assert_eq!(response["result"]["range"]["start"]["character"], 3);
    assert_eq!(response["result"]["range"]["end"]["character"], 3);

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn incremental_did_change_applies_over_the_wire() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///tmp/wire.txt", "a🙂b"),
        )
        .await;
    // Insert "xy" at UTF-16 col 3 (before 'b'): text becomes "a🙂xyb".
    client
        .notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt", "version": 2 },
                "contentChanges": [
                    { "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 0, "character": 3 } }, "text": "xy" }
                ]
            }),
        )
        .await;

    // Hover at the new end of line: UTF-16 col 6 must exist in the edited
    // document and round-trip back as col 6.
    let response = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt" },
                "position": { "line": 0, "character": 6 }
            }),
        )
        .await;

    assert_eq!(response["result"]["range"]["start"]["character"], 6);

    drop(client);
    let _ = bounded(server).await;
}
