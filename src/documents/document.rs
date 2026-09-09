use async_lsp::lsp_types::Url;
use ropey::Rope;
use std::io::{Read, Result};
use std::sync::Arc;

#[cfg(feature = "tree-sitter")]
use async_lsp::lsp_types::{Position, Range};

use crate::server::DocumentMatcher;

#[cfg(feature = "tree-sitter")]
use crate::error::QueryError;
#[cfg(feature = "tree-sitter")]
use crate::tree_sitter::{Language, Node, QueryCursor, StreamingIterator, TextProvider, Tree};
#[cfg(feature = "tree-sitter")]
use crate::tree_sitter_utils::{lsp_position_to_ts_point, ts_range_to_lsp_range};

/// A snapshot of a text document tracked by the language server.
///
/// A cheap handle: cloning bumps a refcount. Writes never mutate a shared
/// inner — the store installs a fresh generation under its guard, so every
/// outstanding clone keeps the content it was created with.
///
/// Not meant to be updated by external sources, only read,
/// since the language server should be responsible for
/// always keeping the document up-to-date when edits occur.
///
/// # `tree-sitter`
///
/// With the `tree-sitter` crate feature enabled, the document
/// may also optionally store a [`tree_sitter::Language`] and
/// a parsed [`tree_sitter::Tree`] for the document's text.
///
/// If a `tree-sitter` language has been associated with the
/// document, the respective tree will be parsed using the initial
/// contents, and incrementally updated thereafter, transparently.
#[derive(Debug, Clone)]
pub struct Document {
    inner: Arc<DocumentInner>,
}

#[derive(Debug)]
pub(crate) struct DocumentInner {
    pub(crate) meta: Arc<DocumentMeta>,
    pub(crate) matcher: Option<Arc<DocumentMatcher>>,
    pub(crate) version: i32,
    pub(crate) text: Rope,
    #[cfg(feature = "tree-sitter")]
    pub(crate) tree_sitter_lang: Option<Language>,
    #[cfg(feature = "tree-sitter")]
    pub(crate) tree_sitter_tree: Option<Tree>,
}

/// Construction-immutable identity: shared untouched across write
/// generations so a copy-on-write costs refcounts, not string clones.
#[derive(Debug)]
pub(crate) struct DocumentMeta {
    uri: Url,
    language: String,
}

/// The tree-sitter half of a document's contents (grammar and parsed tree),
/// or nothing without the feature — one constructor signature across the
/// feature gate.
#[cfg(feature = "tree-sitter")]
pub(crate) type DocumentSyntax = (Option<Language>, Option<Tree>);

#[cfg(not(feature = "tree-sitter"))]
pub(crate) type DocumentSyntax = ();

impl Document {
    /// Builds a document from its flat parts.
    pub(crate) fn from_parts(
        uri: Url,
        language: String,
        matcher: Option<Arc<DocumentMatcher>>,
        version: i32,
        text: Rope,
        syntax: DocumentSyntax,
    ) -> Self {
        Self::from_shared_meta(
            Arc::new(DocumentMeta { uri, language }),
            matcher,
            version,
            text,
            syntax,
        )
    }

    /// Builds a fresh generation sharing an existing [`DocumentMeta`]: the
    /// store's copy-on-write install path, costing refcounts instead of
    /// string clones.
    pub(crate) fn from_shared_meta(
        meta: Arc<DocumentMeta>,
        matcher: Option<Arc<DocumentMatcher>>,
        version: i32,
        text: Rope,
        syntax: DocumentSyntax,
    ) -> Self {
        #[cfg(feature = "tree-sitter")]
        let (tree_sitter_lang, tree_sitter_tree) = syntax;
        #[cfg(not(feature = "tree-sitter"))]
        let () = syntax;

        Self {
            inner: Arc::new(DocumentInner {
                meta,
                matcher,
                version,
                text,
                #[cfg(feature = "tree-sitter")]
                tree_sitter_lang,
                #[cfg(feature = "tree-sitter")]
                tree_sitter_tree,
            }),
        }
    }

    /// The shared inner generation, for the store's copy-on-write installs.
    pub(crate) fn inner_arc(&self) -> &Arc<DocumentInner> {
        &self.inner
    }

