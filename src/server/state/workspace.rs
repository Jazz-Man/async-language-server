use std::{collections::HashSet, ops::ControlFlow, path::PathBuf};

use async_lsp::{
    Result,
    lsp_types::{DidChangeWorkspaceFoldersParams, Url, WorkspaceFolder},
};

use super::{DocumentOrigin, ServerState};

use crate::{
    error::ServerResult,
    workspace::{WorkspaceWalkConfig, WorkspaceWalker, path_to_url},
};

impl ServerState {
    pub(crate) fn set_workspace_folders(&self, folders: impl IntoIterator<Item = WorkspaceFolder>) {
        self.workspace_roots.clear();

        for folder in folders {
            if let Some(path) = workspace_folder_path(&folder) {
                self.workspace_roots.insert(folder.uri, path);
            }
        }
    }

    pub(crate) fn handle_workspace_folders_change(
        &self,
        params: DidChangeWorkspaceFoldersParams,
    ) -> ControlFlow<Result<()>> {
        let removed_roots: Vec<_> = params
            .event
            .removed
            .iter()
            .filter_map(workspace_folder_path)
            .collect();

        for folder in params.event.removed {
            self.workspace_roots.remove(&folder.uri);
        }
        self.remove_workspace_documents_in_roots(&removed_roots);

        for folder in params.event.added {
            if let Some(path) = workspace_folder_path(&folder) {
                self.workspace_roots.insert(folder.uri, path);
            }
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn workspace_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<_> = self
            .workspace_roots
            .iter()
            .map(|root| root.value().clone())
            .collect();
        roots.sort();
        roots
    }

    pub(crate) fn document_urls(&self) -> Vec<Url> {
        let mut urls: Vec<_> = self
            .documents
            .iter()
            .map(|entry| entry.document.uri.clone())
            .collect();
        urls.sort();
        urls
    }

    pub(crate) fn document_workspace_version(&self, url: &Url) -> Option<i64> {
        let entry = self.documents.get(url)?;
        match entry.origin {
            DocumentOrigin::Open => Some(i64::from(entry.document.version())),
            DocumentOrigin::Workspace => None,
        }
    }

    pub(crate) fn refresh_workspace_documents(&self) -> ServerResult<Vec<Url>> {
        if !self.workspace_diagnostics.enabled() {
            return Ok(self.document_urls());
        }

        let roots = self.workspace_roots();
        if roots.is_empty() {
            return Ok(self.document_urls());
        }

        let walker = WorkspaceWalker::new(&roots, WorkspaceWalkConfig::default())?;
        let mut urls = Vec::new();

        for path in walker.files()? {
            let uri = path_to_url(&path)?;
            let Some(matcher) = self.matchers.find_url(&uri) else {
                continue;
            };

            urls.push(uri.clone());
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                continue;
            }

            let language = matcher
                .lang_strings()
                .first()
                .cloned()
                .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
            // arch-lint: allow(no-sync-io) reason="workspace scanning is a synchronous batch pass over the ignore crate by design"
            let text = std::fs::read_to_string(&path)?;
            self.insert_document(uri, text, 0, language, DocumentOrigin::Workspace);
        }

        let urls: HashSet<_> = urls.into_iter().collect();
        self.documents.retain(|url, entry| {
            entry.origin == DocumentOrigin::Open
                || !url_is_in_roots(url, &roots)
                || urls.contains(url)
        });

        let mut urls: Vec<_> = urls.into_iter().collect();
        urls.sort();
        Ok(urls)
    }

    pub(super) fn remove_workspace_documents(&self) {
        self.documents
            .retain(|_, entry| entry.origin == DocumentOrigin::Open);
    }

    fn remove_workspace_documents_in_roots(&self, roots: &[PathBuf]) {
        if roots.is_empty() {
            return;
        }

        self.documents.retain(|url, entry| {
            entry.origin == DocumentOrigin::Open || !url_is_in_roots(url, roots)
        });
    }
}

pub(super) fn url_is_in_roots(url: &Url, roots: &[PathBuf]) -> bool {
    url.to_file_path()
        .is_ok_and(|path| roots.iter().any(|root| path.starts_with(root)))
}

fn workspace_folder_path(folder: &WorkspaceFolder) -> Option<PathBuf> {
    let path = folder.uri.to_file_path().ok()?;
    // arch-lint: allow(no-sync-io) reason="one-time path canonicalization during workspace-folder setup"
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}
