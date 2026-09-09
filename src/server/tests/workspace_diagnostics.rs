//! Workspace-diagnostics plumbing over the wire: the fire-and-forget
//! client-bound requests — capability registration, configuration
//! pulls, and diagnostic refreshes — that only exist as messages on the
//! client side of the pipe.

use async_lsp::lsp_types::{
    ClientCapabilities, DiagnosticOptions, DiagnosticServerCapabilities, ServerCapabilities,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::timeout;

use crate::server::testing::{RawClient, WIRE_TIMEOUT, bounded, spawn_wire_server};
use crate::server::{Server, ServerOptions, WorkspaceDiagnostics};

/// How long an absence check waits before concluding nothing arrives —
/// the same bound the unit-tier gated tests use.
const ABSENCE_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct RegistrationServer;

impl Server for RegistrationServer {
    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::setting("wire"))
    }
}

#[derive(Clone)]
struct RefreshServer;

impl Server for RefreshServer {
    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_workspace_diagnostics(
            WorkspaceDiagnostics::setting("wire").with_default_enabled(true),
        )
    }

    // The advertised provider is what makes diagnostics `supported` on the
    // state, gating `refresh_diagnostics` behind the client's refresh
    // support alone. `workspace_diagnostics: false` here is flipped to
    // true by the wrapper for the Configurable variant.
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("wire-refresh".into()),
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..Default::default()
            })),
            ..ServerCapabilities::default()
        })
    }
}

async fn initialize_with_capabilities(client: &mut RawClient, capabilities: Value) {
    let response = client
        .request(
            1,
            "initialize",
            json!({
                "processId": null,
                "capabilities": capabilities,
            }),
        )
        .await;
    assert!(
        response.get("result").is_some(),
        "initialize succeeds: {response}",
    );
    client.notify("initialized", json!({})).await;
}

/// Reads client-bound messages until a request for `method` arrives;
/// everything else is stashed for later `await_response` calls.
async fn await_server_request(client: &mut RawClient, method: &str) -> Value {
    loop {
        let message = timeout(WIRE_TIMEOUT, client.read_message())
            .await
            .expect("server request arrives within the bound")
            .expect("wire stays open");
        if message.get("method").and_then(Value::as_str) == Some(method) {
            return message;
        }
        client.pending.push(message);
    }
}

/// Bounded absence check: within the window, no request for `method` may
/// arrive (any other message is stashed).
async fn assert_no_server_request(client: &mut RawClient, method: &str) {
    let arrived = timeout(ABSENCE_TIMEOUT, client.read_message()).await;
    if let Ok(Some(message)) = arrived {
        assert_ne!(
            message.get("method").and_then(Value::as_str),
            Some(method),
            "{method} must not arrive",
        );
        client.pending.push(message);
    }
}

async fn reply_result(client: &mut RawClient, request: &Value, result: Value) {
    client
        .write_message(&json!({
            "jsonrpc": "2.0",
            "id": request["id"].clone(),
            "result": result,
        }))
        .await;
}

#[tokio::test]
async fn initialized_registers_did_change_configuration_when_supported() {
    let (mut client, server) = spawn_wire_server(RegistrationServer);
    initialize_with_capabilities(
        &mut client,
        json!({
            "workspace": {
                "didChangeConfiguration": { "dynamicRegistration": true },
            },
        }),
    )
    .await;

    let registration = await_server_request(&mut client, "client/registerCapability").await;
    let registrations = registration["params"]["registrations"]
        .as_array()
        .expect("registrations array");
    assert_eq!(registrations.len(), 1);
    assert_eq!(
        registrations[0]["method"], "workspace/didChangeConfiguration",
        "the registration watches the configuration change mechanism",
    );
    assert_eq!(
        registrations[0]["registerOptions"]["section"], "wire",
        "the registration must watch the configured setting's section",
    );
    reply_result(&mut client, &registration, json!(null)).await;

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn configuration_reply_applies_only_if_generation_current() {
    let (mut client, server) = spawn_wire_server(RefreshServer);
    initialize_with_capabilities(
        &mut client,
        json!({
            "workspace": {
                "configuration": true,
                "diagnostic": { "refreshSupport": true },
            },
        }),
    )
    .await;

    let first = await_server_request(&mut client, "workspace/configuration").await;

    // Supersede the in-flight request: a settings push bumps the
    // generation and applies the same value, so it emits no wire traffic.
    client
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": { "wire": true } }),
        )
        .await;

    // The superseded reply must be dropped: applying it would flip the
    // flag and trigger a refresh for a value that was already superseded.
    reply_result(&mut client, &first, json!([false])).await;
    assert_no_server_request(&mut client, "workspace/diagnostic/refresh").await;

    // An empty settings push carries no value for the section, so the
    // wrapper re-requests the configuration; this reply is current.
    client
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": {} }),
        )
        .await;
    let second = await_server_request(&mut client, "workspace/configuration").await;
    reply_result(&mut client, &second, json!([false])).await;

    let refresh = await_server_request(&mut client, "workspace/diagnostic/refresh").await;
    reply_result(&mut client, &refresh, json!(null)).await;

    drop(client);
    let _ = bounded(server).await;
}

#[tokio::test]
async fn refresh_fires_only_on_change_and_only_when_supported() {
    // A refresh-supporting client: a same-value push refreshes nothing, a
    // real change does.
    let (mut client, server) = spawn_wire_server(RefreshServer);
    initialize_with_capabilities(
        &mut client,
        json!({
            "workspace": { "diagnostic": { "refreshSupport": true } },
        }),
    )
    .await;

    client
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": { "wire": true } }),
        )
        .await;
    assert_no_server_request(&mut client, "workspace/diagnostic/refresh").await;

    client
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": { "wire": false } }),
        )
        .await;
    let refresh = await_server_request(&mut client, "workspace/diagnostic/refresh").await;
    reply_result(&mut client, &refresh, json!(null)).await;
    drop(client);
    let _ = bounded(server).await;

    // A client without refresh support never sees the refresh request.
    let (mut client, server) = spawn_wire_server(RefreshServer);
    initialize_with_capabilities(&mut client, json!({})).await;

    client
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": { "wire": false } }),
        )
        .await;
    assert_no_server_request(&mut client, "workspace/diagnostic/refresh").await;

    drop(client);
    let _ = bounded(server).await;
}