    /// Returns the URL of the document.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.inner.meta.uri
    }

    /// Returns the text of the document, as
    /// its underlying [`Rope`] representation.
    ///
    /// It is usually easier to use one of the several convenience
    /// methods that [`Document`] provides for accessing and searching
    /// through text, but this method exists as an escape hatch.
    #[must_use]
    pub fn text(&self) -> &Rope {
        &self.inner.text
    }

    /// Returns a reader over the full text in the document.
    #[must_use]
    pub fn text_reader(&self) -> DocumentReader<'_> {
        DocumentReader {
            chunks: self.inner.text.chunks(),
            current: None,
            current_offset: 0,
        }
    }

    /// Returns the full text of the document, as a string.
    ///
    /// When possible, prefer [`Document::text_reader`]
    /// for improved performance and less allocations.
    #[must_use]
    pub fn text_contents(&self) -> String {
        self.inner.text.to_string()
    }

    /// Returns the full text of the document, as bytes.
    ///
    /// When possible, prefer [`Document::text_reader`]
    /// for improved performance and less allocations.
    #[must_use]
    pub fn text_bytes(&self) -> Vec<u8> {
        self.inner.text.bytes().collect()
    }

    /// Returns the version of the document.
    ///
    /// This number should be strictly increasing with
    /// each change to the document, including undo/redo.
    #[must_use]
    pub fn version(&self) -> i32 {
        self.inner.version
    }

    /// Returns the language of the document.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.inner.meta.language
    }

    /// Returns the name of the document matcher that this document
    /// was matched against, if one was configured, and either a
    /// language or glob pattern was matched against.
    ///
    /// See [`DocumentMatcher`] for more information.
    #[must_use]
    pub fn matched_name(&self) -> Option<&str> {
        self.inner.matcher.as_ref().map(|matcher| matcher.name())
    }
}

#[cfg(feature = "tree-sitter")]
impl Document {
    /// Returns `true` if the document has an assigned tree-sitter language, otherwise `false`.
    #[must_use]
    pub fn has_syntax_language(&self) -> bool {
        self.inner.tree_sitter_lang.is_some()
    }

    /// Returns `true` if the document has a parsed tree-sitter syntax tree, otherwise `false`.
    #[must_use]
    pub fn has_syntax_tree(&self) -> bool {
        self.inner.tree_sitter_tree.is_some()
    }

    /// Returns the UTF-8 text of a [`Node`].
    ///
    /// # Panics
    ///
    /// Panics if the node's byte range is not within this document.
    #[must_use]
    pub fn node_text(&self, node: Node) -> String {
        self.inner.text.byte_slice(node.byte_range()).to_string()
    }

    /// Returns a [`Node`] at the root of the syntax tree, if one exists.
    #[must_use]
    pub fn node_at_root(&self) -> Option<Node<'_>> {
        self.inner
            .tree_sitter_tree
            .as_ref()
            .map(|tree| tree.root_node())
    }

    /// Returns a [`Node`] at the given LSP position, if one exists.
    #[must_use]
    pub fn node_at_position(&self, position: Position) -> Option<Node<'_>> {
        let root = self.node_at_root()?;
        let point = lsp_position_to_ts_point(position);
        root.descendant_for_point_range(point, point)
    }

    /// Similar to [`Document::node_at_position`], except the node must be named.
    #[must_use]
    pub fn node_at_position_named(&self, position: Position) -> Option<Node<'_>> {
        let root = self.node_at_root()?;
        let point = lsp_position_to_ts_point(position);
        root.named_descendant_for_point_range(point, point)
    }

    /// Creates and runs a query for the given query string.
    ///
    /// The compiled query is cached on the document's matcher, so repeated
    /// queries with the same source string compile once.
    ///
    /// # Panics
    ///
    /// Panics if the stored tree does not correspond to the current text (a
    /// bug: every mutation writes text and tree together), and a capture's
    /// byte range is therefore out of bounds for the rope.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError::NoTree`] when the document has no tree-sitter
    /// language or parsed tree attached, and [`QueryError::InvalidQuery`]
    /// when the query string fails to compile.
    pub fn query(
        &self,
        query: impl AsRef<str>,
    ) -> std::result::Result<Vec<DocumentQueryCapture>, QueryError> {
        let tree = self
            .inner
            .tree_sitter_tree
            .as_ref()
            .ok_or(QueryError::NoTree)?;

        // A parsed tree implies the document's grammar and its matcher (the
        // store derives both from the same match), so the cache consult covers
        // every reachable path; a `None` matcher has no grammar to compile.
        let query = self
            .inner
            .matcher
            .as_ref()
            .ok_or(QueryError::NoTree)?
            .compiled_query(query.as_ref())?;
        let query_names = query.capture_names();

        let mut cursor = QueryCursor::new();
        let mut it = cursor.matches(
            &query,
            tree.root_node(),
            RopeText {
                rope: &self.inner.text,
            },
        );

        let mut items = Vec::new();
        while let Some(matched) = it.next() {
            for capture in matched.captures {
                let name = query_names[capture.index as usize].to_owned();
                let text = self
                    .inner
                    .text
                    .byte_slice(capture.node.byte_range())
                    .chunks()
                    .collect::<String>();
                let range = ts_range_to_lsp_range(capture.node.range());
                items.push(DocumentQueryCapture { name, text, range });
            }
        }
        Ok(items)
    }
}

