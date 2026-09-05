use std::ops::ControlFlow;

use async_lsp::{
    Result,
    lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, FileChangeType, FileDelete, FileEvent, FileRename, Range, Url,
    },
};
use ropey::Rope;

#[cfg(feature = "tree-sitter")]
use async_lsp::lsp_types::TextDocumentContentChangeEvent;

#[cfg(feature = "tree-sitter")]
use tree_sitter::{InputEdit, Parser, Point};

use super::workspace::url_is_in_roots;
use super::{DocumentEntry, DocumentOrigin, ServerState};

use crate::{
    documents::Document,
    text_utils::{Encoding, position_to_encoding},
};

impl ServerState {
    pub(super) fn insert_document(
        &self,
        url: Url,
        text: String,
        version: i32,
        language: String,
        origin: DocumentOrigin,
    ) {
        #[cfg(feature = "tree-sitter")]
        let mut tree_sitter_lang = self
            .matchers
            .find(&url, language.as_str())
            .and_then(|m| m.lang_grammar());

        #[cfg(feature = "tree-sitter")]
        let tree_sitter_tree = if let Some(lang) = tree_sitter_lang.as_ref() {
            let mut parser = Parser::new();
            if parser.set_language(lang).is_ok() {
                parser.parse(&text, None)
            } else {
                tree_sitter_lang.take();
                None
            }
        } else {
            None
        };

        let matcher = self.matchers.find(&url, &language);

        self.documents.insert(
            url.clone(),
            DocumentEntry {
                document: Document {
                    uri: url,
                    text: Rope::from(text),
                    version,
                    language,
                    matcher,
                    #[cfg(feature = "tree-sitter")]
                    tree_sitter_lang,
                    #[cfg(feature = "tree-sitter")]
                    tree_sitter_tree,
                },
                origin,
            },
        );
    }

    pub(crate) fn handle_document_open(
        &mut self,
        params: DidOpenTextDocumentParams,
    ) -> ControlFlow<Result<()>> {
        self.insert_document(
            params.text_document.uri,
            params.text_document.text,
            params.text_document.version,
            params.text_document.language_id,
            DocumentOrigin::Open,
        );

        ControlFlow::Continue(())
    }

