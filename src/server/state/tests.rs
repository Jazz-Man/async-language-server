use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use async_lsp::{
    ClientSocket,
    lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, Position, Range, TextDocumentContentChangeEvent,
        TextDocumentIdentifier, TextDocumentItem, Url, VersionedTextDocumentIdentifier,
        WorkspaceFolder,
    },
};

use crate::server::{DocumentMatcher, Server, ServerOptions, WorkspaceDiagnostics};

use super::ServerState;

struct TestServer;

impl Server for TestServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        vec![
            DocumentMatcher::new("Test")
                .with_url_globs(["**/*.test", "*.test"])
                .with_lang_strings(["test"]),
        ]
    }
}

fn url(path: &str) -> Url {
    Url::parse(&format!("file:///tmp/{path}")).unwrap()
}

fn open_document(state: &mut ServerState, uri: Url, text: impl Into<String>) {
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri, "test".into(), 1, text.into()),
    });
}

fn temp_workspace(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after epoch")
        .as_millis();
    let root = std::env::temp_dir().join(format!("async-language-server-state-{name}-{millis}"));
    fs::create_dir_all(&root).expect("temp workspace can be created");
    root
}

fn workspace_folder(path: &PathBuf) -> WorkspaceFolder {
    let uri = Url::from_file_path(path).expect("path can be converted to a URL");
    WorkspaceFolder {
        uri,
        name: "test".into(),
    }
}

#[test]
fn full_content_change_replaces_document_text() {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let uri = url("full-change.txt");
    open_document(&mut state, uri.clone(), "old");

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new".into(),
        }],
    });

    assert_eq!(state.document(&uri).unwrap().text_contents(), "new");
    assert_eq!(state.document(&uri).unwrap().version(), 2);
}

#[test]
fn workspace_documents_have_no_lsp_version() {
    let root = temp_workspace("workspace-version");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .expect("workspace documents can be refreshed");

    assert_eq!(urls.len(), 1);
    assert_eq!(state.document(&urls[0]).unwrap().text_contents(), "disk");
    assert_eq!(state.document_workspace_version(&urls[0]), None);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn workspace_refresh_preserves_open_documents() {
    let root = temp_workspace("open-document");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(manifest).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");

    let urls = state
        .refresh_workspace_documents()
        .expect("workspace documents can be refreshed");

    assert_eq!(urls, vec![uri.clone()]);
    assert_eq!(state.document(&uri).unwrap().text_contents(), "open");
    assert_eq!(state.document_workspace_version(&uri), Some(1));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn closing_workspace_documents_keeps_disk_snapshot() {
    let root = temp_workspace("close-workspace-document");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(manifest).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");

    let _ = state.handle_document_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });

    assert_eq!(state.document(&uri).unwrap().text_contents(), "disk");
    assert_eq!(state.document_workspace_version(&uri), None);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn closing_workspace_documents_removes_them_when_workspace_diagnostics_are_disabled() {
    let root = temp_workspace("close-disabled-workspace-document");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(manifest).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::disabled()),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");

    let _ = state.handle_document_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });

    assert!(state.document(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn closing_non_workspace_documents_removes_them() {
    let root = temp_workspace("close-non-workspace-document");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(manifest).expect("test file can be canonicalized");
    let uri = Url::from_file_path(manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    open_document(&mut state, uri.clone(), "open");

    let _ = state.handle_document_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });

    assert!(state.document(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn failed_incremental_change_keeps_document_when_reread_fails() {
    let root = temp_workspace("keep-last-known");
    let uri = {
        let path = root.join("missing.test");
        Url::from_file_path(path).expect("path can be converted to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    open_document(&mut state, uri.clone(), "original");

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: 2,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            // Out-of-bounds line makes the incremental application fail
            // (columns are clamped); no file on disk means re-read fails.
            range: Some(Range::new(Position::new(50, 0), Position::new(50, 1))),
            range_length: None,
            text: "x".into(),
        }],
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), "original");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[cfg(feature = "tree-sitter")]
#[test]
fn failed_incremental_change_reparses_kept_text_tree() {
    struct JsonServer;

    impl Server for JsonServer {
        fn server_document_matchers() -> Vec<DocumentMatcher> {
            vec![
                DocumentMatcher::new("json")
                    .with_url_globs(["**/*.json"])
                    .with_lang_grammar(tree_sitter_json::LANGUAGE.into()),
            ]
        }
    }

    let root = temp_workspace("keep-last-known-tree");
    let uri = {
        let path = root.join("missing.json");
        Url::from_file_path(path).expect("path can be converted to a URL")
    };
    let mut state = ServerState::with_options::<JsonServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "json".into(), 1, r#"{"a": 1}"#.into()),
    });

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri.clone(),
            version: 2,
        },
        content_changes: vec![
            TextDocumentContentChangeEvent {
                // Applies: replaces the number with a nested object.
                range: Some(Range::new(Position::new(0, 6), Position::new(0, 7))),
                range_length: None,
                text: "{}".into(),
            },
            TextDocumentContentChangeEvent {
                // Out-of-bounds line fails the batch, and the re-read
                // fails too (the file was never written), keeping the text.
                range: Some(Range::new(Position::new(50, 0), Position::new(50, 1))),
                range_length: None,
                text: "x".into(),
            },
        ],
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), r#"{"a": {}}"#);
    // The kept tree must be a fresh parse of the kept text: a stale,
    // edited-but-never-reparsed tree still holds the old `number` node.
    let numbers = document
        .query("(number) @n")
        .expect("query runs against the kept tree");
    assert!(numbers.is_empty(), "stale tree captures: {numbers:?}");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_replaces_text_from_params() {
    let root = temp_workspace("save-from-params");
    let uri = {
        let path = root.join("saved.txt");
        Url::from_file_path(path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: Some("after".into()),
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), "after");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_falls_back_to_disk_when_params_have_no_text() {
    let root = temp_workspace("save-from-disk");
    let path = root.join("on-disk.txt");
    fs::write(&path, "from disk").expect("file can be written");
    let uri = Url::from_file_path(&path).expect("path converts to a URL");
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), "from disk");

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_removes_the_document_when_no_text_and_no_file() {
    let root = temp_workspace("save-removes");
    let uri = {
        let path = root.join("missing.txt");
        Url::from_file_path(path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    });

    assert!(
        state.document(&uri).is_none(),
        "document is removed on failure"
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
