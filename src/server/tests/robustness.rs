//! Robustness over the wire: panicking handlers come back as structured
//! errors, the concurrency bound holds, cancellation answers
//! `-32800`, and malformed framing closes the connection.

use std::time::Duration;

use async_lsp::lsp_types::{
    ClientCapabilities, Hover, HoverParams, HoverProviderCapability, ServerCapabilities,
};
use rstest::rstest;
use serde_json::json;
use tokio::io::AsyncWriteExt as _;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::server::Server;
use crate::server::testing::{
    EchoServer, WIRE_TIMEOUT, bounded, did_open, echo_hover, hover_params, spawn_wire_server,
};

/// The hover advertisement every fixture that serves `hover` needs: the
/// dispatch gate rejects unadvertised methods before the handler runs.
fn hover_capabilities() -> Option<ServerCapabilities> {
    Some(ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    })
}

#[derive(Clone)]
pub(crate) struct GatedServer {
    pub(crate) entered: mpsc::UnboundedSender<()>,
    pub(crate) release: watch::Receiver<bool>,
}

impl Server for GatedServer {
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        hover_capabilities()
    }

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
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        hover_capabilities()
    }

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

#[rstest]
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
        "message was: {error:?}",
    );

    drop(client);
    let _ = bounded(server).await;
}

#[rstest]
#[tokio::test]
async fn at_most_limit_requests_run_concurrently() {
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_tx, release_rx) = watch::channel(false);
    let server_impl = GatedServer {
        entered: entered_tx,
        release: release_rx,
    };
    let (mut client, server) = spawn_wire_server(server_impl);
    client.initialize_client(&["utf-16"]).await;
    // The gated hovers must run against a TRACKED document: for an
    // untracked file URL the dispatch engine primes the fallback cache on
    // the blocking pool before the handler, so handler entry is no longer
    // a first-poll event and the limit-cohort counting below would race
    // the primes.
    client
        .notify(
            "textDocument/didOpen",
            did_open("file:///tmp/wire.txt", "wire"),
        )
        .await;

    let limit = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    // One more request than the layer admits.
    for id in 0..=i64::try_from(limit).expect("core count fits i64") {
        client
            .send_request(
                id,
                "textDocument/hover",
                hover_params("file:///tmp/wire.txt", 0),
            )
            .await;
    }

    for _ in 0..limit {
        timeout(WIRE_TIMEOUT, entered_rx.recv())
            .await
            .expect("handler entered")
            .expect("signal received");
    }
    // Absence-check: nothing enters while all the permits are held.
    timeout(Duration::from_millis(250), entered_rx.recv())
        .await
        .expect_err("the overflow handler must wait for a permit");

    release_tx.send(true).expect("release sends");

    // PR #30 (drive in-flight tasks while waiting for poll_ready) is in
    // the pinned async-lsp: the released gates free their permits and
    // the overflow handler enters and completes. If this ever regresses
    // to the overflow staying blocked, the git pin to the fix was lost
    // (a crates.io release without the fix replaced the dependency).
    timeout(WIRE_TIMEOUT, entered_rx.recv())
        .await
        .expect("the overflow handler enters after the release")
        .expect("signal received");

    // Every request answers — the limit cohort plus the overflow.
    for id in 0..=i64::try_from(limit).expect("core count fits i64") {
        let response = client.await_response(id).await;
        assert!(
            response.get("result").is_some(),
            "response {id} must succeed: {response:?}",
        );
    }

    drop(client);
    let _ = bounded(server).await;
}

#[rstest]
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

#[rstest]
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
        "the loop must fail, not exit Ok: {outcome:?}",
    );
}
