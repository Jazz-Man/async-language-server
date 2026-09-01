//! Shared harness for the wire-tier tests: a raw JSON-RPC client speaking
//! real `Content-Length` framing over `tokio::io::duplex`, driving the
//! actual middleware stack through `run_over_streams`. The client side
//! deliberately uses no async-lsp code: it sees the exact bytes and stays
//! isolated from async-lsp client-path bugs.
//!
//! The server's halves of the duplex pipes are bridged to the futures
//! traits by the generic `TokioReader`/`TokioWriter` adapters in
//! `serve.rs` — the same ones `serve()` runs process stdio through.

use std::time::Duration;

use async_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, MarkedString, Position, Range as LspRange,
};
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader, DuplexStream, ReadHalf,
    WriteHalf, split,
};

use crate::error::ServerResult;
use crate::server::{
    Server,
    serve::{TokioReader, TokioWriter, run_over_streams},
};

pub(crate) const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) async fn bounded<F: std::future::Future>(future: F) -> F::Output {
    tokio::time::timeout(WIRE_TIMEOUT, future)
        .await
        .expect("completes within the bounded wire timeout")
}

// --- raw JSON-RPC client ---

pub(crate) struct RawClient {
    reader: BufReader<ReadHalf<DuplexStream>>,
    pub(crate) writer: WriteHalf<DuplexStream>,
    pub(crate) pending: Vec<Value>,
}

impl RawClient {
    pub(crate) async fn write_message(&mut self, message: &Value) {
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
    pub(crate) async fn read_message(&mut self) -> Option<Value> {
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

    /// Drains all remaining bytes to EOF, including any still buffered.
    pub(crate) async fn read_to_end(&mut self, out: &mut Vec<u8>) -> std::io::Result<usize> {
        self.reader.read_to_end(out).await
    }

    pub(crate) async fn send_request(&mut self, id: i64, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.write_message(&message).await;
    }

    pub(crate) async fn await_response(&mut self, id: i64) -> Value {
        // A response to this id may already have been stashed in `pending`
        // while reading past it for an earlier message.
        if let Some(position) = self
            .pending
            .iter()
            .position(|message| is_response_to(message, id))
        {
            return self.pending.remove(position);
        }
        loop {
            let message = self
                .read_message()
                .await
                .expect("connection stays open until the response arrives");
            if is_response_to(&message, id) {
                return message;
            }
            self.pending.push(message);
        }
    }

    pub(crate) async fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send_request(id, method, params).await;
        self.await_response(id).await
    }

    pub(crate) async fn notify(&mut self, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_message(&message).await;
    }

    /// Full initialize handshake; returns the `InitializeResult`.
    pub(crate) async fn initialize_client(&mut self, encodings: &[&str]) -> Value {
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

fn is_response_to(message: &Value, id: i64) -> bool {
    message.get("id").and_then(Value::as_i64) == Some(id)
        && (message.get("result").is_some() || message.get("error").is_some())
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
pub(crate) struct EchoServer;

pub(crate) fn echo_hover(position: Position) -> Option<Hover> {
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

pub(crate) fn spawn_wire_server<S>(
    server: S,
) -> (RawClient, tokio::task::JoinHandle<ServerResult<()>>)
where
    S: Server + Clone + Send + Sync + 'static,
{
    let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = split(client_stream);
    let (server_read, server_write) = split(server_stream);
    let handle = tokio::spawn(run_over_streams(
        server,
        TokioReader(server_read),
        TokioWriter(server_write),
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

// --- shared params builders ---

pub(crate) fn did_open(uri: &str, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": "test",
            "version": 1,
            "text": text
        }
    })
}

pub(crate) fn hover_params(uri: &str, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": character }
    })
}
