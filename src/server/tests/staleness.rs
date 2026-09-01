//! Staleness over the wire: a document mutated while its request is in
//! flight answers `CONTENT_MODIFIED` and succeeds on retry.

use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use super::robustness::GatedServer;
use crate::server::testing::{WIRE_TIMEOUT, bounded, did_open, hover_params, spawn_wire_server};

#[tokio::test]
async fn stale_document_answers_content_modified_then_succeeds_on_retry() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;
    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///tmp/wire.txt", "a🙂b"),
        )
        .await;

    // Fire hover but do not await it yet: the handler is gated inside.
    client
        .send_request(
            2,
            "textDocument/hover",
            hover_params("file:///tmp/wire.txt", 1),
        )
        .await;
    // Bounded await, not `now_or_never`: on the current-thread runtime the
    // server task only runs while this task is suspended, so a single poll
    // cannot yet see the enter signal.
    timeout(WIRE_TIMEOUT, entered_rx.recv())
        .await
        .expect("handler entered")
        .expect("signal received");

    // Mutate the document while the handler is in flight.
    client
        .notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///tmp/wire.txt", "version": 2 },
                "contentChanges": [{ "text": "changed" }]
            }),
        )
        .await;
    // Notifications have no acknowledgment, and releasing right after the
    // write would race the gate against the pending didChange bytes: the
    // main loop polls in-flight tasks before unread messages, so the gated
    // hover could finish against the old version. A follow-up request's
    // response is the wire-native proof the change landed — the loop reads
    // messages in order, so once this responds, the version bump is applied.
    let _probe = client
        .request(3, "workspace/symbol", json!({ "query": "" }))
        .await;
    release_tx.send(true).expect("release sends");

    let stale = client.await_response(2).await;
    assert_eq!(stale["error"]["code"], -32801); // ContentModified

    // The retry against the new version succeeds. The watch stays latched,
    // so the retried hover passes the gate immediately.
    let retried = client
        .request(
            4,
            "textDocument/hover",
            hover_params("file:///tmp/wire.txt", 1),
        )
        .await;
    assert!(retried.get("result").is_some());

    drop(client);
    let _ = bounded(server).await;
}
