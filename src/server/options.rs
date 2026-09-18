use std::num::NonZeroUsize;

use async_lsp::lsp_types::{ConfigurationItem, LSPAny};

/// Options for the language server wrapper.
#[derive(Debug, Default, Clone)]
pub struct ServerOptions {
    pub(crate) workspace_diagnostics: WorkspaceDiagnostics,
    pub(crate) diagnostics_parallelism: Option<NonZeroUsize>,
    pub(crate) ignore_filenames: Vec<String>,
    pub(crate) global_ignore_file: Option<std::path::PathBuf>,
}

impl ServerOptions {
    /// Sets how workspace diagnostics should be exposed by the server.
    ///
    /// The implementor's capabilities block is authoritative except for the
    /// `Disabled` kill-switch, which forces the advertised
    /// `workspace_diagnostics` capability off.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::server::{ServerOptions, WorkspaceDiagnostics};
    ///
    /// let options = ServerOptions::default()
    ///     .with_workspace_diagnostics(WorkspaceDiagnostics::disabled());
    /// ```
    #[must_use]
    pub fn with_workspace_diagnostics(
        mut self,
        workspace_diagnostics: impl Into<WorkspaceDiagnostics>,
    ) -> Self {
        self.workspace_diagnostics = workspace_diagnostics.into();
        self
    }

    /// Narrows how many documents the batch diagnostics pipeline works on
    /// at once. Defaults to the machine's CPU core count.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    ///
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_diagnostics_parallelism(NonZeroUsize::new(2).expect("nonzero"));
    /// ```
    #[must_use]
    pub fn with_diagnostics_parallelism(mut self, width: NonZeroUsize) -> Self {
        self.diagnostics_parallelism = Some(width);
        self
    }

    pub(crate) fn diagnostics_parallelism(&self) -> usize {
        self.diagnostics_parallelism
            .map_or_else(default_parallelism, NonZeroUsize::get)
    }

    /// Names of ignore files honored during workspace walks — gitignore
    /// syntax, matched per directory with cascading, independent of git
    /// presence (a project without `.git` still honors them, unlike
    /// `.gitignore` itself). Session-fixed, like matchers. Unconfigured
    /// (the default): no ignore files beyond the built-in git family,
    /// and the walk is byte-identical to a server that never set this.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_ignore_filenames([".mylspignore"]);
    /// ```
    #[must_use]
    pub fn with_ignore_filenames(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.ignore_filenames = names.into_iter().map(Into::into).collect();
        self
    }

    /// Sets one global ignore file (gitignore syntax) applied to every
    /// workspace walk across all roots, regardless of git presence. The
    /// location is the downstream server's choice — the framework
    /// defines no default path. Unset (the default): no global
    /// exclusions.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_language_server::server::ServerOptions;
    ///
    /// let options = ServerOptions::default()
    ///     .with_global_ignore_file("/etc/my-server/ignore");
    /// ```
    #[must_use]
    pub fn with_global_ignore_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.global_ignore_file = Some(path.into());
        self
    }
}

/// The crate-wide default width: every CPU core, at least one.
fn default_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Controls how workspace diagnostics are made available.
#[derive(Debug, Default, Clone)]
pub enum WorkspaceDiagnostics {
    /// Do not handle workspace diagnostics. This is the kill-switch: it
    /// forces the advertised `workspace_diagnostics` capability off
    /// regardless of the implementor's declaration.
    Disabled,
    /// Handle workspace diagnostics; the implementor's advertised
    /// `workspace_diagnostics` value is left verbatim.
    #[default]
    Enabled,
    /// Leave the implementor's advertised `workspace_diagnostics` value
    /// untouched and toggle handling using a setting.
    Configurable(WorkspaceDiagnosticsSetting),
}

impl WorkspaceDiagnostics {
    /// Do not handle workspace diagnostics. This is the kill-switch
    /// constructor: the advertised `workspace_diagnostics` capability is
    /// forced off regardless of the implementor's declaration.
    #[must_use]
    pub const fn disabled() -> Self {
        Self::Disabled
    }

    /// Handle workspace diagnostics; the implementor's advertised
    /// `workspace_diagnostics` value is left verbatim.
    #[must_use]
    pub const fn enabled() -> Self {
        Self::Enabled
    }

    /// Toggles workspace diagnostics using the given workspace setting.
    #[must_use]
    pub fn setting(key: impl Into<ConfigurationKey>) -> WorkspaceDiagnosticsSetting {
        WorkspaceDiagnosticsSetting {
            key: key.into(),
            default_enabled: true,
        }
    }
}

