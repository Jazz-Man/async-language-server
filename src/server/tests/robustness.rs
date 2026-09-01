//! Robustness over the wire: panicking handlers come back as structured
//! errors, the concurrency bound holds, cancellation answers
//! `-32800`, and malformed framing closes the connection.

use std::time::Duration;

use async_lsp::lsp_types::{Hover, HoverParams};
use serde_json::json;
use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::server::Server;
use crate::server::testing::{
    EchoServer, WIRE_TIMEOUT, bounded, echo_hover, hover_params, spawn_wire_server,
};

#[derive(Clone)]
pub(crate) struct GatedServer {
    pub(crate) entered: mpsc::UnboundedSender<()>,
    pub(crate) release: watch::Receiver<bool>,
}

impl Server for GatedServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        let entered = self.entered.clone();
        let mut release = self.release.clone();
        async move {
            let _ = entered.send(());
            while !*release.borrow_and_update() {
                release
                    .changed()
                    .await
                    .expect("release channel stays alive");
            }
            Ok(echo_hover(params.text_document_position_params.position))
        }
    }
}

#[derive(Clone)]
struct PanickingServer;

/// `None` behind a function boundary: `unnecessary_literal_unwrap` only
/// tracks values it can see constructed at the call site.
fn no_hover() -> Option<Hover> {
    None
}

impl Server for PanickingServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        _params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        // Trigger the panic through `expect` (allowed in tests) rather
        // than `panic!`, which `panic_in_result_fn` rejects in a
        // Result-returning function even under -D warnings. The `None`
        // comes from a helper so `unnecessary_literal_unwrap` cannot flag
        // the `expect`, and the binding before the block keeps
        // `manual_async_fn` quiet.
        let nothing = no_hover();
        async move { Ok(Some(nothing.expect("intentional test panic"))) }
    }
}

#[tokio::test]
async fn panicking_handler_returns_structured_error() {
    let (mut client, server) = spawn_wire_server(PanickingServer);
    client.initialize_client(&["utf-16"]).await;

    let response = client
        .request(
            2,
            "textDocument/hover",
            hover_params("file:///tmp/wire.txt", 0),
        )
        .await;

    let error = response["error"].as_object().expect("error, not a hang");
    assert!(
        error["message"]
            .as_str()
            .expect("message is a string")
            .contains("panicked"),
        "message was: {error:?}"
    );

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn at_most_eight_requests_run_concurrently() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;

    for id in 10..19 {
        client
            .send_request(
                id,
                "textDocument/hover",
                hover_params("file:///tmp/wire.txt", 0),
            )
            .await;
    }

    for _ in 0..8 {
        timeout(WIRE_TIMEOUT, entered_rx.recv())
            .await
            .expect("handler entered")
            .expect("signal received");
    }
    // Absence-check: nothing enters while all eight permits are held.
    timeout(Duration::from_millis(250), entered_rx.recv())
        .await
        .expect_err("the ninth handler must wait for a permit");

    release_tx.send(true).expect("release sends");

    // Tripwire: the release must NOT admit the ninth handler. With
    // ConcurrencyLayer at capacity, async-lsp 0.2.4's MainLoop stops
    // polling in-flight tasks while waiting for poll_ready
    // (https://github.com/oxalica/async-lsp/pull/30), so the gated
    // futures never observe the release and the permits never free.
    // When this absence-check starts failing after an async-lsp
    // upgrade, the upstream fix has landed: flip it to asserting the
    // ninth handler enters and completes, await the nine responses
    // again, and restore the `bounded(server)` teardown.
    timeout(Duration::from_millis(250), entered_rx.recv())
        .await
        .expect_err("the ninth handler is still blocked after release: did upstream PR #30 land?");

    // Upstream deadlock (see above): the join handle can never complete,
    // so abort the task instead of awaiting it.
    server.abort();
}

#[tokio::test]
async fn cancel_request_answers_request_cancelled() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;

    client
        .send_request(
            2,
            "textDocument/hover",
            hover_params("file:///tmp/wire.txt", 0),
        )
        .await;
    // Bounded await, not `now_or_never`: on the current-thread runtime the
    // server task only runs while this task is suspended, so a single poll
    // cannot yet see the enter signal.
    timeout(WIRE_TIMEOUT, entered_rx.recv())
        .await
        .expect("handler entered")
        .expect("signal received");

    // Cancel while the handler is still gated. The response is awaited
    // before any release: async-lsp's main loop polls in-flight tasks
    // before unread messages, so releasing first would let the gated
    // handler complete Ok and turn the cancel into a no-op.
    client.notify("$/cancelRequest", json!({ "id": 2 })).await;
    let response = client.await_response(2).await;
    assert_eq!(response["error"]["code"], -32800); // RequestCancelled

    // Cleanup only: the aborted handler never ran to completion, so the
    // watch is released merely to prove the channel is still alive.
    release_tx.send(true).expect("release sends");

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn malformed_header_closes_the_connection() {
    let (mut client, server) = spawn_wire_server(EchoServer);

    client
        .writer
        .write_all(b"Content-Length: abc\r\n\r\n")
        .await
        .expect("garbage writes");

    // The loop fails on framing and closes: EOF within the bound.
    let closed = timeout(WIRE_TIMEOUT, client.read_message())
        .await
        .expect("server reacts within the bound");
    assert!(closed.is_none(), "expected EOF, got {closed:?}");

    // The join handle adds a layer over `ServerResult`: unwrap it first,
    // then assert the loop itself failed (framing error, not a panic).
    let outcome = bounded(server)
        .await
        .expect("serve loop completes within the timeout");
    assert!(
        outcome.is_err(),
        "the loop must fail, not exit Ok: {outcome:?}"
    );
}
