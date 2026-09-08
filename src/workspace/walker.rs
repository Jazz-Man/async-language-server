use crate::error::ServerError;
use crate::server::ServerResult;
use async_lsp::lsp_types::Url;
use ignore::{WalkBuilder, WalkState};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

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
        let (sender, receiver) = mpsc::channel();

        for root in &self.roots {
            let mut builder = WalkBuilder::new(root);
            configure_walker(&mut builder, &self.config);

            builder.build_parallel().run(|| {
                let sender = sender.clone();
                Box::new(move |entry| match entry {
                    Ok(entry) => {
                        // arch-lint: allow(no-sync-io) reason="the ignore-crate walk is a synchronous batch scan by design"
                        if entry.file_type().is_some_and(|ty| ty.is_file()) {
                            // The receiver outlives every send: it is dropped
                            // only after all walks have joined, so the send
                            // cannot fail.
                            let _ = sender.send(entry.into_path());
                        }
                        WalkState::Continue
                    }
                    Err(error) => {
                        tracing::warn!("skipping unreadable workspace entry: {error}");
                        WalkState::Continue
                    }
                })
            });
        }

        drop(sender);
        let mut files = receiver.into_iter().collect::<Vec<_>>();
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
    use super::{WorkspaceWalkConfig, WorkspaceWalker};
    use crate::testing::temp_workspace;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // The walk's observable contract is the sorted `Vec`, identical for the
    // same tree no matter which order entries are delivered in: every file
    // under each root exactly once per root (nested roots duplicate their
    // files), hidden entries skipped unless `with_hidden_files`, ignore-file
    // matches skipped unless `with_ignore_files`. The expected `Vec`s are a
    // golden capture of the walk output.
    #[test]
    fn files_produce_the_identical_sorted_output_for_the_same_tree() {
        let root = temp_workspace("walker", "determinism");
        fs::create_dir_all(root.join("nested/deep")).expect("nested dirs can be created");
        fs::create_dir_all(root.join("skipped-dir")).expect("skipped dir can be created");
        fs::create_dir_all(root.join(".hidden-dir")).expect("hidden dir can be created");
        fs::write(root.join("a.test"), "a").expect("file can be written");
        fs::write(root.join("z.test"), "z").expect("file can be written");
        fs::write(root.join("nested/b.test"), "b").expect("file can be written");
        fs::write(root.join("nested/deep/c.test"), "c").expect("file can be written");
        fs::write(root.join("skip.test"), "ignored").expect("file can be written");
        fs::write(root.join("skipped-dir/x.test"), "x").expect("file can be written");
        fs::write(root.join(".hidden.test"), "hidden").expect("file can be written");
        fs::write(root.join(".hidden-dir/y.test"), "y").expect("file can be written");
        fs::write(root.join(".ignore"), "skip.test\nskipped-dir/\n")
            .expect("ignore file can be written");

        // The second root nests inside the first: a file under both roots is
        // visited once per root, so the sorted output carries duplicates.
        let walker = WorkspaceWalker::new(
            &[root.clone(), root.join("nested")],
            WorkspaceWalkConfig::default(),
        )
        .expect("walker can be created");
        let (canonical_root, canonical_nested) =
            (walker.roots()[0].clone(), walker.roots()[1].clone());
        assert_eq!(
            walker.files().expect("walk succeeds"),
            vec![
                canonical_root.join("a.test"),
                canonical_root.join("nested/b.test"),
                canonical_nested.join("b.test"),
                canonical_root.join("nested/deep/c.test"),
                canonical_nested.join("deep/c.test"),
                canonical_root.join("z.test"),
            ],
        );

        let hidden = WorkspaceWalker::new(
            std::slice::from_ref(&root),
            WorkspaceWalkConfig::default().with_hidden_files(true),
        )
        .expect("walker can be created");
        assert_eq!(
            hidden.files().expect("walk succeeds"),
            vec![
                canonical_root.join(".hidden-dir/y.test"),
                canonical_root.join(".hidden.test"),
                canonical_root.join(".ignore"),
                canonical_root.join("a.test"),
                canonical_root.join("nested/b.test"),
                canonical_root.join("nested/deep/c.test"),
                canonical_root.join("z.test"),
            ],
        );

        let unfiltered = WorkspaceWalker::new(
            std::slice::from_ref(&root),
            WorkspaceWalkConfig::default().with_ignore_files(false),
        )
        .expect("walker can be created");
        assert_eq!(
            unfiltered.files().expect("walk succeeds"),
            vec![
                canonical_root.join("a.test"),
                canonical_root.join("nested/b.test"),
                canonical_root.join("nested/deep/c.test"),
                canonical_root.join("skip.test"),
                canonical_root.join("skipped-dir/x.test"),
                canonical_root.join("z.test"),
            ],
        );

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    // One unreadable entry must not abort the scan; this test is unix-only
    // because the failure is injected with filesystem permissions.
    #[test]
    #[cfg(unix)]
    fn files_skips_unreadable_entries() {
        use std::os::unix::fs::PermissionsExt;

        // The mode is restored so the cleanup below can remove the restricted
        // directory.
        const RESTORED_MODE: u32 = 0o755;

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

        fs::set_permissions(root.join("bad"), fs::Permissions::from_mode(RESTORED_MODE))
            .expect("permissions can be restored");
        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
}
