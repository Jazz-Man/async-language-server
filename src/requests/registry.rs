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
        }
    };
}
pub(crate) use resolve_methods;
