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
        $m! {}
    };
}
pub(crate) use generated_methods;

macro_rules! custom_methods {
    ($m:ident) => {
        $m! {}
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
