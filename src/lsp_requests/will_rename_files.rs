#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::RenameFilesParams,
    response = Option<async_lsp::lsp_types::WorkspaceEdit>,
    outgoing(crate::lsp_requests::conversion::modify_outgoing_workspace_edit),
)]
pub(crate) struct WillRenameFilesRequest;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_lsp::lsp_types::{RenameFilesParams, TextEdit, WorkspaceEdit};
    use lsp_macros::conversion_tests;

    use crate::lsp_requests::WillRenameFilesRequest;
    use crate::testing::{line_position, same_line};

    conversion_tests! {
        will_rename_files_edits_convert_outgoing: WillRenameFilesRequest {
            params: |_uri| RenameFilesParams::default(),
            // Keyed at the emoji document: UTF-8 byte 4 converts to client 2.
            response: |_plain, emoji| {
                let mut changes = HashMap::new();
                changes.insert(
                    emoji,
                    vec![TextEdit {
                        range: same_line(0, 4, 4),
                        new_text: "x".into(),
                    }],
                );
                Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..WorkspaceEdit::default()
                })
            },
            outgoing: |r| match r.as_ref() {
                Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..
                }) => changes.values().next().expect("one file")[0].range.start,
                _ => panic!("expected edit with changes"),
            },
            returns: line_position(0, 2),
        }
    }
}
