use crate::error::ServerError;
use async_lsp::lsp_types::{
    CallHierarchyServerCapability, CodeActionProviderCapability, ColorProviderCapability,
    DeclarationCapability, FoldingRangeProviderCapability, HoverProviderCapability,
    ImplementationProviderCapability, LinkedEditingRangeServerCapabilities, OneOf,
    SelectionRangeProviderCapability, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TypeDefinitionProviderCapability, WorkspaceFileOperationsServerCapabilities,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The `lsp_dispatch!` table's non-resolve trait methods, in table order.
///
/// The resolve family is absent on purpose: its defaults resolve the item
/// unchanged and never produce `method_not_implemented`. The two type
/// hierarchy *calls* (`supertypes`, `subtypes`) and `prepare_type_hierarchy`
/// are present but can never be advertised — lsp-types 0.95.1 carries no
/// type-hierarchy capability field — so their predicates are `false`.
pub(crate) const METHOD_NAMES: &[&str] = &[
    "hover",
    "declaration",
    "definition",
    "references",
    "link",
    "rename",
    "rename_prepare",
    "document_format",
    "document_range_format",
    "implementation",
    "type_definition",
    "document_highlight",
    "on_type_formatting",
    "folding_range",
    "linked_editing_range",
    "code_lens",
    "will_save_wait_until",
    "document_color",
    "color_presentation",
    "prepare_call_hierarchy",
    "prepare_type_hierarchy",
    "moniker",
    "will_create_files",
    "will_rename_files",
    "will_delete_files",
    "inlay_hint",
    "document_symbol",
    "execute_command",
    "semantic_tokens_full",
    "semantic_tokens_range",
    "semantic_tokens_full_delta",
    "completion",
    "code_action",
    "document_diagnostics",
    "selection_range",
    "inline_value",
    "incoming_calls",
    "outgoing_calls",
    "supertypes",
    "subtypes",
    "symbol",
    "signature_help",
];

/// Which `Server` methods the final `InitializeResult` advertised, and
/// which of them have already drawn their single default-warning.
#[derive(Debug, Clone)]
pub(crate) struct MethodInventory {
    inner: Arc<InventoryInner>,
}

#[derive(Debug)]
struct InventoryInner {
    advertised: Box<[bool]>,
    warned: Box<[AtomicBool]>,
}

impl Default for MethodInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl MethodInventory {
    /// An inventory advertising nothing; the state before `initialize`.
    pub(crate) fn new() -> Self {
        let empty = vec![false; METHOD_NAMES.len()].into_boxed_slice();
        let unwarned = METHOD_NAMES
            .iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        Self {
            inner: Arc::new(InventoryInner {
                advertised: empty,
                warned: unwarned,
            }),
        }
    }

    /// Derives the advertised set from the capabilities the wrapper is about
    /// to send: a predicate per method, mirroring the capability its `///`
    /// doc names.
    pub(crate) fn from_capabilities(caps: &ServerCapabilities) -> Self {
        let advertised = METHOD_NAMES
            .iter()
            .map(|name| advertised(name, caps))
            .collect();
        let warned = METHOD_NAMES
            .iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        Self {
            inner: Arc::new(InventoryInner { advertised, warned }),
        }
    }

    fn advertised(&self, method: &str) -> bool {
        index_of(method).is_some_and(|index| self.inner.advertised[index])
    }

    /// Warns once per method when a trait default ran for an advertised
    /// method. Returns whether this call emitted the warning.
    pub(crate) fn warn_once_default(&self, method: &'static str, error: &ServerError) -> bool {
        if !matches!(error, ServerError::MethodNotImplemented { .. }) {
            return false;
        }
        if !self.advertised(method) {
            return false;
        }
        let Some(index) = index_of(method) else {
            return false;
        };
        if self.inner.warned[index].swap(true, Ordering::Relaxed) {
            return false;
        }
        tracing::warn!(
            "LSP method '{method}' is advertised in the server capabilities but not \
             implemented; remove the capability or override the method",
        );
        true
    }
}

fn index_of(method: &str) -> Option<usize> {
    METHOD_NAMES.iter().position(|name| *name == method)
}

/// Presence semantics for `Option<OneOf<bool, Options>>` capabilities:
/// `Left(false)` is an explicit no, everything else present advertises.
fn advertised_bool<T>(provider: Option<&OneOf<bool, T>>) -> bool {
    provider.is_some_and(|one_of| matches!(one_of, OneOf::Left(true) | OneOf::Right(_)))
}

fn semantic_tokens_options(caps: &ServerCapabilities) -> Option<&SemanticTokensOptions> {
    match caps.semantic_tokens_provider.as_ref()? {
        SemanticTokensServerCapabilities::SemanticTokensOptions(options) => Some(options),
        SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(registration) => {
            Some(&registration.semantic_tokens_options)
        }
    }
}

/// The workspace file-operation methods share one presence chain: the
/// workspace block, then its optional file-operations section.
fn file_operation(
    caps: &ServerCapabilities,
    advertised: impl Fn(&WorkspaceFileOperationsServerCapabilities) -> bool,
) -> bool {
    caps.workspace
        .as_ref()
        .and_then(|workspace| workspace.file_operations.as_ref())
        .is_some_and(advertised)
}

/// The advertised-method predicates for the first span of
/// [`METHOD_NAMES`](METHOD_NAMES) (the text-document navigation, sync,
/// formatting, and color methods); names past the span continue in
/// [`advertised_continued`].
fn advertised(name: &str, caps: &ServerCapabilities) -> bool {
    match name {
        "hover" => caps
            .hover_provider
            .as_ref()
            .is_some_and(|provider| !matches!(provider, HoverProviderCapability::Simple(false))),
        "declaration" => caps
            .declaration_provider
            .as_ref()
            .is_some_and(|provider| !matches!(provider, DeclarationCapability::Simple(false))),
        "definition" => advertised_bool(caps.definition_provider.as_ref()),
        "references" => advertised_bool(caps.references_provider.as_ref()),
        "link" => caps.document_link_provider.is_some(),
        "rename" => advertised_bool(caps.rename_provider.as_ref()),
        "rename_prepare" => caps.rename_provider.as_ref().is_some_and(|provider| {
            matches!(
                provider,
                OneOf::Right(options) if options.prepare_provider == Some(true),
            )
        }),
        "document_format" => advertised_bool(caps.document_formatting_provider.as_ref()),
        "document_range_format" => {
            advertised_bool(caps.document_range_formatting_provider.as_ref())
        }
        "implementation" => caps
            .implementation_provider
            .as_ref()
            .is_some_and(|provider| {
                !matches!(provider, ImplementationProviderCapability::Simple(false))
            }),
        "type_definition" => caps
            .type_definition_provider
            .as_ref()
            .is_some_and(|provider| {
                !matches!(provider, TypeDefinitionProviderCapability::Simple(false))
            }),
        "document_highlight" => advertised_bool(caps.document_highlight_provider.as_ref()),
        "on_type_formatting" => caps.document_on_type_formatting_provider.is_some(),
        "folding_range" => caps
            .folding_range_provider
            .as_ref()
            .is_some_and(|provider| {
                !matches!(provider, FoldingRangeProviderCapability::Simple(false))
            }),
        "linked_editing_range" => {
            caps.linked_editing_range_provider
                .as_ref()
                .is_some_and(|provider| {
                    !matches!(
                        provider,
                        LinkedEditingRangeServerCapabilities::Simple(false),
                    )
                })
        }
        "code_lens" => caps.code_lens_provider.is_some(),
        "will_save_wait_until" => matches!(
            caps.text_document_sync.as_ref(),
            Some(TextDocumentSyncCapability::Options(options))
                if options.will_save_wait_until == Some(true),
        ),
        // `color_presentation` is the resolve leg of the color provider: the
        // same capability advertises both.
        "document_color" | "color_presentation" => {
            caps.color_provider.as_ref().is_some_and(|provider| {
                matches!(
                    provider,
                    ColorProviderCapability::Simple(true)
                        | ColorProviderCapability::ColorProvider(_)
                        | ColorProviderCapability::Options(_),
                )
            })
        }
        other => advertised_continued(other, caps),
    }
}

/// The advertised-method predicates for the remaining
/// [`METHOD_NAMES`](METHOD_NAMES) span (workspace operations, tokens, and
/// the hierarchy calls), ending in the alignment escape: the table and
/// these matches must stay in step.
fn advertised_continued(name: &str, caps: &ServerCapabilities) -> bool {
    match name {
        "prepare_call_hierarchy" | "incoming_calls" | "outgoing_calls" => caps
            .call_hierarchy_provider
            .as_ref()
            .is_some_and(|provider| {
                !matches!(provider, CallHierarchyServerCapability::Simple(false))
            }),
        // lsp-types 0.95.1 carries no type-hierarchy capability field: these
        // can never be advertised, so they never warn.
        "prepare_type_hierarchy" | "supertypes" | "subtypes" => false,
        "moniker" => advertised_bool(caps.moniker_provider.as_ref()),
        "will_create_files" => file_operation(caps, |operations| operations.will_create.is_some()),
        "will_rename_files" => file_operation(caps, |operations| operations.will_rename.is_some()),
        "will_delete_files" => file_operation(caps, |operations| operations.will_delete.is_some()),
        "inlay_hint" => advertised_bool(caps.inlay_hint_provider.as_ref()),
        "document_symbol" => advertised_bool(caps.document_symbol_provider.as_ref()),
        "execute_command" => caps.execute_command_provider.is_some(),
        "semantic_tokens_full" => semantic_tokens_options(caps).is_some_and(|options| {
            matches!(
                options.full.as_ref(),
                Some(
                    SemanticTokensFullOptions::Bool(true) | SemanticTokensFullOptions::Delta { .. }
                ),
            )
        }),
        "semantic_tokens_range" => {
            semantic_tokens_options(caps).is_some_and(|options| options.range == Some(true))
        }
        "semantic_tokens_full_delta" => semantic_tokens_options(caps).is_some_and(|options| {
            matches!(
                options.full.as_ref(),
                Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
            )
        }),
        "completion" => caps.completion_provider.is_some(),
        "code_action" => caps.code_action_provider.as_ref().is_some_and(|provider| {
            !matches!(provider, CodeActionProviderCapability::Simple(false))
        }),
        "document_diagnostics" => caps.diagnostic_provider.is_some(),
        "selection_range" => caps
            .selection_range_provider
            .as_ref()
            .is_some_and(|provider| {
                !matches!(provider, SelectionRangeProviderCapability::Simple(false))
            }),
        "inline_value" => advertised_bool(caps.inline_value_provider.as_ref()),
        "symbol" => advertised_bool(caps.workspace_symbol_provider.as_ref()),
        "signature_help" => caps.signature_help_provider.is_some(),
        other => unreachable!(
            "'{other}' is not a dispatch-table method; METHOD_NAMES and this match must stay aligned",
        ),
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        CallHierarchyServerCapability, CodeActionProviderCapability, CodeLensOptions,
        ColorProviderCapability, CompletionOptions, DeclarationCapability, DiagnosticOptions,
        DiagnosticServerCapabilities, DocumentLinkOptions, DocumentOnTypeFormattingOptions,
        ExecuteCommandOptions, FileOperationRegistrationOptions, FoldingRangeProviderCapability,
        HoverProviderCapability, ImplementationProviderCapability,
        LinkedEditingRangeServerCapabilities, OneOf, RenameOptions,
        SelectionRangeProviderCapability, SemanticTokenType, SemanticTokensFullOptions,
        SemanticTokensLegend, SemanticTokensOptions, SemanticTokensRegistrationOptions,
        SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelpOptions,
        StaticRegistrationOptions, TextDocumentRegistrationOptions, TextDocumentSyncCapability,
        TextDocumentSyncOptions, TypeDefinitionProviderCapability, WorkDoneProgressOptions,
        WorkspaceFileOperationsServerCapabilities, WorkspaceServerCapabilities,
    };

    use super::{METHOD_NAMES, MethodInventory};
    use crate::error::ServerError;

    fn error(method: &'static str) -> ServerError {
        ServerError::MethodNotImplemented { method }
    }

    fn server_error() -> ServerError {
        ServerError::rpc(async_lsp::ErrorCode::METHOD_NOT_FOUND, "deliberate".into())
    }

    #[test]
    fn method_names_pin_the_dispatch_table_surface() {
        assert_eq!(METHOD_NAMES.len(), 42);
        assert_eq!(METHOD_NAMES[0], "hover");
        assert_eq!(METHOD_NAMES.last().copied(), Some("signature_help"));
        // The resolve family never warns and never appears.
        assert!(!METHOD_NAMES.contains(&"completion_resolve"));
        // A default error for a method outside the table is a no-op.
        let inventory = MethodInventory::new();
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }

    #[test]
    fn unadvertised_methods_never_warn() {
        let inventory = MethodInventory::from_capabilities(&ServerCapabilities::default());
        // Default capabilities advertise nothing, in either predicate span.
        for name in METHOD_NAMES {
            assert!(!inventory.advertised(name), "'{name}' must stay silent");
        }
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }

    #[test]
    fn advertised_defaults_warn_exactly_once() {
        let caps = ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DiagnosticOptions::default(),
            )),
            ..ServerCapabilities::default()
        };
        let inventory = MethodInventory::from_capabilities(&caps);

        // `rename_prepare` is advertised: rename_provider is OneOf::Right
        // with prepare_provider on — exactly the prepare predicate's shape.
        assert!(inventory.advertised("rename_prepare"));
        // `document_diagnostics` is advertised by provider presence; its
        // `workspace_diagnostics` flag does not affect the method-level
        // advertisement.
        assert!(inventory.advertised("document_diagnostics"));

        assert!(inventory.warn_once_default("hover", &error("hover")));
        assert!(!inventory.warn_once_default("hover", &error("hover")));
        // A deliberate METHOD_NOT_FOUND from an overridden method is not a
        // default hit and draws no warning.
        assert!(!inventory.warn_once_default("rename", &server_error()));
        // Inventory clones share the warned state: no second warning.
        let clone = inventory.clone();
        assert!(!clone.warn_once_default("hover", &error("hover")));
    }

    #[test]
    fn explicit_bool_false_never_advertises() {
        // Every bool-representable provider set to its explicit no: an
        // explicit `false` is a deliberate absence, in any family.
        let caps = ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(false)),
            declaration_provider: Some(DeclarationCapability::Simple(false)),
            definition_provider: Some(OneOf::Left(false)),
            references_provider: Some(OneOf::Left(false)),
            rename_provider: Some(OneOf::Left(false)),
            document_formatting_provider: Some(OneOf::Left(false)),
            document_range_formatting_provider: Some(OneOf::Left(false)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(false)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(false)),
            document_highlight_provider: Some(OneOf::Left(false)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(false)),
            linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(
                false,
            )),
            call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(false)),
            moniker_provider: Some(OneOf::Left(false)),
            inlay_hint_provider: Some(OneOf::Left(false)),
            document_symbol_provider: Some(OneOf::Left(false)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(false)),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(false)),
            inline_value_provider: Some(OneOf::Left(false)),
            workspace_symbol_provider: Some(OneOf::Left(false)),
            color_provider: Some(ColorProviderCapability::Simple(false)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    will_save_wait_until: Some(false),
                    ..TextDocumentSyncOptions::default()
                },
            )),
            ..ServerCapabilities::default()
        };
        let inventory = MethodInventory::from_capabilities(&caps);

        for name in [
            "hover",
            "declaration",
            "definition",
            "references",
            "rename",
            "rename_prepare",
            "document_format",
            "document_range_format",
            "implementation",
            "type_definition",
            "document_highlight",
            "folding_range",
            "linked_editing_range",
            "prepare_call_hierarchy",
            "incoming_calls",
            "outgoing_calls",
            "moniker",
            "inlay_hint",
            "document_symbol",
            "code_action",
            "selection_range",
            "inline_value",
            "symbol",
            "document_color",
            "color_presentation",
            "will_save_wait_until",
        ] {
            assert!(!inventory.advertised(name), "'{name}' must stay silent");
        }
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }

    fn tokens_options(
        full: Option<SemanticTokensFullOptions>,
        range: Option<bool>,
    ) -> SemanticTokensOptions {
        SemanticTokensOptions {
            work_done_progress_options: WorkDoneProgressOptions::default(),
            legend: SemanticTokensLegend {
                token_types: vec![SemanticTokenType::FUNCTION],
                token_modifiers: vec![],
            },
            range,
            full,
        }
    }

    fn tokens_registration(
        full: Option<SemanticTokensFullOptions>,
        range: Option<bool>,
    ) -> SemanticTokensRegistrationOptions {
        SemanticTokensRegistrationOptions {
            text_document_registration_options: TextDocumentRegistrationOptions::default(),
            semantic_tokens_options: tokens_options(full, range),
            static_registration_options: StaticRegistrationOptions::default(),
        }
    }

    fn tokens_inventory(provider: SemanticTokensServerCapabilities) -> MethodInventory {
        let caps = ServerCapabilities {
            semantic_tokens_provider: Some(provider),
            ..ServerCapabilities::default()
        };
        MethodInventory::from_capabilities(&caps)
    }

    fn file_operations(
        will_create: Option<FileOperationRegistrationOptions>,
        will_rename: Option<FileOperationRegistrationOptions>,
        will_delete: Option<FileOperationRegistrationOptions>,
    ) -> ServerCapabilities {
        ServerCapabilities {
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: None,
                file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                    did_create: None,
                    will_create,
                    did_rename: None,
                    will_rename,
                    did_delete: None,
                    will_delete,
                }),
            }),
            ..ServerCapabilities::default()
        }
    }

    /// The all-advertised text-document span: navigation, sync, formatting,
    /// and color providers at their advertised shapes.
    fn all_advertised_text_document_caps() -> ServerCapabilities {
        ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: None,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            document_highlight_provider: Some(OneOf::Left(true)),
            document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                first_trigger_character: "{".into(),
                more_trigger_character: None,
            }),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(true)),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: None,
            }),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    will_save_wait_until: Some(true),
                    ..TextDocumentSyncOptions::default()
                },
            )),
            color_provider: Some(ColorProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        }
    }

    /// The all-advertised second span: workspace operations, hierarchy,
    /// tokens, and the remaining providers, over the text-document base.
    fn all_advertised_workspace_caps() -> ServerCapabilities {
        ServerCapabilities {
            call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
            moniker_provider: Some(OneOf::Left(true)),
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: None,
                file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                    did_create: None,
                    will_create: Some(FileOperationRegistrationOptions::default()),
                    did_rename: None,
                    will_rename: Some(FileOperationRegistrationOptions::default()),
                    did_delete: None,
                    will_delete: Some(FileOperationRegistrationOptions::default()),
                }),
            }),
            inlay_hint_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            execute_command_provider: Some(ExecuteCommandOptions::default()),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(tokens_options(
                    // The richest shape: it serves full, range, and delta.
                    Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                    Some(true),
                )),
            ),
            completion_provider: Some(CompletionOptions::default()),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DiagnosticOptions::default(),
            )),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            inline_value_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            signature_help_provider: Some(SignatureHelpOptions::default()),
            ..all_advertised_text_document_caps()
        }
    }

    #[test]
    fn all_advertised_shapes_advertise_their_methods() {
        // Every representable provider set to its advertised shape: each of
        // the table's methods answers true from its own capability.
        let inventory = MethodInventory::from_capabilities(&all_advertised_workspace_caps());

        for name in METHOD_NAMES {
            if matches!(*name, "prepare_type_hierarchy" | "supertypes" | "subtypes") {
                continue;
            }
            assert!(
                inventory.advertised(name),
                "'{name}' must be advertised by its configured shape",
            );
        }
        // lsp-types 0.95.1 carries no type-hierarchy capability: the trio
        // stays silent whatever else is configured.
        for name in ["prepare_type_hierarchy", "supertypes", "subtypes"] {
            assert!(
                !inventory.advertised(name),
                "'{name}' is never advertisable",
            );
        }
    }

    fn options_inventory(
        full: Option<SemanticTokensFullOptions>,
        range: Option<bool>,
    ) -> MethodInventory {
        tokens_inventory(SemanticTokensServerCapabilities::SemanticTokensOptions(
            tokens_options(full, range),
        ))
    }

    /// Asserts the three tokens methods' advertisement in shape order:
    /// full, range, full-delta.
    fn assert_tokens(inventory: &MethodInventory, full: bool, range: bool, full_delta: bool) {
        assert_eq!(inventory.advertised("semantic_tokens_full"), full);
        assert_eq!(inventory.advertised("semantic_tokens_range"), range);
        assert_eq!(
            inventory.advertised("semantic_tokens_full_delta"),
            full_delta,
        );
    }

    #[test]
    fn semantic_tokens_advertise_full_range_and_delta_by_shape() {
        // The Options variant: Bool(true) fills the full leg only.
        assert_tokens(
            &options_inventory(Some(SemanticTokensFullOptions::Bool(true)), Some(true)),
            true,
            true,
            false,
        );

        // Delta { delta: Some(true) } serves both the full and the delta
        // leg.
        assert_tokens(
            &options_inventory(
                Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                Some(true),
            ),
            true,
            true,
            true,
        );

        // A delta flag off still advertises full, and an absent range flag
        // stays silent.
        assert_tokens(
            &options_inventory(
                Some(SemanticTokensFullOptions::Delta { delta: Some(false) }),
                None,
            ),
            true,
            false,
            false,
        );

        // Explicit negatives: Bool(false) and range: Some(false) are
        // deliberate nos.
        assert_tokens(
            &options_inventory(Some(SemanticTokensFullOptions::Bool(false)), Some(false)),
            false,
            false,
            false,
        );

        // The RegistrationOptions variant flows through the same options
        // block, so the same shapes answer there.
        assert_tokens(
            &tokens_inventory(
                SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                    tokens_registration(Some(SemanticTokensFullOptions::Bool(true)), Some(true)),
                ),
            ),
            true,
            true,
            false,
        );
    }

    #[test]
    fn file_operations_advertise_only_their_own_operation() {
        // The workspace block alone, without its file-operations section,
        // advertises nothing.
        let bare = MethodInventory::from_capabilities(&file_operations(None, None, None));
        for name in [
            "will_create_files",
            "will_rename_files",
            "will_delete_files",
        ] {
            assert!(!bare.advertised(name), "'{name}' must stay silent");
        }

        // Each method answers from its own field: registering will_create
        // alone must not pull in rename or delete.
        let only_create = MethodInventory::from_capabilities(&file_operations(
            Some(FileOperationRegistrationOptions::default()),
            None,
            None,
        ));
        assert!(only_create.advertised("will_create_files"));
        assert!(!only_create.advertised("will_rename_files"));
        assert!(!only_create.advertised("will_delete_files"));
    }

    #[test]
    fn a_predicate_reads_its_own_capability_not_a_neighbors() {
        // Slot 0 (hover) is an explicit no while document_format sits far
        // later in the table: an index bug pinning every lookup to slot 0
        // would turn the positive rows into hover's answer.
        let caps = ServerCapabilities {
            hover_provider: Some(HoverProviderCapability::Simple(false)),
            document_formatting_provider: Some(OneOf::Left(true)),
            ..ServerCapabilities::default()
        };
        let inventory = MethodInventory::from_capabilities(&caps);

        assert!(!inventory.advertised("hover"));
        assert!(inventory.advertised("document_format"));
        // The warning answers through the same index: the advertised method
        // draws its warning exactly once, the unadvertised neighbor never.
        assert!(inventory.warn_once_default("document_format", &error("document_format"),));
        assert!(!inventory.warn_once_default("document_format", &error("document_format"),));
        assert!(!inventory.warn_once_default("hover", &error("hover")));
    }
}
