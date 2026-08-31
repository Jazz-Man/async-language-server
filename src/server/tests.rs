//! Wire-tier tests: a raw JSON-RPC client speaking real `Content-Length`
//! framing over `tokio::io::duplex`, driving the actual middleware stack
//! through `run_over_streams`. The client side deliberately uses no
//! async-lsp code: it sees the exact bytes and stays isolated from
//! async-lsp client-path bugs.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use async_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, MarkedString, Position, Range as LspRange,
};
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt as _, AsyncRead as _, AsyncReadExt as _, AsyncWrite as _, AsyncWriteExt as _,
    BufReader, DuplexStream, ReadBuf, ReadHalf, WriteHalf, split,
};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::error::ServerResult;
use crate::server::{Server, serve::run_over_streams};

const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

async fn bounded<F: std::future::Future>(future: F) -> F::Output {
    tokio::time::timeout(WIRE_TIMEOUT, future)
        .await
        .expect("completes within the bounded wire timeout")
}

// --- tokio → futures adapters (server side only; modeled on transport.rs) ---

struct FuturesReadHalf(tokio::io::ReadHalf<DuplexStream>);

impl futures::AsyncRead for FuturesReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut self.get_mut().0).poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        }
    }
}

struct FuturesWriteHalf(tokio::io::WriteHalf<DuplexStream>);

impl futures::AsyncWrite for FuturesWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

// --- raw JSON-RPC client ---

struct RawClient {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    /// Server-initiated messages seen while waiting for a response.
    pending: Vec<Value>,
}

impl RawClient {
    async fn write_message(&mut self, message: &Value) {
        tokio::time::timeout(WIRE_TIMEOUT, async {
            let body = serde_json::to_string(message).expect("message serializes");
            self.writer
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await
                .expect("header writes");
            self.writer
                .write_all(body.as_bytes())
                .await
                .expect("body writes");
            self.writer.flush().await.expect("flushes");
        })
        .await
        .expect("writes complete within the bounded wire timeout");
    }

    /// Reads one framed message; `None` on EOF (server closed the wire).
    async fn read_message(&mut self) -> Option<Value> {
        tokio::time::timeout(WIRE_TIMEOUT, async {
            let mut content_length = None;
            let mut line = Vec::new();
            loop {
                line.clear();
                if self
                    .reader
                    .read_until(b'\n', &mut line)
                    .await
                    .expect("header reads")
                    == 0
                {
                    return None; // EOF
                }
                let trimmed = trim_crlf(&line);
                if trimmed.is_empty() {
                    break;
                }
                if let Some(value) = trimmed.strip_prefix(b"Content-Length: ") {
                    let value = std::str::from_utf8(value).expect("length header is ASCII");
                    content_length = Some(value.parse::<usize>().expect("length parses"));
                }
            }
            let len = content_length.expect("Content-Length header present");
            let mut body = vec![0u8; len];
            self.reader.read_exact(&mut body).await.expect("body reads");
            Some(serde_json::from_slice(&body).expect("body is JSON"))
        })
        .await
        .expect("reads complete within the bounded wire timeout")
    }

    async fn send_request(&mut self, id: i64, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.write_message(&message).await;
    }

    async fn await_response(&mut self, id: i64) -> Value {
        loop {
            let message = self
                .read_message()
                .await
                .expect("connection stays open until the response arrives");
            if message.get("id").and_then(Value::as_i64) == Some(id)
                && (message.get("result").is_some() || message.get("error").is_some())
            {
                return message;
            }
            self.pending.push(message);
        }
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send_request(id, method, params).await;
        self.await_response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_message(&message).await;
    }

    /// Full initialize handshake; returns the `InitializeResult`.
    async fn initialize_client(&mut self, encodings: &[&str]) -> Value {
        let response = self
            .request(
                1,
                "initialize",
                json!({
                    "processId": null,
                    "capabilities": {
                        "general": { "positionEncodings": encodings }
                    }
                }),
            )
            .await;
        self.notify("initialized", json!({})).await;
        response["result"].clone()
    }
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

// --- test servers ---

#[derive(Clone)]
struct EchoServer;

fn echo_hover(position: Position) -> Option<Hover> {
    Some(Hover {
        contents: HoverContents::Scalar(MarkedString::String("echo".into())),
        range: Some(LspRange::new(position, position)),
    })
}

impl Server for EchoServer {
    fn hover(
        &self,
        _state: crate::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = crate::server::ServerResult<Option<Hover>>> + Send {
        let position = params.text_document_position_params.position;
        async move { Ok(echo_hover(position)) }
    }
}

// --- wiring ---

fn spawn_wire_server<S>(server: S) -> (RawClient, tokio::task::JoinHandle<ServerResult<()>>)
where
    S: Server + Clone + Send + Sync + 'static,
{
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = split(client_stream);
    let (server_read, server_write) = split(server_stream);
    let handle = tokio::spawn(run_over_streams(
        server,
        FuturesReadHalf(server_read),
        FuturesWriteHalf(server_write),
    ));
    (
        RawClient {
            reader: BufReader::new(client_read),
            writer: client_write,
            pending: Vec::new(),
        },
        handle,
    )
}

// --- catalog #1 ---

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

// --- catalog #2-#6 ---

fn did_open(uri: &str, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": "test",
            "version": 1,
            "text": text
        }
    })
}

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

// --- catalog #7-#9 (gated servers) ---

#[derive(Clone)]
struct GatedServer {
    entered: mpsc::UnboundedSender<u64>,
    release: watch::Receiver<bool>,
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
            let _ = entered.send(1);
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

fn hover_params(uri: &str, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": character }
    })
}

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
            .expect("eight handlers enter")
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
