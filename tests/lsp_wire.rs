//! Black-box tests through the only general public entry point: `serve()`
//! over a real TCP socket (`Transport::Socket`).

use std::time::Duration;

use async_language_server::server::{Server, ServerError, ServerResult, Transport, serve};
use async_lsp::lsp_types::{Hover, HoverContents, HoverParams, MarkedString, Range};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufStream};
use tokio::net::TcpStream;
use tokio::time::timeout;

const WIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct EchoServer;

impl Server for EchoServer {
    fn hover(
        &self,
        _state: async_language_server::server::ServerState,
        params: HoverParams,
    ) -> impl Future<Output = ServerResult<Option<Hover>>> + Send {
        let position = params.text_document_position_params.position;
        async move {
            Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String("echo".into())),
                range: Some(Range::new(position, position)),
            }))
        }
    }
}

// The client helpers are fallible and the `expect()` calls live inside the
// `#[tokio::test]` functions: `expect_used` is deny crate-wide and only
// relaxed for code clippy can see is a test (`clippy.toml`,
// `allow-expect-in-tests`), which does not cover helpers in integration
// test crates.
type WireError = Box<dyn std::error::Error + Send + Sync>;

struct RawClient {
    stream: BufStream<TcpStream>,
}

impl RawClient {
    async fn write_message(&mut self, message: &Value) -> Result<(), WireError> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stream.write_all(header.as_bytes()).await?;
        self.stream.write_all(body.as_bytes()).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn read_message(&mut self) -> Result<Option<Value>, WireError> {
        let mut content_length = None;
        let mut line = Vec::new();
        loop {
            line.clear();
            if self.stream.read_until(b'\n', &mut line).await? == 0 {
                return Ok(None);
            }
            let trimmed = trim_crlf(&line);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = std::str::from_utf8(trimmed)
                .map_err(|_| "header line is not UTF-8")?
                .strip_prefix("Content-Length: ")
            {
                content_length = Some(value.trim().parse::<usize>()?);
            }
        }
        let length = content_length.ok_or("Content-Length header missing")?;
        let mut body = vec![0u8; length];
        self.stream.read_exact(&mut body).await?;
        Ok(Some(serde_json::from_slice(&body)?))
    }

    async fn request(&mut self, id: i64, method: &str, params: Value) -> Result<Value, WireError> {
        self.write_message(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
        .await?;
        loop {
            let message = match timeout(WIRE_TIMEOUT, self.read_message()).await? {
                Ok(Some(message)) => message,
                Ok(None) => return Err("connection closed by the server".into()),
                Err(error) => return Err(error),
            };
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return Ok(message);
            }
        }
    }
}

fn trim_crlf(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

#[tokio::test]
async fn socket_connect_failure_maps_to_tcp_connect_error() {
    // Bind, learn the port, drop the listener: nothing listens there.
    // (A rare ephemeral-port reuse race remains; if it flakes, re-run once
    // before investigating — it is not a code defect.)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let error = timeout(WIRE_TIMEOUT, serve(Transport::Socket(port), EchoServer))
        .await
        .expect("fails within the bound")
        .expect_err("connect fails");
    assert!(
        matches!(error, ServerError::TcpConnect { port: p, .. } if p == port),
        "was: {error:?}"
    );
}

#[tokio::test]
async fn serve_happy_path_over_tcp_resolves_ok() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let serve_handle = tokio::spawn(serve(Transport::Socket(port), EchoServer));
    let (stream, _) = listener.accept().await.expect("server connects");
    let mut client = RawClient {
        stream: BufStream::new(stream),
    };

    let initialize = client
        .request(
            1,
            "initialize",
            json!({"processId": null, "capabilities": {}}),
        )
        .await
        .expect("initialize round-trips");
    assert!(initialize.get("result").is_some(), "{initialize}");
    client
        .write_message(&json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}))
        .await
        .expect("initialized notification sends");

    let hover = client
        .request(
            2,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///tmp/tcp.txt" },
                "position": { "line": 0, "character": 0 }
            }),
        )
        .await
        .expect("hover round-trips");
    assert!(hover.get("result").is_some(), "{hover}");

    let shutdown = client
        .request(3, "shutdown", json!(null))
        .await
        .expect("shutdown round-trips");
    assert!(shutdown.get("result").is_some());
    client
        .write_message(&json!({"jsonrpc": "2.0", "method": "exit"}))
        .await
        .expect("exit notification sends");

    serve_handle
        .await
        .expect("task joins")
        .expect("serve() resolves Ok(())");
}
