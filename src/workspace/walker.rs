use std::{
    fs,
    path::{Path, PathBuf},
};

use async_lsp::lsp_types::Url;
use ignore::WalkBuilder;

use crate::{error::ServerError, server::ServerResult};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceWalkConfig {
    include_hidden_files: bool,
    respect_ignore_files: bool,
}

impl WorkspaceWalkConfig {
    pub(crate) fn with_hidden_files(mut self, yes: bool) -> Self {
        self.include_hidden_files = yes;
        self
    }

    pub(crate) fn with_ignore_files(mut self, yes: bool) -> Self {
        self.respect_ignore_files = yes;
        self
    }
}

impl Default for WorkspaceWalkConfig {
    fn default() -> Self {
        Self {
            include_hidden_files: false,
            respect_ignore_files: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceWalker {
    roots: Vec<PathBuf>,
    config: WorkspaceWalkConfig,
}

impl WorkspaceWalker {
    pub(crate) fn new(roots: &[PathBuf], config: WorkspaceWalkConfig) -> ServerResult<Self> {
        let roots = roots
            .iter()
            .map(fs::canonicalize)
            .collect::<Result<_, _>>()?;

        Ok(Self { roots, config })
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub(crate) fn files(&self) -> ServerResult<Vec<PathBuf>> {
        let mut files = Vec::new();

        for root in &self.roots {
            let mut builder = WalkBuilder::new(root);
            configure_walker(&mut builder, &self.config);

            for entry in builder.build() {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!("skipping unreadable workspace entry: {error}");
                        continue;
                    }
                };
                // arch-lint: allow(no-sync-io) reason="the ignore-crate walk is a synchronous batch scan by design"
                if entry.file_type().is_some_and(|ty| ty.is_file()) {
                    files.push(entry.into_path());
                }
            }
        }

        files.sort();
        Ok(files)
    }
}

fn configure_walker(builder: &mut WalkBuilder, config: &WorkspaceWalkConfig) {
    builder
        .standard_filters(false)
        .hidden(!config.include_hidden_files)
        .parents(config.respect_ignore_files)
        .ignore(config.respect_ignore_files)
        .git_ignore(config.respect_ignore_files)
        .git_global(config.respect_ignore_files)
        .git_exclude(config.respect_ignore_files);
}

pub(crate) fn path_to_url(path: &Path) -> ServerResult<Url> {
    Url::from_file_path(path).map_err(|()| ServerError::InvalidFilePath {
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{WorkspaceWalkConfig, WorkspaceWalker};

    // One unreadable entry must not abort the scan; this test is unix-only
    // because the failure is injected with filesystem permissions.
    #[test]
    #[cfg(unix)]
    fn files_skips_unreadable_entries() {
        use std::os::unix::fs::PermissionsExt;

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!("als-walker-skip-{millis}"));
        fs::create_dir_all(root.join("bad")).expect("bad dir can be created");
        fs::write(root.join("good.test"), "good").expect("good file can be written");
        fs::set_permissions(root.join("bad"), fs::Permissions::from_mode(0o000))
            .expect("permissions can be restricted");

        let walker =
            WorkspaceWalker::new(std::slice::from_ref(&root), WorkspaceWalkConfig::default())
                .expect("walker can be created");
        let files = walker
            .files()
            .expect("walk succeeds despite unreadable entry");

        assert!(files.iter().any(|file| file.ends_with("good.test")));

        fs::set_permissions(root.join("bad"), fs::Permissions::from_mode(0o755))
            .expect("permissions can be restored");
        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
}
