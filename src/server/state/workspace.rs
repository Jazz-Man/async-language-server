use std::{collections::HashSet, ops::ControlFlow, path::PathBuf, sync::Arc};

use async_lsp::{
    Result,
    lsp_types::{DidChangeWorkspaceFoldersParams, Url, WorkspaceFolder},
};

use super::{DocumentOrigin, FileStamp, ServerState};

use crate::{
    error::ServerResult,
    server::DocumentMatcher,
    workspace::{WorkspaceWalkConfig, WorkspaceWalker, for_each_bounded, path_to_url},
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
            .map(|entry| entry.document.url().clone())
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

    pub(crate) async fn refresh_workspace_documents(&self) -> ServerResult<Vec<Url>> {
        if !self.workspace_diagnostics.enabled() {
            return Ok(self.document_urls());
        }

        let roots = self.workspace_roots();
        if roots.is_empty() {
            return Ok(self.document_urls());
        }

        let walker = WorkspaceWalker::new(&roots, WorkspaceWalkConfig::default())?;
        let mut urls = Vec::new();
        let mut loads = Vec::new();

        for path in walker.files()? {
            let Some(matcher) = self.matchers.find_path(&path) else {
                continue;
            };
            let uri = path_to_url(&path)?;
            urls.push(uri.clone());
            if self
                .documents
                .get(&uri)
                .is_some_and(|entry| entry.origin == DocumentOrigin::Open)
            {
                continue;
            }

            loads.push((path, uri, matcher));
        }

        let state = self.clone();
        let width = state.diagnostics_parallelism();
        for_each_bounded(loads, width, move |(path, uri, matcher)| {
            let state = state.clone();
            async move { load_workspace_document(state, path, uri, matcher).await }
        })
        .await?;

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

fn file_stamp(path: &std::path::Path) -> Option<FileStamp> {
    // arch-lint: allow(no-sync-io) reason="workspace scanning is a synchronous batch pass over the ignore crate by design"
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

/// Loads one workspace file into the document map: probes the disk stamp
/// first, skips only an unchanged Workspace-origin entry, then reads the
/// file, parses it, and installs the document in one blocking hop,
/// stamping it afterwards. The metadata probe and the read+parse+install
/// composite run on the blocking pool.
async fn load_workspace_document(
    state: ServerState,
    path: PathBuf,
    uri: Url,
    matcher: Arc<DocumentMatcher>,
) -> ServerResult<()> {
    let stamp = tokio::task::spawn_blocking({
        let path = path.clone();
        move || file_stamp(&path)
    })
    .await
    .ok()
    .flatten();
    if state.documents.get(&uri).is_some_and(|entry| {
        entry.origin == DocumentOrigin::Workspace && stamp_unchanged(entry.stamp, stamp)
    }) {
        return Ok(());
    }

    let language = matcher
        .lang_strings()
        .first()
        .cloned()
        .unwrap_or_else(|| matcher.name().to_ascii_lowercase());
    tokio::task::spawn_blocking({
        let state = state.clone();
        let uri = uri.clone();
        move || -> std::io::Result<()> {
            // arch-lint: allow(no-sync-io) reason="workspace file IO runs on the blocking pool by design"
            let text = std::fs::read_to_string(&path)?;
            state.insert_document(uri, text, 0, language, DocumentOrigin::Workspace);
            Ok(())
        }
    })
    .await
    .map_err(std::io::Error::from)??;
    if let Some(mut entry) = state.documents.get_mut(&uri) {
        entry.stamp = stamp;
    }
    Ok(())
}

/// Conservative gate: only an exact stamp match on an already-tracked
/// Workspace-origin document skips the re-read; missing stamps or any
/// difference re-reads.
fn stamp_unchanged(entry_stamp: Option<FileStamp>, disk_stamp: Option<FileStamp>) -> bool {
    matches!((entry_stamp, disk_stamp), (Some(a), Some(b)) if a == b)
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

#[cfg(test)]
mod tests {
    use super::stamp_unchanged;

    #[test]
    fn stamp_gate_is_conservative() {
        use std::time::{Duration, SystemTime};

        let now = SystemTime::now();
        let stamp = (now, 12);
        assert!(stamp_unchanged(Some(stamp), Some(stamp)));
        assert!(!stamp_unchanged(
            Some(stamp),
            Some((now + Duration::from_secs(1), 12))
        ));
        assert!(!stamp_unchanged(Some(stamp), Some((now, 13))));
        assert!(!stamp_unchanged(None, Some(stamp)));
        assert!(!stamp_unchanged(Some(stamp), None));
        assert!(!stamp_unchanged(None, None));
    }
}
