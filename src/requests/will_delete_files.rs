#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{TextEdit, WorkspaceEdit};

    use crate::requests::{Request, WillDeleteFiles};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn will_delete_files_edits_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut changes = HashMap::new();
        changes.insert(
            emoji,
            vec![TextEdit {
                range: same_line(0, 4, 4),
                new_text: "x".into(),
            }],
        );
        let mut response = Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        });

        <WillDeleteFiles as Request>::modify_response(&state, &document, &mut response);

        let edits = response
            .expect("edit present")
            .changes
            .expect("changes present");
        // Keyed at the emoji document: UTF-8 byte 4 converts to client 2.
        assert_eq!(
            edits.values().next().expect("one file")[0].range,
            same_line(0, 2, 2)
        );
    }
}
