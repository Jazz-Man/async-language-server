use crate::documents::DocumentMatcher;
use async_lsp::lsp_types::Url;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// One walked file, fully resolved: the path, its URL, and the matcher that
/// claimed it. Matched-only — non-matching walk output never enters the cache.
pub(crate) type WalkedFile = (PathBuf, Url, Arc<DocumentMatcher>);

/// The workspace file list between invalidation events. One automatic truth:
/// when `dirty` is set, or no entries exist, the next refresh walks again.
#[derive(Debug, Default)]
pub(crate) struct WalkCache {
    inner: Mutex<WalkCacheInner>,
}

#[derive(Debug, Default)]
struct WalkCacheInner {
    entries: Option<Vec<WalkedFile>>,
    dirty: bool,
}

impl WalkCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The cached list, fresh enough to serve: present and not dirty.
    /// The guard never escapes the call.
    pub(crate) fn get_valid(&self) -> Option<Vec<WalkedFile>> {
        let inner = self.lock();
        match inner.entries.as_ref() {
            Some(entries) if !inner.dirty => Some(entries.clone()),
            _ => None,
        }
    }

    pub(crate) fn store(&self, entries: Vec<WalkedFile>) {
        let mut inner = self.lock();
        inner.entries = Some(entries);
        inner.dirty = false;
    }

    pub(crate) fn invalidate(&self) {
        self.lock().dirty = true;
    }

    /// The folders-changed form: the old list is not merely stale, its root
    /// premise moved.
    pub(crate) fn clear(&self) {
        let mut inner = self.lock();
        inner.entries = None;
        inner.dirty = true;
    }

    fn lock(&self) -> MutexGuard<'_, WalkCacheInner> {
        // Poisoning recovers: the guarded fields are a list plus a flag, and
        // a panicked holder leaves both in a state the next writer overwrites.
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}