    pub(crate) fn handle_document_close(
        &self,
        params: DidCloseTextDocumentParams,
    ) -> ControlFlow<Result<()>> {
        let url = params.text_document.uri;
        let Some(entry) = self.documents.get(&url) else {
            return ControlFlow::Continue(());
        };

        let language = entry.document.language.clone();
        let roots = self.workspace_roots();
        let keep_as_workspace = self.workspace_diagnostics.enabled()
            && self.matchers.find_url(&url).is_some()
            && url_is_in_roots(&url, &roots);
        drop(entry);

        if !keep_as_workspace {
            self.documents.remove(&url);
            return ControlFlow::Continue(());
        }

        // arch-lint: allow(no-sync-io) reason="LSP notification handlers must stay synchronous per the spec; closing keeps a disk snapshot via std::fs"
        if let Ok(text) = std::fs::read_to_string(url.path()) {
            self.insert_document(url, text, 0, language, DocumentOrigin::Workspace);
        } else {
            self.documents.remove(&url);
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn handle_document_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> ControlFlow<Result<()>> {
        let Some(mut entry) = self.documents.get_mut(&params.text_document.uri) else {
            return ControlFlow::Continue(());
        };

        entry.origin = DocumentOrigin::Open;
        let doc = &mut entry.document;
        doc.version = params.text_document.version;

        let encoding = self.encoding.as_ref();

        // Try to perform an incremental update on the document contents, using the changes
        let mut incremental_update_failed = false;
        #[cfg(feature = "tree-sitter")]
        let mut tree_sitter_incrementally_edited = false;

        for change in params.content_changes {
            let Some(range) = change.range else {
                doc.text = Rope::from_str(&change.text);

                #[cfg(feature = "tree-sitter")]
                {
                    let mut parser = doc_parser(doc);
                    doc.tree_sitter_tree = parser
                        .as_mut()
                        .and_then(|parser| parser.parse(doc.text_contents(), None));
                }

                continue;
            };

            // 1. Convert the LSP positions, using their arbitrary encoding,
            //    to what Ropey expects to use for its incremental updates
            let Some((start_char_absolute, end_char_absolute)) =
                change_char_range(doc, range, *encoding)
            else {
                incremental_update_failed = true;
                break;
            };

            // 3. Perform incremental edit on the syntax tree as well, if enabled
            //    Note that we need to do this before updating the document contents
            #[cfg(feature = "tree-sitter")]
            if let Some(tree) = doc.tree_sitter_tree.as_mut()
                && let Some(edit) = tree_sitter_edit(
                    &doc.text,
                    &change,
                    start_char_absolute,
                    end_char_absolute,
                    *encoding,
                )
            {
                tree.edit(&edit);
                tree_sitter_incrementally_edited = true;
            }

            // 4. Finally, try to incrementally update the document contents
            if doc
                .text
                .try_remove(start_char_absolute..end_char_absolute)
                .is_err()
                || doc
                    .text
                    .try_insert(start_char_absolute, &change.text)
                    .is_err()
            {
                incremental_update_failed = true;
                break;
            }
        }

        // If the incremental update was successful, and we applied edits to the syntax
        // tree, we must finalize those changes by parsing using tree-sitter once again
        #[cfg(feature = "tree-sitter")]
        if !incremental_update_failed
            && tree_sitter_incrementally_edited
            && let Some(tree) = doc.tree_sitter_tree.as_ref()
        {
            #[expect(
                clippy::expect_used,
                reason = "invariant: a document carrying a tree always has its parser"
            )]
            let mut parser = doc_parser(doc).expect("has tree - must have parser");
            let updated_tree = parser.parse(doc.text_contents(), Some(tree));
            doc.tree_sitter_tree = updated_tree;
        }

        // If the incremental update failed, we will re-insert the entire file instead
        // Note: we must first drop the document reference to prevent a deadlock
        if incremental_update_failed {
            let uri = doc.uri.clone();
            let version = doc.version();
            let language = doc.language.clone();

            drop(entry);

            self.recover_failed_incremental_update(uri, version, language);
        }

        ControlFlow::Continue(())
    }

    /// Recovers a document whose incremental update failed: reload from
    /// disk when possible, otherwise keep the last-known text (and re-parse
    /// its tree under the tree-sitter feature).
    fn recover_failed_incremental_update(&mut self, uri: Url, version: i32, language: String) {
        // NOTE: We must read the contents of the file synchronously
        // as the fallback here, since notification handlers are actually
        // synchronous both according to LSP spec and the async-lsp crate.
        // The re-read intentionally replaces the in-memory text, whose
        // edits were only partially applied; this discards unsaved
        // editor changes, which is the accepted trade-off of the
        // synchronous-handler constraint.
        // arch-lint: allow(no-sync-io) reason="LSP notification handlers must stay synchronous per the spec; the failed-incremental fallback re-reads via std::fs"
        if let Ok(text) = std::fs::read_to_string(uri.path()) {
            self.insert_document(uri, text, version, language, DocumentOrigin::Open);
        } else {
            // Keeping the last-known (possibly partially edited) text is
            // better than dropping the document: the editor still
            // considers it open, and handlers keep resolving it.
            tracing::warn!(
                "did_change: incremental update failed and '{}' could not be re-read; keeping last-known text",
                uri
            );

            // The kept tree may already carry `tree.edit()` calls for
            // changes whose rope edits never applied, and the finalize
            // re-parse above never ran; re-parse the kept text from
            // scratch so the tree cannot diverge from the text.
            #[cfg(feature = "tree-sitter")]
            if let Some(mut entry) = self.documents.get_mut(&uri) {
                let doc = &mut entry.document;
                let mut parser = doc_parser(doc);
                doc.tree_sitter_tree = parser
                    .as_mut()
                    .and_then(|parser| parser.parse(doc.text_contents(), None));
            }
        }
    }