/// Runtime setting for workspace diagnostics.
#[derive(Debug, Clone)]
pub struct WorkspaceDiagnosticsSetting {
    pub(crate) key: ConfigurationKey,
    pub(crate) default_enabled: bool,
}

impl WorkspaceDiagnosticsSetting {
    /// Sets the initial value used before client configuration is available.
    #[must_use]
    pub fn with_default_enabled(mut self, yes: bool) -> Self {
        self.default_enabled = yes;
        self
    }
}

impl From<WorkspaceDiagnosticsSetting> for WorkspaceDiagnostics {
    fn from(setting: WorkspaceDiagnosticsSetting) -> Self {
        Self::Configurable(setting)
    }
}

/// Key for a workspace configuration setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationKey {
    section: String,
    path: Vec<String>,
}

impl ConfigurationKey {
    /// Creates a configuration key from an LSP configuration section.
    #[must_use]
    pub fn new(section: impl Into<String>) -> Self {
        Self {
            section: section.into(),
            path: Vec::new(),
        }
    }

    /// Looks up a nested boolean value inside the configuration section.
    #[must_use]
    pub fn with_path(mut self, path: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.path = path.into_iter().map(Into::into).collect();
        self
    }

    pub(crate) fn item(&self) -> ConfigurationItem {
        ConfigurationItem {
            scope_uri: None,
            section: Some(self.section.clone()),
        }
    }

    pub(crate) fn value(&self, settings: &LSPAny) -> Option<bool> {
        if self.path.is_empty() {
            return settings
                .as_bool()
                .or_else(|| settings.get(&self.section).and_then(LSPAny::as_bool))
                .or_else(|| value_at(settings, self.section.split('.')));
        }

        value_at(settings, &self.path).or_else(|| {
            settings
                .get(&self.section)
                .and_then(|settings| value_at(settings, &self.path))
        })
    }

    pub(crate) fn section(&self) -> &str {
        &self.section
    }
}

impl From<String> for ConfigurationKey {
    fn from(section: String) -> Self {
        Self::new(section)
    }
}

impl From<&str> for ConfigurationKey {
    fn from(section: &str) -> Self {
        Self::new(section)
    }
}

fn value_at(value: &LSPAny, path: impl IntoIterator<Item = impl AsRef<str>>) -> Option<bool> {
    let mut value = value;
    for segment in path {
        value = value.get(segment.as_ref())?;
    }
    value.as_bool()
}

#[cfg(test)]
mod tests {
    use super::{ConfigurationKey, ServerOptions};

    #[test]
    fn configuration_key_reads_dotted_settings() {
        let key = ConfigurationKey::new("test.workspaceDiagnostics.enabled");

        assert_eq!(
            key.value(&serde_json::json!({
                "test": {
                    "workspaceDiagnostics": {
                        "enabled": true,
                    },
                },
            })),
            Some(true),
        );
        assert_eq!(
            key.value(&serde_json::json!({
                "test.workspaceDiagnostics.enabled": false,
            })),
            Some(false),
        );
        assert_eq!(key.value(&serde_json::json!(true)), Some(true));
    }

    #[test]
    fn configuration_key_reads_section_path_settings() {
        let key = ConfigurationKey::new("test").with_path(["workspaceDiagnostics", "enabled"]);

        assert_eq!(
            key.value(&serde_json::json!({
                "workspaceDiagnostics": {
                    "enabled": true,
                },
            })),
            Some(true),
        );
        assert_eq!(
            key.value(&serde_json::json!({
                "test": {
                    "workspaceDiagnostics": {
                        "enabled": false,
                    },
                },
            })),
            Some(false),
        );
    }

    #[test]
    fn diagnostics_parallelism_defaults_to_cores_and_is_narrowable() {
        use std::num::NonZeroUsize;

        let cores = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
        assert_eq!(ServerOptions::default().diagnostics_parallelism(), cores);

        let narrowed = ServerOptions::default()
            .with_diagnostics_parallelism(NonZeroUsize::new(2).expect("constant is nonzero"));
        assert_eq!(narrowed.diagnostics_parallelism(), 2);
        assert_eq!(
            narrowed
                .with_diagnostics_parallelism(NonZeroUsize::new(1).expect("constant is nonzero"))
                .diagnostics_parallelism(),
            1,
        );
    }

    #[test]
    fn configuration_item_carries_the_section() {
        let key = ConfigurationKey::new("test");

        // The configuration request must name the section it reads, and
        // the registration's section must match the key.
        let item = key.item();
        assert_eq!(item.scope_uri, None);
        assert_eq!(item.section.as_deref(), Some("test"));
        assert_eq!(key.section(), "test");
    }
}
