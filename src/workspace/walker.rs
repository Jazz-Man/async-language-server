use crate::error::ServerError;
use crate::server::ServerResult;
use async_lsp::lsp_types::Url;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::{WalkBuilder, WalkState};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceWalkConfig {
    include_hidden_files: bool,
    respect_ignore_files: bool,
    ignore_filenames: Vec<String>,
    global_ignore_file: Option<PathBuf>,
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

    pub(crate) fn with_ignore_filenames(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.ignore_filenames = names.into_iter().map(Into::into).collect();
        self
    }

    pub(crate) fn with_global_ignore_file(mut self, file: Option<PathBuf>) -> Self {
        self.global_ignore_file = file;
        self
    }
}

impl Default for WorkspaceWalkConfig {
    fn default() -> Self {
        Self {
            include_hidden_files: false,
            respect_ignore_files: true,
            ignore_filenames: Vec::new(),
            global_ignore_file: None,
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
            let global = self
                .config
                .global_ignore_file
                .as_ref()
                .map(|file| Arc::new(global_ignore_matcher(file, root)));

            builder.build_parallel().run(|| {
                let sender = sender.clone();
                let global = global.clone();
                Box::new(move |entry| match entry {
                    Ok(entry) => {
                        // arch-lint: allow(no-sync-io) reason="the ignore-crate walk is a synchronous batch scan by design"
                        if entry.file_type().is_some_and(|ty| ty.is_file())
                            && global.as_ref().is_none_or(|matcher| {
                                // Parents walk with the match: a
                                // directory pattern must drop the files
                                // below it, since this matcher is
                                // consulted per file, not during
                                // traversal. Entries always sit under
                                // the matcher's root — the walk built
                                // them from it.
                                !matcher
                                    .matched_path_or_any_parents(entry.path(), false)
                                    .is_ignore()
                            })
                        {
                            // The receiver outlives every send: it is
                            // dropped only after all walks have
                            // joined, so the send cannot fail.
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

/// Compiles the global ignore file against one walk root: gitignore
/// syntax, patterns anchored at the root — git's per-repo semantics. A
/// missing or unreadable file matches nothing (warned, never fatal).
fn global_ignore_matcher(file: &Path, root: &Path) -> Gitignore {
    let shown = file.display();
    let mut builder = GitignoreBuilder::new(root);
    match fs::read_to_string(file) {
        Ok(text) => {
            for line in text.lines() {
                if let Err(error) = builder.add_line(None, line) {
                    tracing::warn!("skipping bad pattern '{line}' in '{shown}': {error}");
                }
            }
        }
        Err(error) => {
            tracing::warn!("skipping unreadable global ignore file '{shown}': {error}");
        }
    }
    match builder.build() {
        Ok(matcher) => matcher,
        Err(error) => {
            tracing::warn!("skipping invalid global ignore file '{shown}': {error}");
            Gitignore::empty()
        }
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
    for name in &config.ignore_filenames {
        builder.add_custom_ignore_filename(name);
    }
}

pub(crate) fn path_to_url(path: &Path) -> ServerResult<Url> {
    Url::from_file_path(path).map_err(|()| ServerError::InvalidFilePath {
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceWalkConfig, WorkspaceWalker};
    use crate::testing::{TempWorkspace, workspace};
    use rstest::rstest;
    use std::fs;

    // The walk's observable contract is the sorted `Vec`, identical for the
    // same tree no matter which order entries are delivered in: every file
    // under each root exactly once per root (nested roots duplicate their
    // files), hidden entries skipped unless `with_hidden_files`, ignore-file
    // matches skipped unless `with_ignore_files`. The expected `Vec`s are a
    // golden capture of the walk output.
    #[rstest]
    fn files_produce_the_identical_sorted_output_for_the_same_tree(
        #[with("walker")] workspace: TempWorkspace,
    ) {
        workspace.write("a.test", "a");
        workspace.write("z.test", "z");
        workspace.write("nested/b.test", "b");
        workspace.write("nested/deep/c.test", "c");
        workspace.write("skip.test", "ignored");
        workspace.write("skipped-dir/x.test", "x");
        workspace.write(".hidden.test", "hidden");
        workspace.write(".hidden-dir/y.test", "y");
        workspace.write(".ignore", "skip.test\nskipped-dir/\n");

        // The second root nests inside the first: a file under both roots is
        // visited once per root, so the sorted output carries duplicates.
        let walker = WorkspaceWalker::new(
            &[workspace.root().to_path_buf(), workspace.join("nested")],
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
            &[workspace.root().to_path_buf()],
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
            &[workspace.root().to_path_buf()],
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
    }

    // Custom ignore names work with no `.git` anywhere (spec §3.3): the
    // mechanism is git-independent, gitignore syntax, cascading per
    // directory, with negation.
    #[rstest]
    fn custom_ignore_filenames_exclude_entries_without_git(
        #[with("walker")] workspace: TempWorkspace,
    ) {
        workspace.write("a.test", "a");
        workspace.write("skip.test", "skip");
        workspace.write("keep.log", "keep");
        workspace.write("drop.log", "drop");
        workspace.write("skipped-dir/x.test", "x");
        workspace.write("nested/inner.test", "inner");
        workspace.write(
            ".mylspignore",
            "skip.test\nskipped-dir/\n*.log\n!keep.log\n",
        );

        let walker = WorkspaceWalker::new(
            &[workspace.root().to_path_buf()],
            WorkspaceWalkConfig::default().with_ignore_filenames([".mylspignore"]),
        )
        .expect("walker can be created");
        let canonical = &walker.roots()[0];
        assert_eq!(
            walker.files().expect("walk succeeds"),
            vec![
                canonical.join("a.test"),
                canonical.join("keep.log"),
                canonical.join("nested/inner.test"),
            ],
        );

        // Cascading: a nested .mylspignore drops only what it names.
        workspace.write("nested/.mylspignore", "inner.test\n");
        assert_eq!(
            walker.files().expect("walk succeeds"),
            vec![canonical.join("a.test"), canonical.join("keep.log")],
        );
    }

    // The global ignore file applies to every root regardless of git
    // presence; patterns anchor at each root (git per-repo semantics).
    // Two independent roots: the second guard comes from the
    // fixture-as-function call (the plain name is taken by the injected
    // parameter, so the path form reaches the fixture fn).
    #[rstest]
    fn global_ignore_file_filters_every_root(#[with("walker")] workspace: TempWorkspace) {
        let sibling = crate::testing::workspace("walker");
        workspace.write("vendor/v.test", "v");
        workspace.write("top.test", "t");
        workspace.write("deep.test", "d");
        let global = workspace.join("global.ignore");
        workspace.write("global.ignore", "/top.test\nvendor/\ndeep.test\n");
        sibling.write("s.test", "s");
        sibling.write("deep.test", "deep");

        let walker = WorkspaceWalker::new(
            &[workspace.root().to_path_buf(), sibling.root().to_path_buf()],
            WorkspaceWalkConfig::default().with_global_ignore_file(Some(global)),
        )
        .expect("walker can be created");
        // The walk reports canonical paths; assertions use the walker's own
        // roots so the comparison holds where the temp dir sits behind a
        // symlink (macOS /var -> private/var).
        let (canonical_root, canonical_sibling) =
            (walker.roots()[0].clone(), walker.roots()[1].clone());
        let files = walker.files().expect("walk succeeds");
        assert!(
            files.contains(&canonical_root.join("global.ignore")),
            "the global file itself is a plain file: {files:?}",
        );
        assert!(
            !files.contains(&canonical_root.join("top.test")),
            "anchored pattern drops the root file",
        );
        assert!(
            !files.contains(&canonical_root.join("vendor/v.test")),
            "directory pattern prunes",
        );
        assert!(
            !files.contains(&canonical_root.join("deep.test"))
                && !files.contains(&canonical_sibling.join("deep.test")),
            "unanchored pattern matches in every root",
        );
        assert!(files.contains(&canonical_sibling.join("s.test")));
    }

    // One unreadable entry must not abort the scan; this test is unix-only
    // because the failure is injected with filesystem permissions.
    #[rstest]
    #[cfg(unix)]
    fn files_skips_unreadable_entries(#[with("walker")] workspace: TempWorkspace) {
        use std::os::unix::fs::PermissionsExt;

        // The mode is restored before the test ends so the guard's cleanup
        // can remove the restricted directory.
        const RESTORED_MODE: u32 = 0o755;

        fs::create_dir_all(workspace.join("bad")).expect("bad dir can be created");
        workspace.write("good.test", "good");
        fs::set_permissions(workspace.join("bad"), fs::Permissions::from_mode(0o000))
            .expect("permissions can be restricted");

        let walker = WorkspaceWalker::new(
            &[workspace.root().to_path_buf()],
            WorkspaceWalkConfig::default(),
        )
        .expect("walker can be created");
        let files = walker
            .files()
            .expect("walk succeeds despite unreadable entry");

        assert!(files.iter().any(|file| file.ends_with("good.test")));

        fs::set_permissions(
            workspace.join("bad"),
            fs::Permissions::from_mode(RESTORED_MODE),
        )
        .expect("permissions can be restored");
    }
}
