//! Termination and server→client requests: the shutdown exit terminates
//! the loop cleanly, and `workspace/configuration` is served mid-request.

use async_lsp::lsp_types::{
    ClientCapabilities, DiagnosticOptions, DiagnosticServerCapabilities, ServerCapabilities,
};
use serde_json::{Value, json};
use tokio::time::timeout;

use crate::server::Server;
use crate::server::testing::{EchoServer, WIRE_TIMEOUT, bounded, spawn_wire_server};

#[derive(Clone)]
struct ConfigurableServer;

impl Server for ConfigurableServer {
    fn server_options(&self) -> crate::server::ServerOptions {
        crate::server::ServerOptions::default()
            .with_workspace_diagnostics(crate::server::WorkspaceDiagnostics::setting("wireTest"))
    }

    // Without an advertised diagnostic provider the wrapper reports
    // workspace diagnostics as unsupported (-32601) and the test below
    // would never see a report. `workspace_diagnostics: false` here is
    // flipped to true by the wrapper for the Configurable variant.
    fn server_capabilities(_client: ClientCapabilities) -> Option<ServerCapabilities> {
        Some(ServerCapabilities {
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("wire-test".into()),
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..Default::default()
            })),
            ..ServerCapabilities::default()
        })
    }
}

#[tokio::test]
async fn shutdown_exit_terminates_the_server_loop_cleanly() {
    let (mut client, server) = spawn_wire_server(EchoServer);
    client.initialize_client(&["utf-16"]).await;

    let shutdown = client.request(2, "shutdown", json!(null)).await;
    assert!(shutdown.get("result").is_some());
    client.notify("exit", json!(null)).await;

    bounded(server)
        .await
        .expect("serve loop completes within the timeout")
        .expect("serve loop resolves Ok(())");

    // EOF is the expected termination, not a hang: read until close.
    let mut raw = Vec::new();
    timeout(WIRE_TIMEOUT, client.read_to_end(&mut raw))
        .await
        .expect("server closes the wire")
        .expect("read succeeds");
    assert!(raw.is_empty(), "no trailing bytes after exit");
}

#[tokio::test]
async fn workspace_configuration_request_is_served_mid_request() {
    let root = {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-wire-config-{millis}"));
        tokio::fs::create_dir_all(&root)
            .await
            .expect("temp workspace can be created");
        root
    };

    let (mut client, server) = spawn_wire_server(ConfigurableServer);

    // initialize with configuration capability + a workspace folder
    let response = client
        .request(
            1,
            "initialize",
            json!({
                "processId": null,
                "capabilities": { "workspace": { "configuration": true } },
                "workspaceFolders": [{ "uri": format!("file://{}", root.display()), "name": "root" }]
            }),
        )
        .await;
    assert!(
        response.get("result").is_some(),
        "initialize succeeds: {response}"
    );
    client.notify("initialized", json!({})).await;

    client
        .send_request(
            2,
            "workspace/diagnostic",
            json!({ "previousResultIds": [] }),
        )
        .await;

    // The server asks for its setting mid-flight; answer from the raw side.
    let configuration_request = loop {
        let message = timeout(WIRE_TIMEOUT, client.read_message())
            .await
            .expect("configuration request arrives")
            .expect("wire stays open");
        if message.get("method").is_some_and(Value::is_string) {
            break message;
        }
        client.pending.push(message);
    };
    assert_eq!(configuration_request["method"], "workspace/configuration");
    let request_id = configuration_request["id"].clone();
    client
        .write_message(&json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": [true]
        }))
        .await;

    let report = client.await_response(2).await;
    assert!(
        report.get("result").is_some(),
        "diagnostics report returns: {report}"
    );

    drop(client);
    let _ = bounded(server).await;
    tokio::fs::remove_dir_all(root)
        .await
        .expect("temp workspace can be removed");
}
