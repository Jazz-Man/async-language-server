use async_lsp::lsp_types::{
    RenameParams as LspRenameParams, Url, WorkspaceEdit as LspWorkspaceEdit,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_position, convert_workspace_edit},
};

pub struct Rename;

impl Request for Rename {
    type Params = LspRenameParams;
    type Response = Option<LspWorkspaceEdit>;

    fn extract_url(params: &Self::Params) -> Option<Url> {
        Some(params.text_document_position.text_document.uri.clone())
    }

    fn modify_params(state: &ServerState, document: &Document, params: &mut Self::Params) {
        convert_position(
            state,
            document,
            &mut params.text_document_position.position,
            Direction::Incoming,
        );
    }

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(response) = response.as_mut() {
            convert_workspace_edit(state, document, response, Direction::Outgoing);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{TextEdit, WorkspaceEdit};

    use crate::testing::{same_line, state_with_documents, url};

    use super::{Rename, Request};

    #[test]
    fn workspace_edits_are_converted_using_their_own_document() {
        let (state, source, target) = state_with_documents();
        let document = state.document(&source).unwrap();
        let mut response = Some(WorkspaceEdit {
            changes: Some(HashMap::from([(
                target,
                vec![TextEdit::new(same_line(0, 4, 4), "x".into())],
            )])),
            ..Default::default()
        });

        <Rename as Request>::modify_response(&state, &document, &mut response);

        let edit = response.unwrap();
        let edit = edit.changes.unwrap().into_values().next().unwrap();
        assert_eq!(edit[0].range, same_line(0, 2, 2));
    }

    #[test]
    fn rename_edits_fall_back_to_request_document_when_target_is_unknown() {
        let (state, _, target) = state_with_documents();
        let document = state.document(&target).unwrap();
        let missing = url("missing.txt");
        let mut response = Some(WorkspaceEdit {
            changes: Some(HashMap::from([(
                missing,
                vec![TextEdit::new(same_line(0, 4, 4), "x".into())],
            )])),
            ..Default::default()
        });

        <Rename as Request>::modify_response(&state, &document, &mut response);

        let edit = response.unwrap();
        let edit = edit.changes.unwrap().into_values().next().unwrap();
        assert_eq!(edit[0].range, same_line(0, 2, 2));
    }
}
