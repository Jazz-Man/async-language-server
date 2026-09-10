use super::{DocumentOrigin, ServerState};
use crate::lsp_requests::{Request, SemanticTokensFullRequest};
use crate::server::{DocumentMatcher, Server, ServerOptions, WorkspaceDiagnostics};
use crate::testing::{open_document, temp_workspace, token, url, workspace_folder};
use crate::text_utils::Encoding;
use async_lsp::ClientSocket;
use async_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, FileChangeType, FileDelete, FileEvent,
    FileRename, Position, Range, SemanticTokens, SemanticTokensResult,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Url,
    VersionedTextDocumentIdentifier, WorkspaceFoldersChangeEvent,
};
use std::fs;

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

/// Serves the shared json matchers, so `.json` documents parse with the
/// tree-sitter json grammar through the store's normal install path.
#[cfg(feature = "tree-sitter")]
struct JsonServer;

#[cfg(feature = "tree-sitter")]
impl Server for JsonServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        crate::testing::json_matchers()
    }
}

/// Seeds the semantic-tokens delta cache the way a full response does,
/// mirroring the request-level conversion tests' flow.
fn seed_semantic_tokens(state: &ServerState, uri: &Url) {
    let document = state.document(uri).expect("document is tracked");
    let mut response = Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: Some("r1".into()),
        data: vec![token(0, 0, 4), token(0, 4, 3)],
    }));
    <SemanticTokensFullRequest as Request>::modify_response(state, &document, &mut response);
}

#[test]
fn document_clones_keep_their_snapshot_across_changes() {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let uri = url("snapshot.txt");
    open_document(&mut state, uri.clone(), "before");

    let before = state.document(&uri).expect("document is tracked");
    let text_before = before.text_contents();

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "completely new contents".into(),
        }],
    });

    let after = state.document(&uri).expect("document still tracked");
    assert_eq!(before.text_contents(), text_before);
    assert_eq!(after.version(), 2);
    assert_ne!(after.text_contents(), text_before);
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