    pub(crate) fn handle_document_save(
        &self,
        params: DidSaveTextDocumentParams,
    ) -> ControlFlow<Result<()>> {
        let url = params.text_document.uri;
        let Some(mut entry) = self.documents.get_mut(&url) else {
            return ControlFlow::Continue(());
        };

        // NOTE: We must read the contents of the file synchronously
        // as the fallback here, since notification handlers are actually
        // synchronous both according to LSP spec and the async-lsp crate
        let text = if let Some(text) = &params.text {
            Rope::from_str(text)
            // arch-lint: allow(no-sync-io) reason="LSP notification handlers must stay synchronous per the spec; the no-text fallback reads via std::fs"
        } else if let Ok(text) = std::fs::read_to_string(url.path()) {
            Rope::from_str(&text)
        } else {
            drop(entry);
            self.documents.remove(&url);
            return ControlFlow::Continue(());
        };
        let doc = &mut entry.document;
        doc.text = text;

        // The implementor may want to know what, if any, document
        // matcher we may have matched against - so let's save that
        let matcher = self.matchers.find(doc.url(), doc.language());
        doc.matcher.clone_from(&matcher);

        // Since we just read the entire file contents, we will also
        // re-create the entire tree-sitter tree using those new contents
        #[cfg(feature = "tree-sitter")]
        {
            let mut tree_sitter_lang = matcher.and_then(|m| m.lang_grammar());

            let tree_sitter_tree = if let Some(lang) = tree_sitter_lang.as_ref() {
                let mut parser = Parser::new();
                if parser.set_language(lang).is_ok() {
                    parser.parse(doc.text_contents(), None)
                } else {
                    tree_sitter_lang.take();
                    None
                }
            } else {
                None
            };

            doc.tree_sitter_lang = tree_sitter_lang;
            doc.tree_sitter_tree = tree_sitter_tree;
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn handle_watched_files_change(
        &self,
        changes: Vec<FileEvent>,
    ) -> ControlFlow<Result<()>> {
        for event in changes {
            let Some(entry) = self.documents.get(&event.uri) else {
                // Untracked URIs are not loaded here - the next workspace
                // scan picks up new files instead.
                continue;
            };
            if entry.origin == DocumentOrigin::Open {
                // The editor owns open documents; disk events never touch them.
                continue;
            }

            if event.typ == FileChangeType::DELETED {
                drop(entry);
                self.documents.remove_if(&event.uri, |_, entry| {
                    entry.origin == DocumentOrigin::Workspace
                });
                continue;
            }

            // Created/Changed: replace the tracked snapshot with a fresh read
            // of the file. NOTE: we must read the contents of the file
            // synchronously, since notification handlers are actually
            // synchronous both according to LSP spec and the async-lsp crate.
            let language = entry.document.language.clone();
            drop(entry);
            // arch-lint: allow(no-sync-io) reason="LSP notification handlers must stay synchronous per the spec; the watched-files refresh re-reads via std::fs"
            if let Ok(text) = std::fs::read_to_string(event.uri.path()) {
                self.insert_document(event.uri, text, 0, language, DocumentOrigin::Workspace);
            } else {
                // Keep the old snapshot: a stale tracked document beats
                // dropping one that handlers may still be resolving.
                tracing::warn!(
                    "did_change_watched_files: '{}' could not be re-read; keeping last-known snapshot",
                    event.uri
                );
            }
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn handle_files_renamed(&self, files: Vec<FileRename>) -> ControlFlow<Result<()>> {
        for file in files {
            self.remove_workspace_document_by_uri_string(&file.old_uri);
        }

        ControlFlow::Continue(())
    }

    pub(crate) fn handle_files_deleted(&self, files: Vec<FileDelete>) -> ControlFlow<Result<()>> {
        for file in files {
            self.remove_workspace_document_by_uri_string(&file.uri);
        }

        ControlFlow::Continue(())
    }

    /// Drops the tracked Workspace-origin snapshot for a URI supplied as a
    /// client string: unparseable URIs are traced and skipped, and open
    /// documents are never touched (the next workspace scan re-adds what
    /// still exists on disk).
    fn remove_workspace_document_by_uri_string(&self, raw_uri: &str) {
        let Ok(url) = Url::parse(raw_uri) else {
            tracing::warn!("file operation: unparseable URI '{raw_uri}' skipped");
            return;
        };

        self.documents
            .remove_if(&url, |_, entry| entry.origin == DocumentOrigin::Workspace);
    }
}

#[cfg(feature = "tree-sitter")]
fn doc_parser(doc: &Document) -> Option<Parser> {
    let lang = doc.tree_sitter_lang.as_ref()?;
    let mut parser = Parser::new();
    if parser.set_language(lang).is_ok() {
        Some(parser)
    } else {
        None
    }
}

/// Converts the range of an incremental change, using its arbitrary encoding,
/// to the char offsets Ropey expects for its incremental updates.
///
/// Returns `None` when either endpoint's line is out of bounds.
fn change_char_range(doc: &Document, range: Range, encoding: Encoding) -> Option<(usize, usize)> {
    let start_line_char_offset = doc.text.try_line_to_char(range.start.line as usize).ok()?;
    let start = position_to_encoding(doc.text(), range.start, encoding, Encoding::UTF32);
    let start_char_absolute = start_line_char_offset + start.character as usize;

    let end_line_char_offset = doc.text.try_line_to_char(range.end.line as usize).ok()?;
    let end = position_to_encoding(doc.text(), range.end, encoding, Encoding::UTF32);
    let end_char_absolute = (end_line_char_offset + end.character as usize)
        .max(start_char_absolute)
        .min(doc.text.len_chars());

    Some((start_char_absolute, end_char_absolute))
}

/// Builds the tree-sitter incremental edit for one content change,
/// returning `None` when the changed range cannot be resolved.
///
/// Reads the yet-to-be-changed rope, so it must be called before the
/// document contents are updated.
#[cfg(feature = "tree-sitter")]
fn tree_sitter_edit(
    text: &Rope,
    change: &TextDocumentContentChangeEvent,
    start_char_absolute: usize,
    end_char_absolute: usize,
    encoding: Encoding,
) -> Option<InputEdit> {
    let range = change.range?;

    // Compute some byte offsets based on the yet-to-be-changed rope
    let start_byte = text.char_to_byte(start_char_absolute);
    let old_end_byte = text.char_to_byte(end_char_absolute);
    let new_end_byte = start_byte + change.text.len();

    // Convert the start and old end positions to the correct encoding
    let start_position = position_to_encoding(text, range.start, encoding, Encoding::UTF8);
    let old_end_position = position_to_encoding(text, range.end, encoding, Encoding::UTF8);

    // Compute the new end point based on the contents of the edit
    let (new_end_row, new_end_col_bytes) = change.text.chars().fold(
        (
            start_position.line as usize,
            start_position.character as usize,
        ),
        |(row, col_bytes), ch| {
            if ch == '\n' {
                (row + 1, 0)
            } else {
                (row, col_bytes + ch.len_utf8())
            }
        },
    );

    // Finally, build the edit for incrementally updating the syntax tree
    Some(InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: Point {
            row: start_position.line as usize,
            column: start_position.character as usize,
        },
        old_end_position: Point {
            row: old_end_position.line as usize,
            column: old_end_position.character as usize,
        },
        new_end_position: Point {
            row: new_end_row,
            column: new_end_col_bytes,
        },
    })
}
