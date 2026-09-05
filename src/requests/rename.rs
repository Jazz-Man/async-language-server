#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::RenameParams,
    response = Option<async_lsp::lsp_types::WorkspaceEdit>,
    document(text_document_position.text_document),
    incoming_position(text_document_position.position),
    outgoing(crate::requests::conversion::modify_outgoing_workspace_edit),
)]
pub(crate) struct RenameRequest;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{TextEdit, WorkspaceEdit};

    use crate::requests::{RenameRequest, Request};
    use crate::testing::{same_line, state_with_documents, url};

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

        <RenameRequest as Request>::modify_response(&state, &document, &mut response);

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

        <RenameRequest as Request>::modify_response(&state, &document, &mut response);

        let edit = response.unwrap();
        let edit = edit.changes.unwrap().into_values().next().unwrap();
        assert_eq!(edit[0].range, same_line(0, 2, 2));
    }
}
