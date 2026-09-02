//! The method registry: the single source of truth for (trait method,
//! async-lsp method, Request type, params/response types, doc, hook shape).
//!
//! Three tables, each expanded through a consumer's stamper macro via the
//! `table!(stamper)` passthrough — the same pattern async-lsp's `define!`
//! uses one level down. `generated_methods!` rows fully determine a
//! `Request` impl (extract-url path, incoming hook, outgoing helper);
//! `custom_methods!` rows only bind names/types — the hooks live in the
//! per-method file under `src/requests/`; `resolve_methods!` rows bind the
//! resolve trio. Consumers: `server_trait.rs` (trait methods),
//! `with_state/mod.rs` (dispatch), `requests/mod.rs` (generated impls).
//!
//! Rows carry TWO method idents: the `Server` trait method and the
//! async-lsp trait method (they differ for `rename_prepare`/`prepare_rename`,
//! `document_format`/`formatting`, `document_range_format`/`range_formatting`,
//! `document_diagnostics`/`document_diagnostic`, `link`/`document_link`).
//! Types are full paths because rows expand in three different scopes.

macro_rules! generated_methods {
    ($m:ident) => {
        $m! {
            hover: hover @ Hover {
                doc: "Handles `textDocument/hover` requests from the client.\n\nReturns hover contents for the position in `params`, or `None` when there is nothing to show. Positions and ranges are UTF-8. Requires a hover provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::HoverParams,
                response: Option<async_lsp::lsp_types::Hover>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_hover,
            }
            declaration: declaration @ Declaration {
                doc: "Handles `textDocument/declaration` requests from the client.\n\nReturns the declaration locations of the symbol at the position in `params`, or `None`. Requires a declaration provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::request::GotoDeclarationParams,
                response: Option<async_lsp::lsp_types::request::GotoDeclarationResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            definition: definition @ Definition {
                doc: "Handles `textDocument/definition` requests from the client.\n\nReturns the definition locations of the symbol at the position in `params`, or `None`. Requires a definition provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::GotoDefinitionParams,
                response: Option<async_lsp::lsp_types::GotoDefinitionResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            references: references @ References {
                doc: "Handles `textDocument/references` requests from the client.\n\nReturns the locations that reference the symbol at the position in `params`, or `None`. Requires a references provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::ReferenceParams,
                response: Option<Vec<async_lsp::lsp_types::Location>>,
                document: text_document_position.text_document,
                incoming: position at text_document_position.position,
                outgoing: modify_outgoing_locations,
            }
            link: document_link @ DocumentLink {
                doc: "Handles `textDocument/documentLink` requests from the client.\n\nReturns links inside the document in `params`, or `None`. Requires a document link provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentLinkParams,
                response: Option<Vec<async_lsp::lsp_types::DocumentLink>>,
                document: text_document,
                outgoing: modify_outgoing_document_links,
            }
            rename: rename @ Rename {
                doc: "Handles `textDocument/rename` requests from the client.\n\nReturns a workspace edit renaming the symbol at the position in `params` to `params.new_name`, or `None` when renaming is not possible. Requires a rename provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::RenameParams,
                response: Option<async_lsp::lsp_types::WorkspaceEdit>,
                document: text_document_position.text_document,
                incoming: position at text_document_position.position,
                outgoing: modify_outgoing_workspace_edit,
            }
            rename_prepare: prepare_rename @ RenamePrepare {
                doc: "Handles `textDocument/prepareRename` requests from the client.\n\nReturns the range of the symbol at the position in `params` that a rename would apply to, or `None` when renaming is not possible. Requires a rename provider with `prepare_provider` enabled.",
                params: async_lsp::lsp_types::TextDocumentPositionParams,
                response: Option<async_lsp::lsp_types::PrepareRenameResponse>,
                document: text_document,
                incoming: position at position,
                outgoing: modify_outgoing_prepare_rename_response,
            }
            document_format: formatting @ DocumentFormat {
                doc: "Handles `textDocument/formatting` requests from the client.\n\nReturns edits formatting the whole document in `params`, or `None`. Requires a document formatting provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentFormattingParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document,
                outgoing: modify_outgoing_text_edits,
            }
            document_range_format: range_formatting @ DocumentRangeFormat {
                doc: "Handles `textDocument/rangeFormatting` requests from the client.\n\nReturns edits formatting the range in `params`, or `None`. Requires a document range formatting provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentRangeFormattingParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document,
                incoming: range at range,
                outgoing: modify_outgoing_text_edits,
            }
            implementation: implementation @ Implementation {
                doc: "Handles `textDocument/implementation` requests from the client.\n\nReturns the implementation locations of the symbol at the position in `params`, or `None`. Requires an implementation provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::request::GotoImplementationParams,
                response: Option<async_lsp::lsp_types::request::GotoImplementationResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            type_definition: type_definition @ TypeDefinition {
                doc: "Handles `textDocument/typeDefinition` requests from the client.\n\nReturns the type definition locations of the symbol at the position in `params`, or `None`. Requires a type definition provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::request::GotoTypeDefinitionParams,
                response: Option<async_lsp::lsp_types::request::GotoTypeDefinitionResponse>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_goto_response,
            }
            document_highlight: document_highlight @ DocumentHighlight {
                doc: "Handles `textDocument/documentHighlight` requests from the client.\n\nReturns the highlights of the symbol at the position in `params`, or `None`. Requires a document highlight provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentHighlightParams,
                response: Option<Vec<async_lsp::lsp_types::DocumentHighlight>>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_document_highlights,
            }
            on_type_formatting: on_type_formatting @ OnTypeFormatting {
                doc: "Handles `textDocument/onTypeFormatting` requests from the client.\n\nReturns edits formatting around the typed character at the position in `params`, or `None`. Requires a document on-type formatting provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentOnTypeFormattingParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document_position.text_document,
                incoming: position at text_document_position.position,
                outgoing: modify_outgoing_text_edits,
            }
            folding_range: folding_range @ FoldingRange {
                doc: "Handles `textDocument/foldingRange` requests from the client.\n\nReturns the folding ranges of the document in `params`, or `None`. Requires a folding range provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::FoldingRangeParams,
                response: Option<Vec<async_lsp::lsp_types::FoldingRange>>,
                document: text_document,
                outgoing: modify_outgoing_folding_ranges,
            }
            linked_editing_range: linked_editing_range @ LinkedEditingRange {
                doc: "Handles `textDocument/linkedEditingRange` requests from the client.\n\nReturns the ranges that rename together with the symbol at the position in `params`, or `None`. Requires a linked editing range provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::LinkedEditingRangeParams,
                response: Option<async_lsp::lsp_types::LinkedEditingRanges>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_linked_editing_ranges,
            }
            code_lens: code_lens @ CodeLens {
                doc: "Handles `textDocument/codeLens` requests from the client.\n\nReturns the code lenses of the document in `params`, or `None`. Requires a code lens provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CodeLensParams,
                response: Option<Vec<async_lsp::lsp_types::CodeLens>>,
                document: text_document,
                outgoing: modify_outgoing_code_lenses,
            }
            will_save_wait_until: will_save_wait_until @ WillSaveWaitUntil {
                doc: "Handles `textDocument/willSaveWaitUntil` requests from the client.\n\nReturns edits applied to the document before it is saved, or `None`. Requires `will_save_wait_until` enabled in the text-document sync options of [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::WillSaveTextDocumentParams,
                response: Option<Vec<async_lsp::lsp_types::TextEdit>>,
                document: text_document,
                outgoing: modify_outgoing_text_edits,
            }
            document_color: document_color @ DocumentColor {
                doc: "Handles `textDocument/documentColor` requests from the client.\n\nReturns all color references found in the document in `params`. Requires a color provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentColorParams,
                response: Vec<async_lsp::lsp_types::ColorInformation>,
                document: text_document,
                outgoing: modify_outgoing_color_informations,
            }
            color_presentation: color_presentation @ ColorPresentation {
                doc: "Handles `textDocument/colorPresentation` requests from the client.\n\nReturns the presentations for the color at the range in `params`. Sent as the resolve leg of a document color provider.",
                params: async_lsp::lsp_types::ColorPresentationParams,
                response: Vec<async_lsp::lsp_types::ColorPresentation>,
                document: text_document,
                incoming: range at range,
                outgoing: modify_outgoing_color_presentations,
            }
            prepare_call_hierarchy: prepare_call_hierarchy @ CallHierarchyPrepare {
                doc: "Handles `textDocument/prepareCallHierarchy` requests from the client.\n\nReturns the call hierarchy items for the symbol at the position in `params`, or `None`. Requires a call hierarchy provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CallHierarchyPrepareParams,
                response: Option<Vec<async_lsp::lsp_types::CallHierarchyItem>>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_call_hierarchy_items,
            }
            prepare_type_hierarchy: prepare_type_hierarchy @ TypeHierarchyPrepare {
                doc: "Handles `textDocument/prepareTypeHierarchy` requests from the client.\n\nReturns the type hierarchy items for the symbol at the position in `params`, or `None`. Requires a type hierarchy provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::TypeHierarchyPrepareParams,
                response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
                outgoing: modify_outgoing_type_hierarchy_items,
            }
            moniker: moniker @ Moniker {
                doc: "Handles `textDocument/moniker` requests from the client.\n\nReturns the symbol monikers at the position in `params`, or `None`. Requires a moniker provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::MonikerParams,
                response: Option<Vec<async_lsp::lsp_types::Moniker>>,
                document: text_document_position_params.text_document,
                incoming: position at text_document_position_params.position,
            }
            will_create_files: will_create_files @ WillCreateFiles {
                doc: "Handles `workspace/willCreateFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are created, or `None`. Requires `workspace.fileOperations.willCreate` in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CreateFilesParams,
                response: Option<async_lsp::lsp_types::WorkspaceEdit>,
                outgoing: modify_outgoing_workspace_edit,
            }
            will_rename_files: will_rename_files @ WillRenameFiles {
                doc: "Handles `workspace/willRenameFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are renamed, or `None`. Requires `workspace.fileOperations.willRename` in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::RenameFilesParams,
                response: Option<async_lsp::lsp_types::WorkspaceEdit>,
                outgoing: modify_outgoing_workspace_edit,
            }
            will_delete_files: will_delete_files @ WillDeleteFiles {
                doc: "Handles `workspace/willDeleteFiles` requests from the client.\n\nReturns a workspace edit applied before the files in `params` are deleted, or `None`. Requires `workspace.fileOperations.willDelete` in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DeleteFilesParams,
                response: Option<async_lsp::lsp_types::WorkspaceEdit>,
                outgoing: modify_outgoing_workspace_edit,
            }
            inlay_hint: inlay_hint @ InlayHint {
                doc: "Handles `textDocument/inlayHint` requests from the client.\n\nReturns inlay hints for the range in `params`, or `None`. Requires an inlay hint provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::InlayHintParams,
                response: Option<Vec<async_lsp::lsp_types::InlayHint>>,
                document: text_document,
                incoming: range at range,
                outgoing: modify_outgoing_inlay_hints,
            }
            document_symbol: document_symbol @ DocumentSymbol {
                doc: "Handles `textDocument/documentSymbol` requests from the client.\n\nReturns the symbol tree of the document in `params` (nested when the client supports it, flat otherwise), or `None`. Requires a document symbol provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentSymbolParams,
                response: Option<async_lsp::lsp_types::DocumentSymbolResponse>,
                document: text_document,
                outgoing: modify_outgoing_document_symbols,
            }
            execute_command: execute_command @ ExecuteCommand {
                doc: "Handles `workspace/executeCommand` requests from the client.\n\nExecutes the command in `params` and returns an opaque result. Requires an execute command provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::ExecuteCommandParams,
                response: Option<async_lsp::lsp_types::LSPAny>,
            }
            semantic_tokens_full: semantic_tokens_full @ SemanticTokensFull {
                doc: "Handles `textDocument/semanticTokens/full` requests from the client.\n\nReturns the document's full semantic token stream, or `None`. Token columns and lengths are UTF-8 here and converted to the negotiated encoding on the wire. Requires a semantic tokens provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::SemanticTokensParams,
                response: Option<async_lsp::lsp_types::SemanticTokensResult>,
                document: text_document,
                outgoing: modify_outgoing_semantic_tokens_result,
            }
            semantic_tokens_range: semantic_tokens_range @ SemanticTokensRange {
                doc: "Handles `textDocument/semanticTokens/range` requests from the client.\n\nReturns the semantic token stream for the range in `params`, or `None`. Token columns and lengths are UTF-8 here and converted on the wire. Requires a semantic tokens provider with `range` support in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::SemanticTokensRangeParams,
                response: Option<async_lsp::lsp_types::SemanticTokensRangeResult>,
                document: text_document,
                incoming: range at range,
                outgoing: modify_outgoing_semantic_tokens_range_result,
            }
            semantic_tokens_full_delta: semantic_tokens_full_delta @ SemanticTokensFullDelta {
                doc: "Handles `textDocument/semanticTokens/full/delta` requests from the client.\n\nReturns edits transforming the previous token stream (identified by `params.previous_result_id`) into the current one, or a full stream when a delta is not practical. Token columns and lengths are UTF-8 here; edits' inserted tokens are converted seeded against the cached previous UTF-8 stream, and flat-array indices pass through unchanged. Requires a semantic tokens provider with `full.delta` support in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::SemanticTokensDeltaParams,
                response: Option<async_lsp::lsp_types::SemanticTokensFullDeltaResult>,
                document: text_document,
                outgoing: modify_outgoing_semantic_tokens_delta_result,
            }
        }
    };
}
pub(crate) use generated_methods;

macro_rules! custom_methods {
    ($m:ident) => {
        $m! {
            completion: completion @ Completion {
                doc: "Handles `textDocument/completion` requests from the client.\n\nReturns completion items at the position in `params`, or `None`. Requires a completion provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CompletionParams,
                response: Option<async_lsp::lsp_types::CompletionResponse>,
            }
            code_action: code_action @ CodeAction {
                doc: "Handles `textDocument/codeAction` requests from the client.\n\nReturns code actions available for the range in `params`, or `None`. Requires a code action provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::CodeActionParams,
                response: Option<async_lsp::lsp_types::CodeActionResponse>,
            }
            document_diagnostics: document_diagnostic @ DocumentDiagnostics {
                doc: "Handles `textDocument/diagnostic` requests from the client.\n\nReturns the diagnostics for the document in `params`. The document's current snapshot is available through `state.document(&params.text_document.uri)`. Requires a diagnostic provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::DocumentDiagnosticParams,
                response: async_lsp::lsp_types::DocumentDiagnosticReportResult,
            }
            selection_range: selection_range @ SelectionRange {
                doc: "Handles `textDocument/selectionRange` requests from the client.\n\nReturns the selection-range chains for the positions in `params`, or `None`; `positions[i]` must be contained in `result[i].range`. Requires a selection range provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::SelectionRangeParams,
                response: Option<Vec<async_lsp::lsp_types::SelectionRange>>,
            }
            incoming_calls: incoming_calls @ IncomingCalls {
                doc: "Handles `callHierarchy/incomingCalls` requests from the client.\n\nReturns the callers of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.",
                params: async_lsp::lsp_types::CallHierarchyIncomingCallsParams,
                response: Option<Vec<async_lsp::lsp_types::CallHierarchyIncomingCall>>,
            }
            outgoing_calls: outgoing_calls @ OutgoingCalls {
                doc: "Handles `callHierarchy/outgoingCalls` requests from the client.\n\nReturns the callees of the item in `params`, or `None`. Only issued when the server registered a call hierarchy provider. The item's own ranges arrive converted to UTF-8 and return converted to the negotiated encoding against the item's document when tracked; the response's `from_ranges` convert against the request document (the caller's).",
                params: async_lsp::lsp_types::CallHierarchyOutgoingCallsParams,
                response: Option<Vec<async_lsp::lsp_types::CallHierarchyOutgoingCall>>,
            }
            supertypes: supertypes @ Supertypes {
                doc: "Handles `typeHierarchy/supertypes` requests from the client.\n\nReturns the supertypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. The item's ranges arrive converted to UTF-8 and return converted to the negotiated encoding, each against the item's own document when tracked.",
                params: async_lsp::lsp_types::TypeHierarchySupertypesParams,
                response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
            }
            subtypes: subtypes @ Subtypes {
                doc: "Handles `typeHierarchy/subtypes` requests from the client.\n\nReturns the subtypes of the item in `params`, or `None`. Only issued when the server registered a type hierarchy provider. Conversion as per `supertypes`.",
                params: async_lsp::lsp_types::TypeHierarchySubtypesParams,
                response: Option<Vec<async_lsp::lsp_types::TypeHierarchyItem>>,
            }
            inline_value: inline_value @ InlineValue {
                doc: "Handles `textDocument/inlineValue` requests from the client.\n\nReturns a single inline value computed for the range in `params`, or `None`. Requires an inline value provider in [`Server::server_capabilities`].",
                params: async_lsp::lsp_types::InlineValueParams,
                response: Option<async_lsp::lsp_types::InlineValue>,
            }
            symbol: symbol @ Symbol {
                doc: "Handles `workspace/symbol` requests from the client.\n\nReturns the workspace-wide symbols matching the query, or `None`. Requires a workspace symbol provider in [`Server::server_capabilities`]. Symbol locations convert against their own document when tracked; untracked files are read from disk once per request (cached); unreadable locations pass through unchanged.",
                params: async_lsp::lsp_types::WorkspaceSymbolParams,
                response: Option<async_lsp::lsp_types::WorkspaceSymbolResponse>,
            }
            signature_help: signature_help @ SignatureHelp {
                doc: "Handles `textDocument/signatureHelp` requests from the client.\n\nReturns signature help at the position in `params`, or `None`. Requires a signature help provider in [`Server::server_capabilities`]. The position AND the label offsets of an echoed `context.active_signature_help` are converted to UTF-8 before the handler runs; label offsets are recounted against the label string itself.",
                params: async_lsp::lsp_types::SignatureHelpParams,
                response: Option<async_lsp::lsp_types::SignatureHelp>,
            }
        }
    };
}
pub(crate) use custom_methods;

macro_rules! resolve_methods {
    ($m:ident) => {
        $m! {
            completion_resolve: completion_item_resolve @ CompletionResolve {
                doc: "Handles `completionItem/resolve` requests from the client.\n\nFills in additional detail on an item previously returned by [`Server::completion`]. The default implementation resolves the item unchanged; returning the item as-is is always valid. Requires a completion provider with `resolve_provider` enabled. Positions in the incoming item are converted to UTF-8 before the handler runs, and positions in returned edits are converted back to the negotiated encoding — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
                params: async_lsp::lsp_types::CompletionItem,
                response: async_lsp::lsp_types::CompletionItem,
            }
            code_action_resolve: code_action_resolve @ CodeActionResolve {
                doc: "Handles `codeAction/resolve` requests from the client.\n\nFills in additional detail on an action previously returned by [`Server::code_action`]. The default implementation resolves the action unchanged. Requires a code action provider with `resolve_provider` enabled. Positions in the incoming action are converted to UTF-8 before the handler runs, and positions in returned edits are converted back to the negotiated encoding — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
                params: async_lsp::lsp_types::CodeAction,
                response: async_lsp::lsp_types::CodeAction,
            }
            link_resolve: document_link_resolve @ DocumentLinkResolve {
                doc: "Handles `documentLink/resolve` requests from the client.\n\nFills in the target of a link previously returned by [`Server::link`]. The default implementation resolves the link unchanged. Requires a document link provider with `resolve_provider` enabled. The range in the incoming link is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise it passes through unchanged.",
                params: async_lsp::lsp_types::DocumentLink,
                response: async_lsp::lsp_types::DocumentLink,
            }
            code_lens_resolve: code_lens_resolve @ CodeLensResolve {
                doc: "Handles `codeLens/resolve` requests from the client.\n\nFills in the command of a lens previously returned by [`Server::code_lens`]. The default implementation resolves the lens unchanged. Requires a code lens provider with `resolve_provider` enabled. The lens's range is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise it passes through unchanged.",
                params: async_lsp::lsp_types::CodeLens,
                response: async_lsp::lsp_types::CodeLens,
            }
            inlay_hint_resolve: inlay_hint_resolve @ InlayHintResolve {
                doc: "Handles `inlayHint/resolve` requests from the client.\n\nFills in additional detail on a hint previously returned by [`Server::inlay_hint`]. The default implementation resolves the hint unchanged. Requires an inlay hint provider with `resolve_provider` enabled. The hint's position, edits, and label-part locations are converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — both against the sole tracked document, when exactly one document is tracked; otherwise they pass through unchanged.",
                params: async_lsp::lsp_types::InlayHint,
                response: async_lsp::lsp_types::InlayHint,
            }
            workspace_symbol_resolve: workspace_symbol_resolve @ WorkspaceSymbolResolve {
                doc: "Handles `workspaceSymbol/resolve` requests from the client.\n\nFills in the location range of a symbol previously returned by [`Server::symbol`] without one. The default implementation resolves the symbol unchanged. Requires a workspace symbol provider with `resolve_provider` enabled. The symbol's location is converted to UTF-8 before the handler runs and back to the negotiated encoding afterwards — against the location's own document when tracked, reading from disk otherwise; a location without a range passes through unchanged.",
                params: async_lsp::lsp_types::WorkspaceSymbol,
                response: async_lsp::lsp_types::WorkspaceSymbol,
            }
        }
    };
}
pub(crate) use resolve_methods;
