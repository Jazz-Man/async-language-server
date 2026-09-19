//! Workspace-diagnostics plumbing over the wire: the fire-and-forget
//! client-bound requests — capability registration, configuration
//! pulls, and diagnostic refreshes — that only exist as messages on the
//! client side of the pipe.

use async_lsp::lsp_types::{
    ClientCapabilities, DiagnosticOptions, DiagnosticServerCapabilities, ServerCapabilities,
};
use rstest::rstest;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::timeout;

use crate::error::ServerResult;
use crate::server::testing::{RawClient, WIRE_TIMEOUT, bounded, spawn_wire_server};
use crate::server::{DocumentMatcher, Server, ServerOptions, WorkspaceDiagnostics};
use crate::testing::diagnostic_provider_capabilities;

/// How long an absence check waits before concluding nothing arrives —
/// the same bound the unit-tier gated tests use.
const ABSENCE_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct RegistrationServer;

impl Server for RegistrationServer {
    fn server_options(&self) -> ServerOptions {
        ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::setting("wire"))
    }

    // `initialized` registers the configuration watcher only when the final
    // advertisement said supported, so this server must advertise the
    // provider for the registration to fire.
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(diagnostic_provider_capabilities(true, false))
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
    // state, and `can_refresh` requires both that verdict and the client's
    // refresh support. The flag is the implementor's own declaration: the
    // wrapper leaves it verbatim (only `Disabled` overrides it, as a
    // kill-switch).
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("wire-refresh".into()),
                inter_file_dependencies: false,
                workspace_diagnostics: true,
                ..Default::default()
            })),
            ..ServerCapabilities::default()
        })
    }
}

#[derive(Clone)]
struct WatcherServer;

impl Server for WatcherServer {
    // One matcher with one url glob, so the registration's watcher list is
    // a single entry and the arrival assertion is exact.
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("watched").with_url_globs(["**/*.watched"])]
    }

    // The watcher gate requires enabled diagnostics: watched-file events only
    // carry meaning for Workspace-origin documents, which exist solely under
    // an advertised, enabled provider.
    fn server_capabilities(_: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(diagnostic_provider_capabilities(true, false))
    }
}

/// The gate's negative direction: the same url-glob matcher, but no
/// diagnostic provider advertised — enabled diagnostics is false, so a
/// capable client still must not receive a watcher registration.
#[derive(Clone)]
struct UnadvertisedWatcherServer;

impl Server for UnadvertisedWatcherServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![DocumentMatcher::new("watched").with_url_globs(["**/*.watched"])]
    }
}

/// Spawns one of this file's servers and runs the `initialize` handshake
/// with the given client `capabilities` — the tail shared by all seven
/// spawn sites below. A deliberate per-file helper: the shared wire
/// harness stays free of bespoke handshake wrappers.
async fn spawn_initialized<S>(
    server: S,
    capabilities: Value,
) -> (RawClient, tokio::task::JoinHandle<ServerResult<()>>)
where
    S: Server + Clone + Send + Sync + 'static,
{
    let (mut client, server) = spawn_wire_server(server);
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
    (client, server)
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

#[rstest]
#[tokio::test]
async fn initialized_registers_did_change_configuration_when_supported() {
    let (mut client, server) = spawn_initialized(
        RegistrationServer,
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

#[rstest]
#[tokio::test]
async fn initialized_registers_file_watchers_when_supported() {
    let (mut client, server) = spawn_initialized(
        WatcherServer,
        json!({
            "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": true },
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
        registrations[0]["id"], "async-language-server.watchedFiles",
        "matchers are session-fixed, so the registration carries a fixed id",
    );
    assert_eq!(
        registrations[0]["method"], "workspace/didChangeWatchedFiles",
        "the registration watches the file-change mechanism",
    );
    let watchers = registrations[0]["registerOptions"]["watchers"]
        .as_array()
        .expect("watchers array");
    assert_eq!(watchers.len(), 2);
    assert_eq!(
        watchers[0]["globPattern"], "**/*.watched",
        "the watcher's glob is the fixture matcher's url glob",
    );
    assert_eq!(
        watchers[1]["globPattern"], "**/.gitignore",
        "the built-in .gitignore always registers: its edits change walk membership",
    );
    assert_eq!(
        watchers[0]["kind"], 7,
        "all three kinds: create, change, delete",
    );
    reply_result(&mut client, &registration, json!(null)).await;

    drop(client);
    let _ = bounded(server).await;
}

#[rstest]
#[tokio::test]
async fn initialized_skips_watcher_registration_without_support() {
    // Case 1 — the missing client capability holds the registration back:
    // the fixture matcher carries a url glob and its provider is advertised,
    // so only the capability conjunct is off.
    let (mut client, server) = spawn_initialized(WatcherServer, json!({})).await;
    assert_no_server_request(&mut client, "client/registerCapability").await;
    drop(client);
    let _ = bounded(server).await;

    // Case 2 — the gate's enabled-diagnostics conjunct: the client IS
    // capable, but this fixture advertises no provider, so watched-file
    // events would carry no meaning and no registration may be sent.
    let (mut client, server) = spawn_initialized(
        UnadvertisedWatcherServer,
        json!({
            "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": true },
            },
        }),
    )
    .await;
    assert_no_server_request(&mut client, "client/registerCapability").await;
    drop(client);
    let _ = bounded(server).await;
}

#[rstest]
#[tokio::test]
async fn configuration_reply_applies_only_if_generation_current() {
    let (mut client, server) = spawn_initialized(
        RefreshServer,
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

#[rstest]
#[tokio::test]
async fn refresh_fires_only_on_change_and_only_when_supported() {
    // A refresh-supporting client: a same-value push refreshes nothing, a
    // real change does.
    let (mut client, server) = spawn_initialized(
        RefreshServer,
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
    let (mut client, server) = spawn_initialized(RefreshServer, json!({})).await;

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