/// Serves tree-sitter's text-provider callbacks from the document's rope,
/// yielding the requested node's byte range as rope chunks — avoiding the
/// whole-file `String` that a `&[u8]` provider would require.
#[cfg(feature = "tree-sitter")]
struct RopeText<'text> {
    rope: &'text Rope,
}

#[cfg(feature = "tree-sitter")]
impl<'text> TextProvider<&'text str> for RopeText<'text> {
    type I = ropey::iter::Chunks<'text>;

    fn text(&mut self, node: Node) -> Self::I {
        self.rope.byte_slice(node.byte_range()).chunks()
    }
}

impl AsRef<Rope> for Document {
    fn as_ref(&self) -> &Rope {
        &self.inner.text
    }
}

/// A reader over the full text contents of a document.
///
/// Created by calling [`Document::text_reader`].
pub struct DocumentReader<'d> {
    chunks: ropey::iter::Chunks<'d>,
    current: Option<&'d str>,
    current_offset: usize,
}

impl Read for DocumentReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut written = 0;

        while written < buf.len() {
            if self.current.is_none() {
                self.current = self.chunks.next();
                self.current_offset = 0;
            }

            let Some(chunk) = self.current else {
                break;
            };

            let remaining = &chunk.as_bytes()[self.current_offset..];
            let len = remaining.len().min(buf.len() - written);
            buf[written..written + len].copy_from_slice(&remaining[..len]);

            written += len;
            self.current_offset += len;

            if self.current_offset == chunk.len() {
                self.current = None;
                self.current_offset = 0;
            }
        }

        Ok(written)
    }
}

