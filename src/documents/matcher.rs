use std::{collections::HashMap, sync::Arc};

use async_lsp::lsp_types::Url;
use globset::{Glob, GlobSet};

#[cfg(feature = "tree-sitter")]
use tree_sitter::Language;

/// Associates documents with a name by URL glob and/or language id.
///
/// May associate an optional tree-sitter language grammar with matched
/// documents when the tree-sitter feature is enabled.
///
/// # Examples
///
/// ```
/// use async_language_server::server::DocumentMatcher;
///
/// let matcher = DocumentMatcher::new("json")
///     .with_url_globs(["**/*.json", "*.jsonc"])
///     .with_lang_strings(["json", "jsonc"]);
///
/// assert_eq!(matcher.name(), "json");
/// ```
#[derive(Debug, Default, Clone)]
pub struct DocumentMatcher {
    /// The name of the document matcher.
    name: String,
    /// Optional globs to match documents based on their URLs.
    url_globs: Vec<String>,
    /// Strings to match documents based on their language identifiers.
    lang_strings: Vec<String>,
    /// The tree-sitter language grammar to associate with the matched document.
    #[cfg(feature = "tree-sitter")]
    lang_grammar: Option<Language>,
}

impl DocumentMatcher {
    /// Returns the matcher's name.
    ///
    /// The name is exposed on matched documents through
    /// [`crate::server::Document::matched_name`]; it does not
    /// need to be unique.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Creates a new document matcher with the given name.
    ///
    /// The name is exposed on matched documents through
    /// [`crate::server::Document::matched_name`]; it does not
    /// need to be unique.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url_globs: Vec::new(),
            lang_strings: Vec::new(),
            #[cfg(feature = "tree-sitter")]
            lang_grammar: None,
        }
    }

    /// Adds the given URL globs to the matcher.
    #[must_use]
    pub fn with_url_globs<I, U>(mut self, url_globs: I) -> Self
    where
        I: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.url_globs.extend(url_globs.into_iter().map(Into::into));
        self
    }

    /// Adds the given language identifiers to the matcher.
    #[must_use]
    pub fn with_lang_strings<I, U>(mut self, lang_strings: I) -> Self
    where
        I: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.lang_strings
            .extend(lang_strings.into_iter().map(Into::into));
        self
    }

    /// Sets the tree-sitter language grammar
    /// to associate with the document matcher.
    #[cfg(feature = "tree-sitter")]
    #[must_use]
    pub fn with_lang_grammar(mut self, lang_grammar: Language) -> Self {
        self.lang_grammar = Some(lang_grammar);
        self
    }

    pub(crate) fn lang_strings(&self) -> &[String] {
        &self.lang_strings
    }

    #[cfg(feature = "tree-sitter")]
    pub(crate) fn lang_grammar(&self) -> Option<Language> {
        self.lang_grammar.clone()
    }
}

/// Private struct created from individual [`DocumentMatcher`]s
/// to easily match against documents and find the original matcher.
#[derive(Debug, Default, Clone)]
pub(crate) struct DocumentMatchers {
    globsets: Arc<Vec<(GlobSet, Arc<DocumentMatcher>)>>,
    languages: Arc<HashMap<String, Arc<DocumentMatcher>>>,
}

impl DocumentMatchers {
    pub(crate) fn new(it: impl IntoIterator<Item = DocumentMatcher>) -> Self {
        let mut globsets = Vec::new();
        let mut languages = HashMap::new();

        for matcher in it {
            let matcher = Arc::new(matcher);

            let mut globset = GlobSet::builder();
            let mut globset_any = false;
            for glob in &matcher.url_globs {
                if let Ok(glob) = Glob::new(glob) {
                    globset.add(glob);
                    globset_any = true;
                } else {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        "Encountered invalid glob pattern '{}' in matcher '{}'",
                        glob,
                        matcher.name
                    );
                }
            }

            if globset_any {
                if let Ok(globset) = globset.build() {
                    globsets.push((globset, Arc::clone(&matcher)));
                } else {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Encountered invalid globset in matcher '{}'", matcher.name);
                }
            }

            for lang in &matcher.lang_strings {
                let mut lang = lang.trim().to_owned();
                lang.make_ascii_lowercase();
                languages.insert(lang, Arc::clone(&matcher));
            }
        }

        Self {
            globsets: Arc::new(globsets),
            languages: Arc::new(languages),
        }
    }

    pub(crate) fn find(&self, url: &Url, lang: &str) -> Option<Arc<DocumentMatcher>> {
        let mut lang = lang.trim().to_owned();
        lang.make_ascii_lowercase();
        self.languages
            .get(lang.as_str())
            .cloned()
            .or_else(|| self.find_url(url))
    }

    pub(crate) fn find_url(&self, url: &Url) -> Option<Arc<DocumentMatcher>> {
        url.to_file_path().ok().and_then(|p| {
            self.globsets
                .iter()
                .find(|(globset, _)| globset.is_match(&p))
                .map(|(_, matcher)| Arc::clone(matcher))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use async_lsp::lsp_types::Url;

    use super::{DocumentMatcher, DocumentMatchers};

    #[test]
    fn find_matches_language_strings_case_insensitively() {
        let matchers =
            DocumentMatchers::new([DocumentMatcher::new("json").with_lang_strings(["Json"])]);

        let found = matchers
            .find(&Url::parse("file:///tmp/any.txt").unwrap(), "JSON")
            .expect("matched by language");
        assert_eq!(found.name(), "json");
    }

    #[test]
    fn find_matches_url_globs_against_real_paths() {
        let root = crate::testing::temp_workspace("matcher", "url-glob");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        let matchers =
            DocumentMatchers::new([DocumentMatcher::new("json").with_url_globs(["**/*.json"])]);

        let found = matchers
            .find(&uri, "plaintext")
            .expect("matched by glob when the language is unknown");
        assert_eq!(found.name(), "json");

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[test]
    fn language_strings_win_over_url_globs() {
        let root = crate::testing::temp_workspace("matcher", "precedence");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        let matchers = DocumentMatchers::new([
            DocumentMatcher::new("by-lang").with_lang_strings(["json"]),
            DocumentMatcher::new("by-glob").with_url_globs(["**/*.json"]),
        ]);

        let found = matchers.find(&uri, "json").expect("matched");
        assert_eq!(found.name(), "by-lang");

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[test]
    fn invalid_globs_are_skipped_not_matched() {
        let root = crate::testing::temp_workspace("matcher", "invalid-glob");
        let uri = Url::from_file_path(root.join("data.json")).unwrap();
        // "[" is not a valid glob: the matcher contributes nothing, and the
        // document simply stays unmatched — the return half of the warn path.
        let matchers =
            DocumentMatchers::new([DocumentMatcher::new("broken").with_url_globs(["["])]);

        assert!(matchers.find(&uri, "plaintext").is_none());

        fs::remove_dir_all(root).expect("temp dir can be removed");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn lang_grammar_rides_along_with_the_matcher() {
        let matcher =
            DocumentMatcher::new("json").with_lang_grammar(tree_sitter_json::LANGUAGE.into());

        assert!(matcher.lang_grammar().is_some());
        assert_eq!(
            DocumentMatcher::new("bare").lang_grammar(),
            None,
            "a bare matcher carries no grammar"
        );
    }
}
