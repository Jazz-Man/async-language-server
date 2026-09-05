use async_lsp::{
    ResponseError, Result,
    lsp_types::{
        InitializeParams, InitializeResult, SaveOptions, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
        WorkspaceFolder,
    },
};
use futures::future::BoxFuture;
use tracing::info;

use super::{LanguageServerWithState, POSITION_ENCODING_PREFERRED_ORDER};

use crate::{server::Server, text_utils::Encoding};

fn workspace_folders(params: &InitializeParams) -> Vec<WorkspaceFolder> {
    params.workspace_folders.clone().unwrap_or_default()
}

impl<T: Server + Send + Sync + 'static> LanguageServerWithState<T> {
    pub(super) fn initialize(
        &mut self,
        params: InitializeParams,
    ) -> BoxFuture<'static, Result<InitializeResult, ResponseError>> {
        let workspace_folders = workspace_folders(&params);
        let client_capabilities = params.capabilities.clone();
        let initialization_options = params.initialization_options.clone();

        // 1. Extract available client position encodings, if any
        let client_position_encodings = params
            .capabilities
            .general
            .as_ref()
            .and_then(|g| g.position_encodings.clone())
            .filter(|e| !e.is_empty());

        // 2. Get server info & capabilities from the server implementor
        let mut result = InitializeResult {
            server_info: T::server_info(),
            capabilities: T::server_capabilities(params.capabilities).unwrap_or_default(),
        };
        crate::workspace::configure_capabilities(&self.state, &mut result, &client_capabilities);
        crate::workspace::apply_initialization_options(
            &self.state,
            initialization_options.as_ref(),
        );

        // 3. Try to figure out what position encoding best matches what
        //    both our server + the connected client prefers / supports
        let mut negotiated_position_encoding = Encoding::default();
        if let Some(client_available_encodings) = client_position_encodings {
            let client_available_encodings: Vec<Encoding> = client_available_encodings
                .into_iter()
                .filter_map(|kind| Encoding::try_from_lsp(&kind))
                .collect();
            for server_preferred_encoding in POSITION_ENCODING_PREFERRED_ORDER {
                if client_available_encodings.contains(&server_preferred_encoding) {
                    negotiated_position_encoding = server_preferred_encoding;
                    break;
                }
            }
        }

        // 4. Insert capabilities for our automatic handling of encodings & documents
        result.capabilities.position_encoding = Some(negotiated_position_encoding.into_lsp());
        result.capabilities.text_document_sync = Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                open_close: Some(true),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        ));

        // 5. Make sure that the state now also uses the negotiated encoding
        self.state
            .set_position_encoding(negotiated_position_encoding);
        self.state.set_workspace_folders(workspace_folders.clone());

        // 6. Emit a useful message about the negotiation
        {
            let mut lines = Vec::new();

            // 6a. Client name & version
            if let Some(info) = &params.client_info {
                if let Some(version) = &info.version {
                    lines.push(format!("{} v{}", info.name, version));
                } else {
                    lines.push(info.name.clone());
                }
            }

            // 6b. Workspace folders
            let num_folders = workspace_folders.len();
            lines.push(format!(
                "{} workspace folder{}",
                num_folders,
                if num_folders == 1 { "" } else { "s" }
            ));

            // 6c. Position encoding
            lines.push(format!(
                "{} position encoding",
                negotiated_position_encoding.as_str().to_ascii_uppercase(),
            ));

            info!(
                "Client negotiation was successful\n- {}",
                lines.join("\n- ")
            );
        }

        Box::pin(async move { Ok(result) })
    }
}