/// A capture from a tree-sitter query on a document.
///
/// Created by calling [`Document::query`].
#[cfg(feature = "tree-sitter")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentQueryCapture {
    /// The capture name
    pub name: String,
    /// The textual contents of the capture
    pub text: String,
    /// The document range of the capture
    pub range: Range,
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;
    use std::time::Duration;

    use ropey::Rope;

    use super::{Document, DocumentReader};

    /// Ceiling for [`read_to_end_bounded`]: a correct reader drains any text
    /// in microseconds, so a full wait means the read loop is livelocked —
    /// the test must fail fast, not hang.
    const READ_TIMEOUT: Duration = Duration::from_secs(5);

    /// Reads fresh `text` to exhaustion through a [`DocumentReader`] with
    /// `buf_len`-byte buffers on a worker thread, bounding the whole loop by
    /// [`READ_TIMEOUT`]: a livelocked or panicking `read` fails the bounded
    /// wait instead of hanging the test. `None` means the loop never
    /// terminated.
    fn read_to_end_bounded(text: Rope, buf_len: usize) -> Option<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = DocumentReader {
                chunks: text.chunks(),
                current: None,
                current_offset: 0,
            };

            let mut buf = vec![0; buf_len];
            let mut actual = Vec::new();
            for _ in 0..=text.len_bytes() {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => actual.extend_from_slice(&buf[..n]),
                }
            }

            let _ = tx.send(actual);
        });
        rx.recv_timeout(READ_TIMEOUT).ok()
    }

    #[test]
    fn reader_preserves_unread_chunk_bytes() {
        // A one-byte buffer routes every fill through the chunk-advance
        // path, and the loop must end at EOF (Ok(0)) rather than spin.
        let actual = read_to_end_bounded(Rope::from_str("hello"), 1).expect("read loop terminates");
        assert_eq!(actual, b"hello");
    }

    #[test]
    fn read_fills_multi_chunk_buffers_across_chunks() {
        // 400 patterns = 4000 ASCII bytes: several rope chunks at ropey's
        // ~1 KB chunk size.
        const PATTERN: &str = "0123456789";
        const PATTERN_REPEATS: usize = 400;

        let text = PATTERN.repeat(PATTERN_REPEATS);
        let rope = Rope::from_str(&text);
        assert!(
            rope.chunks().count() > 1,
            "fixture spans multiple rope chunks",
        );

        // The fill sequence crosses chunk boundaries, so a mutated fill
        // bound overruns the buffer instead of answering with exact bytes.
        // Ropey-upgrade sensitivity: the `document.rs:359` panic-kill rides on
        // a chunk boundary landing misaligned with the 8-byte buffer (4000 % 8
        // keeps a straddling final read); the byte-fidelity assert below is
        // boundary-independent and survives any chunking change.
        let actual = read_to_end_bounded(rope, 8).expect("read loop terminates");
        assert_eq!(actual, text.as_bytes());
    }

    #[test]
    fn text_bytes_returns_the_document_bytes() {
        #[cfg(feature = "tree-sitter")]
        let syntax = (None, None);
        #[cfg(not(feature = "tree-sitter"))]
        let syntax = ();

        let text = "🙂abc";
        let document = Document::from_parts(
            crate::testing::url("text-bytes.json"),
            "json".into(),
            None,
            1,
            Rope::from_str(text),
            syntax,
        );

        assert_eq!(document.text_bytes(), text.as_bytes());
    }

    /// Opens `json_text` as a JSON document plus a grammarless `plain.txt`
    /// neighbor against the shared json matchers, returning both document
    /// snapshots. The tracking state is dropped; the snapshots stand alone.
    #[cfg(feature = "tree-sitter")]
    fn opened_json_fixtures(
        root: &std::path::Path,
        json_text: impl Into<String>,
    ) -> (Document, Document) {
        use async_lsp::{
            ClientSocket,
            lsp_types::{DidOpenTextDocumentParams, TextDocumentItem, Url},
        };

        use crate::server::{DocumentMatcher, Server, ServerOptions, ServerState};

        struct JsonServer;

        impl Server for JsonServer {
            fn server_document_matchers() -> Vec<DocumentMatcher> {
                crate::testing::json_matchers()
            }
        }

        let mut state = ServerState::with_options::<JsonServer>(
            ClientSocket::new_closed(),
            &ServerOptions::default(),
        );

        let uri = Url::from_file_path(root.join("doc.json")).expect("path converts to a URL");
        let _ = state.handle_document_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "json".into(), 1, json_text.into()),
        });
        let document = state.document(&uri).expect("document is tracked");

        // A language string no matcher claims, on a URL no glob matches:
        // tracked, but grammarless.
        let plain_uri =
            Url::from_file_path(root.join("plain.txt")).expect("path converts to a URL");
        let _ = state.handle_document_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                plain_uri.clone(),
                "plaintext".into(),
                1,
                "x".into(),
            ),
        });
        let plain = state.document(&plain_uri).expect("document is tracked");

        (document, plain)
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn query_errors_on_invalid_query_and_grammarless_documents() {
        use std::fs;

        use crate::error::QueryError;

        let root = crate::testing::temp_workspace("documents", "query");
        let (document, plain) = opened_json_fixtures(&root, r#"{"a": 1}"#);

        // Malformed query syntax: the typed compile failure, not a bare None.
        assert!(matches!(
            document.query("(node"),
            Err(QueryError::InvalidQuery { .. })
        ));

        // A document with no grammar/tree answers NoTree, distinctly.
        assert!(matches!(plain.query("(node"), Err(QueryError::NoTree)));

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn node_accessors_resolve_positions_in_parsed_documents() {
        use std::fs;

        use crate::testing::line_position;

        let root = crate::testing::temp_workspace("documents", "node-accessors");
        let (document, plain) = opened_json_fixtures(&root, r#"{"aa": 1, "b": 2}"#);

        // A parsed document has a root; a grammarless one has nothing.
        let tree_root = document.node_at_root().expect("parsed document has a root");
        assert_eq!(tree_root.kind(), "document");
        assert!(plain.node_at_root().is_none());
        assert!(plain.node_at_position(line_position(0, 0)).is_none());
        assert!(plain.node_at_position_named(line_position(0, 0)).is_none());

        // The smallest node at the pair key's inner column is the key's
        // string content, not the string around it.
        let key = document
            .node_at_position(line_position(0, 2))
            .expect("position inside the tree");
        assert_eq!(key.kind(), "string_content");
        assert_eq!(key.byte_range(), 2..4);

        // At the anonymous ':' the unfiltered accessor answers the token,
        // the named-only one its nearest named ancestor, the pair. Past the
        // source the points clamp to the outermost spanning node — only a
        // grammarless document answers None here.
        let colon = document
            .node_at_position(line_position(0, 5))
            .expect("position inside the tree");
        assert_eq!(colon.kind(), ":");
        let named = document
            .node_at_position_named(line_position(0, 5))
            .expect("a named node spans the anonymous token");
        assert_eq!(named.kind(), "pair");
        let clamped = document
            .node_at_position(line_position(5, 0))
            .expect("out-of-tree positions clamp to a node");
        assert_eq!(clamped.kind(), "document");

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn node_text_returns_the_node_slice() {
        use std::fs;

        use crate::testing::line_position;

        let root = crate::testing::temp_workspace("documents", "node-text");
        let (document, _plain) = opened_json_fixtures(&root, r#"{"aa": 1, "b": 2}"#);

        let tree_root = document.node_at_root().expect("parsed document has a root");
        assert_eq!(document.node_text(tree_root), r#"{"aa": 1, "b": 2}"#);

        let key = document
            .node_at_position(line_position(0, 2))
            .expect("position inside the tree");
        assert_eq!(document.node_text(key), "aa");

        let value = document
            .node_at_position(line_position(0, 7))
            .expect("position inside the tree");
        assert_eq!(document.node_text(value), "1");

        fs::remove_dir_all(root).expect("temp workspace can be removed");
    }
}