#[tokio::test]
async fn workspace_documents_have_no_lsp_version() {
    let root = temp_workspace("state", "workspace-version");
    let manifest = root.join("a.test");
    fs::write(&manifest, "disk").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");

    assert_eq!(urls.len(), 1);
    assert_eq!(state.document(&urls[0]).unwrap().text_contents(), "disk");
    assert_eq!(state.document_workspace_version(&urls[0]), None);

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn workspace_refresh_preserves_open_documents() {
    let root = temp_workspace("state", "open-document");
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
        .await
        .expect("workspace documents can be refreshed");

    assert_eq!(urls, vec![uri.clone()]);
    assert_eq!(state.document(&uri).unwrap().text_contents(), "open");
    assert_eq!(state.document_workspace_version(&uri), Some(1));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn workspace_refresh_rereads_changed_files_and_keeps_untouched_ones() {
    let root = temp_workspace("state", "refresh-stamp-gate");
    let changed_path = root.join("a.test");
    fs::write(&changed_path, "short").expect("test file can be written");
    let stable_path = root.join("b.test");
    fs::write(&stable_path, "stable").expect("test file can be written");
    let changed = fs::canonicalize(&changed_path).expect("test file can be canonicalized");
    let changed_uri = Url::from_file_path(&changed).expect("path can be converted to a URL");
    let stable = fs::canonicalize(&stable_path).expect("test file can be canonicalized");
    let stable_uri = Url::from_file_path(&stable).expect("path can be converted to a URL");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls.len(), 2);
    assert_eq!(
        state
            .document(&changed_uri)
            .expect("changed file is tracked")
            .text_contents(),
        "short",
    );

    // Longer content: the size half of the stamp cannot match, so the gate
    // must re-read even if the modification time stayed the same.
    fs::write(&changed_path, "a longer replacement").expect("test file can be written");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");

    assert_eq!(
        urls.len(),
        2,
        "untouched files stay tracked across refreshes",
    );
    assert_eq!(
        state
            .document(&changed_uri)
            .expect("changed file stays tracked")
            .text_contents(),
        "a longer replacement",
    );
    assert_eq!(
        state
            .document(&stable_uri)
            .expect("untouched file stays tracked")
            .text_contents(),
        "stable",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn closing_workspace_documents_keeps_disk_snapshot() {
    let root = temp_workspace("state", "close-workspace-document");
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
    let root = temp_workspace("state", "close-disabled-workspace-document");
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
    let root = temp_workspace("state", "close-non-workspace-document");
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
fn did_close_evicts_cached_semantic_tokens() {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let uri = url("tokens-close.txt");
    open_document(&mut state, uri.clone(), "body");
    seed_semantic_tokens(&state, &uri);
    assert!(state.cached_semantic_tokens(&uri).is_some());

    let _ = state.handle_document_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });

    assert!(state.document(&uri).is_none());
    assert!(state.cached_semantic_tokens(&uri).is_none());
}

#[test]
fn did_close_keeps_disk_snapshot_but_evicts_cached_semantic_tokens() {
    let root = temp_workspace("state", "close-evict-workspace");
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
    seed_semantic_tokens(&state, &uri);
    assert!(state.cached_semantic_tokens(&uri).is_some());

    let _ = state.handle_document_close(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });

    assert_eq!(state.document(&uri).unwrap().text_contents(), "disk");
    assert!(state.cached_semantic_tokens(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn failed_incremental_change_keeps_document_when_reread_fails() {
    let root = temp_workspace("state", "keep-last-known");
    let uri = {
        let file_path = root.join("missing.test");
        Url::from_file_path(file_path).expect("path can be converted to a URL")
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
    let root = temp_workspace("state", "keep-last-known-tree");
    let uri = {
        let file_path = root.join("missing.json");
        Url::from_file_path(file_path).expect("path can be converted to a URL")
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
    let root = temp_workspace("state", "save-from-params");
    let uri = {
        let file_path = root.join("saved.txt");
        Url::from_file_path(file_path).expect("path converts to a URL")
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
    let root = temp_workspace("state", "save-from-disk");
    let file_path = root.join("on-disk.txt");
    fs::write(&file_path, "from disk").expect("file can be written");
    let uri = Url::from_file_path(&file_path).expect("path converts to a URL");
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
    let root = temp_workspace("state", "save-removes");
    let uri = {
        let file_path = root.join("missing.txt");
        Url::from_file_path(file_path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let _ = state.handle_document_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "test".into(), 1, "before".into()),
    });
    assert!(
        state.document(&uri).is_some(),
        "document is tracked before save",
    );

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: None,
    });

    assert!(
        state.document(&uri).is_none(),
        "document is removed on failure",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn document_save_evicts_cached_semantic_tokens() {
    let root = temp_workspace("state", "save-evict");
    let uri = {
        let file_path = root.join("saved.test");
        Url::from_file_path(file_path).expect("path converts to a URL")
    };
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    open_document(&mut state, uri.clone(), "before");
    seed_semantic_tokens(&state, &uri);
    assert!(state.cached_semantic_tokens(&uri).is_some());

    let _ = state.handle_document_save(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        text: Some("after".into()),
    });

    assert!(state.document(&uri).is_some());
    assert!(state.cached_semantic_tokens(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn watched_files_change_rereads_mutated_workspace_document() {
    let root = temp_workspace("state", "watched-changed");
    let file_path = root.join("a.test");
    fs::write(&file_path, "before").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    let uri = urls[0].clone();
    assert_eq!(state.document(&uri).unwrap().text_contents(), "before");

    fs::write(&file_path, "after").expect("test file can be written");
    let _ = state
        .handle_watched_files_change(vec![FileEvent::new(uri.clone(), FileChangeType::CHANGED)]);

    assert_eq!(state.document(&uri).unwrap().text_contents(), "after");
    assert_eq!(
        state.document_workspace_version(&uri),
        None,
        "the refreshed snapshot stays Workspace-origin",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn watched_files_delete_drops_the_workspace_document() {
    let root = temp_workspace("state", "watched-deleted");
    fs::write(root.join("a.test"), "disk").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    let uri = urls[0].clone();
    assert!(state.document(&uri).is_some());

    let _ = state
        .handle_watched_files_change(vec![FileEvent::new(uri.clone(), FileChangeType::DELETED)]);

    assert!(state.document(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn file_rename_and_delete_drop_the_workspace_documents() {
    let root = temp_workspace("state", "file-operations");
    fs::write(root.join("a.test"), "a").expect("test file can be written");
    fs::write(root.join("b.test"), "b").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls.len(), 2);

    let _ = state.handle_files_renamed(vec![FileRename {
        old_uri: urls[0].to_string(),
        new_uri: "file:///tmp/async-language-server-moved.test".into(),
    }]);
    assert!(state.document(&urls[0]).is_none());
    assert!(state.document(&urls[1]).is_some());

    let _ = state.handle_files_deleted(vec![FileDelete {
        uri: urls[1].to_string(),
    }]);
    assert!(state.document(&urls[1]).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn watched_delete_and_file_operations_evict_cached_semantic_tokens() {
    let root = temp_workspace("state", "evict-file-operations");
    fs::write(root.join("a.test"), "a").expect("test file can be written");
    fs::write(root.join("b.test"), "b").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls.len(), 2);
    for uri in &urls {
        seed_semantic_tokens(&state, uri);
        assert!(state.cached_semantic_tokens(uri).is_some());
    }

    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        urls[0].clone(),
        FileChangeType::DELETED,
    )]);
    assert!(state.document(&urls[0]).is_none());
    assert!(
        state.cached_semantic_tokens(&urls[0]).is_none(),
        "a watched delete evicts the cache",
    );

    let _ = state.handle_files_renamed(vec![FileRename {
        old_uri: urls[1].to_string(),
        new_uri: "file:///tmp/async-language-server-moved.test".into(),
    }]);
    assert!(state.document(&urls[1]).is_none());
    assert!(
        state.cached_semantic_tokens(&urls[1]).is_none(),
        "a rename evicts the old URL's cache",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn open_documents_survive_watched_files_and_file_operations() {
    let root = temp_workspace("state", "open-immunity");
    let file_path = root.join("a.test");
    fs::write(&file_path, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(&file_path).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");

    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls, vec![uri.clone()]);

    fs::write(&file_path, "mutated").expect("test file can be written");
    let _ = state.handle_watched_files_change(vec![
        FileEvent::new(uri.clone(), FileChangeType::CHANGED),
        FileEvent::new(uri.clone(), FileChangeType::DELETED),
    ]);
    assert_eq!(state.document(&uri).unwrap().text_contents(), "open");
    assert_eq!(state.document_workspace_version(&uri), Some(1));

    let _ = state.handle_files_renamed(vec![FileRename {
        old_uri: uri.to_string(),
        new_uri: "file:///tmp/async-language-server-moved.test".into(),
    }]);
    assert!(state.document(&uri).is_some());

    let _ = state.handle_files_deleted(vec![FileDelete {
        uri: uri.to_string(),
    }]);
    assert!(state.document(&uri).is_some());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn open_documents_keep_their_cached_semantic_tokens_across_file_operations() {
    let root = temp_workspace("state", "open-cache-immunity");
    let file_path = root.join("a.test");
    fs::write(&file_path, "disk").expect("test file can be written");
    let manifest = fs::canonicalize(&file_path).expect("test file can be canonicalized");
    let uri = Url::from_file_path(&manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");
    seed_semantic_tokens(&state, &uri);
    assert!(state.cached_semantic_tokens(&uri).is_some());

    fs::write(&file_path, "mutated").expect("test file can be written");
    let _ = state.handle_watched_files_change(vec![
        FileEvent::new(uri.clone(), FileChangeType::CHANGED),
        FileEvent::new(uri.clone(), FileChangeType::DELETED),
    ]);

    let _ = state.handle_files_renamed(vec![FileRename {
        old_uri: uri.to_string(),
        new_uri: "file:///tmp/async-language-server-moved.test".into(),
    }]);
    assert!(state.document(&uri).is_some());
    assert!(
        state.cached_semantic_tokens(&uri).is_some(),
        "an open document keeps its cache across file operations",
    );

    let _ = state.handle_files_deleted(vec![FileDelete {
        uri: uri.to_string(),
    }]);
    assert!(state.document(&uri).is_some());
    assert!(state.cached_semantic_tokens(&uri).is_some());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn workspace_refresh_evicts_tokens_of_dropped_documents() {
    let root = temp_workspace("state", "refresh-evict");
    let dropped_path = root.join("a.test");
    fs::write(&dropped_path, "a").expect("test file can be written");
    fs::write(root.join("b.test"), "b").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls.len(), 2);
    for uri in &urls {
        seed_semantic_tokens(&state, uri);
        assert!(state.cached_semantic_tokens(uri).is_some());
    }

    fs::remove_file(&dropped_path).expect("test file can be removed");
    let kept = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(kept.len(), 1);

    let dropped_uri = urls
        .iter()
        .find(|uri| !kept.contains(uri))
        .expect("dropped URL is identified");
    assert!(state.document(dropped_uri).is_none());
    assert!(
        state.cached_semantic_tokens(dropped_uri).is_none(),
        "a document the refresh drops loses its cache",
    );
    let kept_uri = &kept[0];
    assert!(state.document(kept_uri).is_some());
    assert!(
        state.cached_semantic_tokens(kept_uri).is_some(),
        "a document the refresh keeps keeps its cache",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[tokio::test]
async fn disabling_workspace_diagnostics_evicts_cached_semantic_tokens() {
    let root = temp_workspace("state", "disable-evict");
    fs::write(root.join("a.test"), "disk").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    let uri = urls[0].clone();
    seed_semantic_tokens(&state, &uri);
    assert!(state.cached_semantic_tokens(&uri).is_some());

    assert!(state.set_workspace_diagnostics_enabled(false));

    assert!(state.document(&uri).is_none());
    assert!(state.cached_semantic_tokens(&uri).is_none());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn enabling_workspace_diagnostics_keeps_workspace_documents() {
    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_workspace_diagnostics(
            WorkspaceDiagnostics::setting("test").with_default_enabled(false),
        ),
    );
    let uri = url("enable-keeps.test");
    state.insert_document(
        &uri,
        "disk".into(),
        0,
        "test".into(),
        DocumentOrigin::Workspace,
    );
    assert!(
        state.document(&uri).is_some(),
        "workspace document is tracked",
    );

    assert!(
        state.set_workspace_diagnostics_enabled(true),
        "enabling is a change",
    );
    assert!(
        state.document(&uri).is_some(),
        "enabling keeps workspace documents",
    );

    assert!(
        state.set_workspace_diagnostics_enabled(false),
        "disabling is a change",
    );
    assert!(
        state.document(&uri).is_none(),
        "disabling purges workspace documents",
    );
}

#[test]
fn position_encoding_setter_updates_negotiated_state() {
    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    assert_eq!(state.get_position_encoding(), Encoding::UTF16);
    state.set_position_encoding(Encoding::UTF8);
    assert_eq!(state.get_position_encoding(), Encoding::UTF8);
}

/// A successful ranged edit on a grammar-carrying document must re-parse
/// the tree: the query runs against the installed generation, so a skipped
/// (or un-finalized) re-parse leaves the pre-edit structure visible.
#[cfg(feature = "tree-sitter")]
#[test]
fn incremental_did_change_updates_the_syntax_tree() {
    let mut state = ServerState::with_options::<JsonServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let uri = url("incremental-tree.json");
    open_document(&mut state, uri.clone(), r#"{"a": 1}"#);

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 7), Position::new(0, 7))),
            range_length: None,
            text: r#", "b": 2"#.into(),
        }],
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(document.text_contents(), r#"{"a": 1, "b": 2}"#);
    let pairs = document
        .query("(pair) @p")
        .expect("tree is coherent with the edited text");
    assert_eq!(pairs.len(), 2, "the re-parsed tree sees the inserted pair");
}

/// The chunked-input parse callback must slice each rope chunk relative to
/// the chunk's own start: a fixture spanning several chunks fails loudly
/// otherwise, while single-chunk documents mask the offset arithmetic.
#[cfg(feature = "tree-sitter")]
#[test]
fn parse_rope_serves_multi_chunk_ropes() {
    let mut state = ServerState::with_options::<JsonServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let text = format!(
        "{{{}}}",
        (0..400)
            .map(|i| format!(r#""k{i:03}": {i}"#))
            .collect::<Vec<_>>()
            .join(","),
    );
    let uri = url("multi-chunk.json");
    open_document(&mut state, uri.clone(), text);

    let document = state.document(&uri).expect("document is tracked");
    assert!(
        document.as_ref().chunks().count() > 1,
        "fixture spans multiple rope chunks",
    );
    let pairs = document
        .query("(pair) @p")
        .expect("tree reflects the full multi-chunk text");
    assert_eq!(pairs.len(), 400, "every pair across all chunks is parsed");
}

/// The incremental `InputEdit` must count the inserted text exactly —
/// byte offsets, the newline-driven row advance, and byte columns — so the
/// re-parsed tree stays coherent with the edited text.
#[cfg(feature = "tree-sitter")]
#[test]
fn tree_sitter_edit_computes_new_end_from_inserted_text() {
    use std::fmt::Write as _;

    // A non-zero start, multi-line, multi-byte insert: every computed field
    // of the edit (start/new end bytes, row, byte columns) has to be exact.
    let mut state = ServerState::with_options::<JsonServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let uri = url("incremental-edit.json");
    open_document(&mut state, uri.clone(), r#"{"a": 1}"#);

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 6), Position::new(0, 7))),
            range_length: None,
            text: "2,\n  \"b\": \"🙂\"".into(),
        }],
    });

    let document = state.document(&uri).expect("document stays tracked");
    assert_eq!(
        document.text_contents(),
        r#"{"a": 2,
  "b": "🙂"}"#,
    );
    let pairs = document
        .query("(pair) @p")
        .expect("tree is coherent with the edited text");
    assert_eq!(pairs.len(), 2, "the re-parsed tree sees the inserted pair");
    assert_eq!(pairs[1].text, r#""b": "🙂""#);
    assert_eq!(
        pairs[1].range.start,
        Position::new(1, 2),
        "the inserted newline lands the second pair on row 1, byte column 2",
    );

    // A long tail after the edit: tree-sitter carries the reused tail
    // across the incremental re-parse shifted by the edit's new-end delta,
    // so a new-end byte that does not count the inserted text poisons the
    // tail's mapping and surfaces as a truncated tree. Tree-sitter-upgrade
    // sensitivity: the truncation rides on the reuse decision; the pair
    // count is the discriminator.
    let mut state = ServerState::with_options::<JsonServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let mut text = String::from(r#"{"a": 1"#);
    for i in 0..100 {
        write!(text, r#", "t{i:03}": {i}"#).expect("writing to a String cannot fail");
    }
    text.push('}');
    let uri = url("incremental-edit-tail.json");
    open_document(&mut state, uri.clone(), text);

    let _ = state.handle_document_change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 6), Position::new(0, 7))),
            range_length: None,
            text: "9,\n  \"x\": 5".into(),
        }],
    });

    let document = state.document(&uri).expect("document stays tracked");
    let pairs = document
        .query("(pair) @p")
        .expect("tree is coherent with the edited text");
    assert_eq!(
        pairs.len(),
        102,
        "the whole tail survives the incremental edit",
    );
    assert_eq!(
        pairs.last().expect("pairs are collected").range.start,
        Position::new(1, 1188),
        "the tail rides one row down at the delta-shifted column",
    );
}

/// The early-return paths of a workspace refresh must still report the
/// documents the state tracks, not an empty batch.
#[tokio::test]
async fn refresh_without_roots_reports_tracked_documents() {
    let uri = url("early-return-urls.test");

    let mut disabled = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_workspace_diagnostics(WorkspaceDiagnostics::disabled()),
    );
    open_document(&mut disabled, uri.clone(), "open");
    let urls = disabled
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls,
        vec![uri.clone()],
        "the disabled-diagnostics early return reports tracked documents",
    );

    let mut rootless = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    open_document(&mut rootless, uri.clone(), "open");
    let urls = rootless
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls,
        vec![uri],
        "the no-roots early return reports tracked documents",
    );
}

/// Folder removal drops only Workspace-origin snapshots inside the removed
/// roots: open documents survive anywhere, and workspace snapshots outside
/// the removed roots survive too.
#[tokio::test]
async fn removing_folder_roots_keeps_open_drops_workspace_documents() {
    let root_a = temp_workspace("state", "remove-roots-a");
    let root_b = temp_workspace("state", "remove-roots-b");
    fs::write(root_a.join("a.test"), "open").expect("test file can be written");
    fs::write(root_a.join("c.test"), "dropped").expect("test file can be written");
    fs::write(root_b.join("b.test"), "kept").expect("test file can be written");
    let uri_of = |path: std::path::PathBuf| {
        let manifest = fs::canonicalize(path).expect("test file can be canonicalized");
        Url::from_file_path(manifest).expect("path can be converted to a URL")
    };
    let a_uri = uri_of(root_a.join("a.test"));
    let c_uri = uri_of(root_a.join("c.test"));
    let b_uri = uri_of(root_b.join("b.test"));

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root_a), workspace_folder(&root_b)]);
    open_document(&mut state, a_uri.clone(), "open");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(
        urls.len(),
        3,
        "two workspace files load; the open document is reported",
    );

    let _ = state.handle_workspace_folders_change(DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            removed: vec![workspace_folder(&root_a)],
            added: Vec::new(),
        },
    });

    assert!(
        state.document(&a_uri).is_some(),
        "the open document survives its folder's removal",
    );
    assert!(
        state.document(&c_uri).is_none(),
        "the workspace snapshot inside the removed root is dropped",
    );
    assert!(
        state.document(&b_uri).is_some(),
        "the workspace snapshot outside the removed root survives",
    );

    fs::remove_dir_all(root_a).expect("temp workspace can be removed");
    fs::remove_dir_all(root_b).expect("temp workspace can be removed");
}

/// The refresh's retention predicate must keep open documents on the
/// `Open` disjunct alone: an open document inside the roots whose file
/// vanished from disk is absent from the freshly walked set and still may
/// not be evicted.
#[tokio::test]
async fn refresh_retains_open_documents_absent_from_the_fresh_set() {
    let root = temp_workspace("state", "refresh-absent-open");
    let file_path = root.join("vanishing.test");
    fs::write(&file_path, "open").expect("test file can be written");
    let manifest = fs::canonicalize(&file_path).expect("test file can be canonicalized");
    let uri = Url::from_file_path(manifest).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    open_document(&mut state, uri.clone(), "open");

    fs::remove_file(&file_path).expect("test file can be removed");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert!(
        urls.is_empty(),
        "precondition: the vanished file is absent from the walk",
    );
    assert!(
        state.document(&uri).is_some(),
        "an open document whose file vanished stays tracked across a refresh",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}
