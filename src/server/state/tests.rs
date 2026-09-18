use super::walk_cache::WalkCache;
use super::{CONVERSION_FALLBACK_BOUND, DocumentOrigin, ServerState};
use crate::lsp_requests::{Request, SemanticTokensFullRequest};
use crate::server::{DocumentMatcher, Server, ServerOptions, WorkspaceDiagnostics};
use crate::testing::{
    advertise_workspace_diagnostics, open_document, temp_workspace, token, url, workspace_folder,
};
use crate::text_utils::Encoding;
use async_lsp::ClientSocket;
use async_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, FileChangeType, FileDelete, FileEvent,
    FileRename, HoverProviderCapability, Position, Range, SemanticTokens, SemanticTokensResult,
    ServerCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    Url, VersionedTextDocumentIdentifier, WorkspaceFoldersChangeEvent,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

struct TestServer;

impl Server for TestServer {
    fn server_document_matchers() -> Vec<DocumentMatcher> {
        crate::testing::test_document_matchers()
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    // no seeding: the disabled mode's kill-switch keeps support off
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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
    advertise_workspace_diagnostics(&state);
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

/// `set_advertised_methods` replaces the pre-`initialize` inventory: until
/// it runs a default error is silent, afterwards the advertised method
/// warns through the state exactly once.
#[test]
fn advertised_methods_warn_once_through_the_state() {
    use crate::error::ServerError;

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    let default_error = || ServerError::MethodNotImplemented { method: "hover" };

    assert!(
        !state.warn_once_default("hover", &default_error()),
        "before initialize records anything, a default error is silent",
    );

    let caps = ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
    };
    state.set_advertised_methods(&caps);
    assert!(state.warn_once_default("hover", &default_error()));
    assert!(
        !state.warn_once_default("hover", &default_error()),
        "the second default hit stays silent",
    );
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
    advertise_workspace_diagnostics(&rootless);
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
    advertise_workspace_diagnostics(&state);
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

/// A per-file load failure inside a poll degrades to a traced skip: the
/// poll succeeds and covers the remaining files, and the poison never
/// enters the documents map. Unix-only: the failure is injected with
/// permissions.
#[tokio::test]
#[cfg(unix)]
async fn refresh_skips_unreadable_files_and_keeps_the_rest() {
    use std::os::unix::fs::PermissionsExt;

    // The mode is restored so the cleanup below can remove the workspace.
    const RESTORED_MODE: u32 = 0o644;

    let root = temp_workspace("state", "skip-unreadable");
    fs::write(root.join("good.test"), "good").expect("file can be written");
    fs::write(root.join("locked.test"), "locked").expect("file can be written");
    fs::set_permissions(root.join("locked.test"), fs::Permissions::from_mode(0o000))
        .expect("permissions can be restricted");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds despite the unreadable file");

    assert!(
        urls.iter().any(|url| url.as_str().ends_with("good.test")),
        "the readable file is covered by the poll: {urls:?}",
    );
    assert!(
        !urls.iter().any(|url| url.as_str().ends_with("locked.test")),
        "the unreadable file is excluded from the poll: {urls:?}",
    );
    assert!(
        !state
            .document_urls()
            .iter()
            .any(|tracked| tracked.as_str().ends_with("locked.test")),
        "the poison never enters the documents map",
    );

    fs::set_permissions(
        root.join("locked.test"),
        fs::Permissions::from_mode(RESTORED_MODE),
    )
    .expect("permissions can be restored");
    fs::remove_dir_all(root).expect("temp workspace can be removed");
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
    advertise_workspace_diagnostics(&state);
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

/// The cache contract, black-box: list membership changes reach the
/// returned urls only after an invalidation event. A created file is
/// invisible until the (watcher-simulated) invalidation; a deleted file
/// degrades to a skip, never a failure.
#[tokio::test]
async fn walk_cache_serves_between_invalidations_and_refreshes_on_them() {
    let root = temp_workspace("state", "walk-cache");
    fs::write(root.join("a.test"), "a").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    // The end state a successful initialize produces: the client is capable
    // and its acceptance of the watcher registration arrived — both flags
    // gate the cache-serving branch.
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    let a_uri = urls[0].clone();
    assert_eq!(urls, vec![a_uri.clone()]);

    // The stored list is fresh enough to serve: the created file is
    // invisible to the poll until an invalidation event arrives.
    fs::write(root.join("b.test"), "b").expect("test file can be written");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(
        urls,
        vec![a_uri.clone()],
        "the fresh cache serves; the created file stays invisible",
    );

    // The watcher-simulated event: membership may change on the next poll.
    state.walk_cache().invalidate();
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(
        urls.len(),
        2,
        "the invalidation lets the created file enter the poll: {urls:?}",
    );
    let b_uri = urls
        .iter()
        .find(|uri| **uri != a_uri)
        .expect("the created URL is identified")
        .clone();
    assert!(
        state.document(&b_uri).is_some(),
        "the created file loads on the fresh walk",
    );

    // A deleted entry degrades to a skip: the cached list may still carry
    // the file, the failed load never fails the poll, and the retain pass
    // drops the document.
    fs::remove_file(root.join("b.test")).expect("test file can be removed");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("the deleted file degrades to a skip");
    assert_eq!(urls, vec![a_uri.clone()]);
    assert!(
        state.document(&b_uri).is_none(),
        "the deleted entry falls out via the retain pass",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// The cache gate's negative direction: without an accepted watcher
/// registration the cache never serves, so every poll walks and a file
/// created between two refreshes is visible immediately — no invalidation
/// needed. The both-direction pin for the `watchers_registered()` gate
/// (the serving test above pins the positive direction).
#[tokio::test]
async fn without_watchers_every_poll_walks_and_sees_new_files() {
    let root = temp_workspace("state", "walk-per-poll");
    fs::write(root.join("a.test"), "a").expect("test file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    // Neither flag set: no watcher support (or the registration never
    // completed) — the cache must stay out of the way.
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(urls.len(), 1);

    fs::write(root.join("b.test"), "b").expect("test file can be written");
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("workspace documents can be refreshed");
    assert_eq!(
        urls.len(),
        2,
        "no cache without a registration: the new file is visible at once",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// An event on a configured ignore file invalidates the walk cache: a
/// file the new rules exclude falls out, a file they newly include loads
/// — through the ordinary refresh path, no new state channel.
#[tokio::test]
async fn ignore_file_events_invalidate_the_walk_cache_not_documents() {
    let root = temp_workspace("state", "ignore-watch");
    fs::write(root.join("a.test"), "a").expect("file can be written");
    fs::write(root.join("b.test"), "b").expect("file can be written");
    // Empty rules exclude nothing: the first walk sees both files, and
    // the write below is the rule flip the changed event responds to.
    fs::write(root.join(".mylspignore"), "").expect("ignore file can be written");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_ignore_filenames([".mylspignore"]),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    let a_uri = urls
        .iter()
        .find(|url| url.as_str().ends_with("a.test"))
        .expect("a.test is walked")
        .clone();
    let b_uri = urls
        .iter()
        .find(|url| url.as_str().ends_with("b.test"))
        .expect("b.test is walked")
        .clone();
    assert_eq!(urls.len(), 2, "no ignore event yet: both files are in");

    // The event flips the rules: the CHANGED event on the ignore file
    // invalidates the cache, the re-walk excludes b.test, and its
    // workspace document falls out via the retain pass.
    fs::write(root.join(".mylspignore"), "b.test\n").expect("ignore file can be written");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".mylspignore")).expect("path converts"),
        FileChangeType::CHANGED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls,
        vec![a_uri.clone()],
        "the flipped rules exclude b.test from the walk",
    );
    assert!(
        state.document(&b_uri).is_none(),
        "the newly excluded document falls out via the retain pass",
    );

    // The event flips the rules back: without the ignore file both files
    // walk again, and the re-walk re-loads what the rules had dropped.
    fs::remove_file(root.join(".mylspignore")).expect("ignore file can be removed");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".mylspignore")).expect("path converts"),
        FileChangeType::DELETED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls.len(),
        2,
        "without the ignore file both files walk again",
    );
    assert!(state.document(&a_uri).is_some());
    assert!(state.document(&b_uri).is_some());

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// Ignore rules govern walking only (spec §6, pin 2): an open document
/// the rules exclude stays tracked with its editor's text, stays out of
/// the poll's list, and an ignore-file event never evicts it.
#[tokio::test]
async fn open_documents_stay_tracked_when_ignore_rules_exclude_them() {
    let root = temp_workspace("state", "ignore-open-immunity");
    fs::write(root.join("a.test"), "a").expect("file can be written");
    fs::write(root.join("b.test"), "b").expect("file can be written");
    fs::write(root.join(".mylspignore"), "b.test\n").expect("ignore file can be written");
    let canonical_b =
        fs::canonicalize(root.join("b.test")).expect("test file can be canonicalized");
    let b_uri = Url::from_file_path(canonical_b).expect("path can be converted to a URL");

    let mut state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_ignore_filenames([".mylspignore"]),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    open_document(&mut state, b_uri.clone(), "open b");
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert!(
        !urls.contains(&b_uri),
        "the rules keep the open document out of the poll's walk",
    );
    assert_eq!(
        state
            .document(&b_uri)
            .expect("open document stays tracked")
            .text_contents(),
        "open b",
        "the tracked snapshot keeps the editor's text",
    );

    // The ignore file's deletion re-includes the file in the walk, and
    // the open document survives the event untouched.
    fs::remove_file(root.join(".mylspignore")).expect("ignore file can be removed");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".mylspignore")).expect("path converts"),
        FileChangeType::DELETED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert!(
        urls.contains(&b_uri),
        "the open document reports again once the rules are gone",
    );
    assert!(
        state.document(&b_uri).is_some(),
        "an open document is never evicted by ignore rules",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// The built-in `.gitignore` closes the walk-cache spec's staleness
/// limitation: an edit event invalidates, so membership changes are seen
/// on the next poll.
#[tokio::test]
async fn gitignore_edit_events_invalidate_the_walk_cache() {
    let root = temp_workspace("state", "gitignore-watch");
    // ignore 0.4.33 defaults to require_git = true: .gitignore is honored
    // only with a .git at or above the walk root.
    fs::create_dir_all(root.join(".git")).expect("git dir can be created");
    fs::write(root.join("a.test"), "a").expect("file can be written");
    fs::write(root.join(".gitignore"), "\n").expect("gitignore exists");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    state.set_workspace_folders([workspace_folder(&root)]);
    advertise_workspace_diagnostics(&state);
    state.set_file_watching(true);
    state.set_watchers_registered(true);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(urls.len(), 1);

    fs::write(root.join(".gitignore"), "a.test\n").expect("a.test becomes ignored");
    fs::write(root.join("b.test"), "b").expect("file can be written");
    let _ = state.handle_watched_files_change(vec![FileEvent::new(
        Url::from_file_path(root.join(".gitignore")).expect("path converts"),
        FileChangeType::CHANGED,
    )]);
    let urls = state
        .refresh_workspace_documents()
        .await
        .expect("refresh succeeds");
    assert_eq!(
        urls.len(),
        1,
        "the gitignore edit re-walks: a.test excluded, b.test included",
    );
    assert!(urls[0].as_str().ends_with("b.test"));

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// The both-direction pin for the dirty flag (mutation-driven rules):
/// invalidation forces a walk; a second refresh with no event between
/// serves the cache. Pinned through the observable list behavior —
/// a walk-but-discard mutant is an equivalent-mutant disposition. The
/// folders-changed `clear` is pinned at the tail: the list is gone
/// entirely, not merely dirty.
#[test]
fn invalidation_flips_serving_to_walking_and_back() {
    let entry = || {
        (
            PathBuf::from("/tmp/walk-cache-flip.test"),
            url("walk-cache-flip.test"),
            Arc::new(DocumentMatcher::new("flip")),
        )
    };

    let cache = WalkCache::new();
    assert!(
        cache.get_valid().is_none(),
        "no entries: the next refresh walks",
    );

    cache.store(vec![entry()]);
    assert_eq!(
        cache.get_valid().map(|entries| entries.len()),
        Some(1),
        "no event between refreshes: the cache serves",
    );

    cache.invalidate();
    assert!(
        cache.get_valid().is_none(),
        "invalidation forces the next refresh to walk",
    );

    // The re-walk stores a fresh list, which serves again — and a clear on
    // top of the fresh list drops it entirely (re-stored first, so the
    // clear's effect cannot ride the earlier dirty flag).
    cache.store(vec![entry()]);
    assert_eq!(
        cache.get_valid().map(|entries| entries.len()),
        Some(1),
        "the re-walk's store serves again",
    );
    cache.clear();
    assert!(
        cache.get_valid().is_none(),
        "the folders-changed clear drops the list entirely",
    );
}

/// The conversion-fallback cache: priming reads disk once per
/// (URL, stamp) on the blocking pool; a changed stamp re-reads; the
/// cache-only read replaces the old per-request disk read.
#[tokio::test]
async fn conversion_fallback_primes_once_per_stamp_and_rereads_on_change() {
    let root = temp_workspace("state", "fallback-cache");
    let file_path = root.join("untracked.test");
    fs::write(&file_path, "first").expect("file can be written");
    let uri = Url::from_file_path(&file_path).expect("path converts to a URL");

    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );

    assert!(
        state.fallback_document(&uri).is_none(),
        "nothing cached before the first prime",
    );
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state
            .fallback_document(&uri)
            .expect("primed")
            .text_contents(),
        "first",
    );

    // Same stamp: the second prime must not re-read — observable through
    // a disk write that the stamp cannot yet see. Equal size alone is not
    // enough: filesystems stamp writes with nanosecond mtimes, so the
    // test pins the file's mtime back to the first write's value, making
    // the (mtime, size) stamp of both writes identical by construction.
    let pinned_mtime = fs::metadata(&file_path)
        .expect("file exists")
        .modified()
        .expect("mtime is available");
    fs::write(&file_path, "secon").expect("same-size write keeps the stamp");
    fs::File::options()
        .write(true)
        .open(&file_path)
        .expect("file can be reopened")
        .set_modified(pinned_mtime)
        .expect("mtime can be pinned");
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state
            .fallback_document(&uri)
            .expect("still cached")
            .text_contents(),
        "first",
        "an unchanged stamp does not re-read",
    );

    // Different size: the stamp changes, the prime re-reads.
    fs::write(&file_path, "second version").expect("stamp-changing write");
    state.prime_conversion_fallback(uri.clone()).await;
    assert_eq!(
        state
            .fallback_document(&uri)
            .expect("re-primed")
            .text_contents(),
        "second version",
    );

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

/// The bound-clear contract: priming the ([`CONVERSION_FALLBACK_BOUND`] +
/// 1)-th URL clears the whole cache and installs the fresh entry — the
/// breaching prime is cached, every earlier entry is gone.
#[tokio::test]
async fn conversion_fallback_clears_whole_cache_at_the_bound() {
    let root = temp_workspace("state", "fallback-bound");
    let state = ServerState::with_options::<TestServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );

    let urls: Vec<Url> = (0..CONVERSION_FALLBACK_BOUND)
        .map(|index| {
            let file_path = root.join(format!("file-{index}.txt"));
            fs::write(&file_path, "before the bound").expect("file can be written");
            Url::from_file_path(&file_path).expect("path converts to a URL")
        })
        .collect();
    for uri in &urls {
        state.prime_conversion_fallback(uri.clone()).await;
    }
    assert_eq!(
        state
            .fallback_document(&urls[0])
            .expect("primed")
            .text_contents(),
        "before the bound",
        "the first entry is cached before the bound is reached",
    );

    // The (bound + 1)-th prime breaches the bound: clear-then-insert —
    // the fresh entry is cached, all earlier entries are gone.
    let overflow_path = root.join("overflow.txt");
    fs::write(&overflow_path, "past the bound").expect("file can be written");
    let overflow_uri = Url::from_file_path(&overflow_path).expect("path converts to a URL");
    state.prime_conversion_fallback(overflow_uri.clone()).await;
    assert_eq!(
        state
            .fallback_document(&overflow_uri)
            .expect("primed")
            .text_contents(),
        "past the bound",
        "the bound-breaching prime is cached",
    );
    for uri in &urls {
        assert!(
            state.fallback_document(uri).is_none(),
            "the bound breach cleared earlier entries: {uri}",
        );
    }

    fs::remove_dir_all(root).expect("temp workspace can be removed");
}

#[test]
fn watcher_globs_sort_and_dedup_across_matchers() {
    struct TwinMatchersServer;

    impl Server for TwinMatchersServer {
        fn server_document_matchers() -> Vec<DocumentMatcher> {
            vec![
                DocumentMatcher::new("a").with_url_globs(["*.z", "*.a"]),
                DocumentMatcher::new("b").with_url_globs(["*.a", "*.m"]),
            ]
        }
    }

    let state = ServerState::with_options::<TwinMatchersServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default(),
    );
    assert_eq!(
        state.watcher_globs(),
        ["**/.gitignore", "*.a", "*.m", "*.z"],
        "globs shared across matchers register once, in a stable order; the built-in .gitignore always registers",
    );
}

/// One invalid configured ignore name must not poison the registration:
/// it is skipped (warned), the valid names and the built-in still
/// register — the same validity rule the matcher globs are filtered by.
#[test]
fn watcher_globs_skip_invalid_configured_names() {
    struct BareServer;

    impl Server for BareServer {}

    let state = ServerState::with_options::<BareServer>(
        ClientSocket::new_closed(),
        &ServerOptions::default().with_ignore_filenames([".valid", "bad[glob"]),
    );
    assert_eq!(state.watcher_globs(), ["**/.gitignore", "**/.valid"]);
}

#[test]
fn ignore_configuration_defaults_to_inert() {
    let options = ServerOptions::default();
    assert!(options.ignore_filenames.is_empty());
    assert!(options.global_ignore_file.is_none());
}
