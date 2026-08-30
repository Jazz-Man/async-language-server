use async_lsp::lsp_types::{
    CompletionTextEdit as LspCompletionTextEdit, Diagnostic as LspDiagnostic,
    DocumentDiagnosticReportKind, Location as LspLocation, LocationLink as LspLocationLink, OneOf,
    Position as LspPosition, Range as LspRange, TextEdit as LspTextEdit, Url,
    WorkspaceEdit as LspWorkspaceEdit,
};

use crate::{
    server::{Document, ServerState},
    text_utils::{Encoding, position_to_encoding},
};

pub(crate) fn modify_incoming_position(
    state: &ServerState,
    document: &Document,
    position: &mut LspPosition,
) {
    *position = position_to_encoding(
        &document.text,
        *position,
        state.get_position_encoding(),
        Encoding::UTF8,
    );
}

pub(crate) fn modify_incoming_position_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    position: &mut LspPosition,
) {
    if url == fallback.url() {
        modify_incoming_position(state, fallback, position);
    } else if let Some(document) = state.document(url) {
        modify_incoming_position(state, &document, position);
    } else {
        modify_incoming_position(state, fallback, position);
    }
}

pub(crate) fn modify_incoming_range(
    state: &ServerState,
    document: &Document,
    range: &mut LspRange,
) {
    modify_incoming_position(state, document, &mut range.start);
    modify_incoming_position(state, document, &mut range.end);
}

pub(crate) fn modify_incoming_range_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    range: &mut LspRange,
) {
    modify_incoming_position_at_url(state, fallback, url, &mut range.start);
    modify_incoming_position_at_url(state, fallback, url, &mut range.end);
}

pub(crate) fn modify_incoming_location(
    state: &ServerState,
    document: &Document,
    loc: &mut LspLocation,
) {
    let uri = loc.uri.clone();
    modify_incoming_range_at_url(state, document, &uri, &mut loc.range);
}

pub(crate) fn modify_outgoing_position(
    state: &ServerState,
    document: &Document,
    position: &mut LspPosition,
) {
    *position = position_to_encoding(
        &document.text,
        *position,
        Encoding::UTF8,
        state.get_position_encoding(),
    );
}

pub(crate) fn modify_outgoing_position_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    position: &mut LspPosition,
) {
    if url == fallback.url() {
        modify_outgoing_position(state, fallback, position);
    } else if let Some(document) = state.document(url) {
        modify_outgoing_position(state, &document, position);
    } else {
        modify_outgoing_position(state, fallback, position);
    }
}

pub(crate) fn modify_outgoing_range(
    state: &ServerState,
    document: &Document,
    range: &mut LspRange,
) {
    modify_outgoing_position(state, document, &mut range.start);
    modify_outgoing_position(state, document, &mut range.end);
}

pub(crate) fn modify_outgoing_range_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    range: &mut LspRange,
) {
    modify_outgoing_position_at_url(state, fallback, url, &mut range.start);
    modify_outgoing_position_at_url(state, fallback, url, &mut range.end);
}

pub(crate) fn modify_outgoing_location(
    state: &ServerState,
    document: &Document,
    loc: &mut LspLocation,
) {
    let uri = loc.uri.clone();
    modify_outgoing_range_at_url(state, document, &uri, &mut loc.range);
}

pub(crate) fn modify_outgoing_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspTextEdit,
) {
    modify_outgoing_range(state, document, &mut edit.range);
}

pub(crate) fn modify_incoming_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspTextEdit,
) {
    modify_incoming_range(state, document, &mut edit.range);
}

pub(crate) fn modify_incoming_diagnostic(
    state: &ServerState,
    document: &Document,
    diag: &mut LspDiagnostic,
) {
    modify_incoming_range(state, document, &mut diag.range);
    if let Some(related) = diag.related_information.as_mut() {
        for info in related {
            modify_incoming_location(state, document, &mut info.location);
        }
    }
}

pub(crate) fn modify_outgoing_diagnostic(
    state: &ServerState,
    document: &Document,
    diag: &mut LspDiagnostic,
) {
    let url = document.url().clone();
    modify_outgoing_diagnostic_at_url(state, document, &url, diag);
}

pub(crate) fn modify_outgoing_diagnostic_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    diag: &mut LspDiagnostic,
) {
    modify_outgoing_range_at_url(state, fallback, url, &mut diag.range);
    if let Some(related) = diag.related_information.as_mut() {
        for info in related {
            modify_outgoing_location(state, fallback, &mut info.location);
        }
    }
}

pub(crate) fn modify_outgoing_diagnostic_report_kind_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    report: &mut DocumentDiagnosticReportKind,
) {
    if let DocumentDiagnosticReportKind::Full(report) = report {
        for diag in &mut report.items {
            modify_outgoing_diagnostic_at_url(state, fallback, url, diag);
        }
    }
}

pub(crate) fn modify_outgoing_completion_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspCompletionTextEdit,
) {
    match edit {
        LspCompletionTextEdit::Edit(edit) => modify_outgoing_text_edit(state, document, edit),
        LspCompletionTextEdit::InsertAndReplace(edit) => {
            modify_outgoing_range(state, document, &mut edit.insert);
            modify_outgoing_range(state, document, &mut edit.replace);
        }
    }
}

pub(crate) fn modify_incoming_completion_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspCompletionTextEdit,
) {
    match edit {
        LspCompletionTextEdit::Edit(edit) => modify_incoming_text_edit(state, document, edit),
        LspCompletionTextEdit::InsertAndReplace(edit) => {
            modify_incoming_range(state, document, &mut edit.insert);
            modify_incoming_range(state, document, &mut edit.replace);
        }
    }
}

pub(crate) fn modify_outgoing_location_link(
    state: &ServerState,
    document: &Document,
    link: &mut LspLocationLink,
) {
    if let Some(origin_range) = link.origin_selection_range.as_mut() {
        modify_outgoing_range(state, document, origin_range);
    }

    modify_outgoing_range_at_url(state, document, &link.target_uri, &mut link.target_range);
    modify_outgoing_range_at_url(
        state,
        document,
        &link.target_uri,
        &mut link.target_selection_range,
    );
}

pub(crate) fn modify_outgoing_workspace_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspWorkspaceEdit,
) {
    use async_lsp::lsp_types::{DocumentChangeOperation, DocumentChanges};

    if let Some(changes) = edit.changes.as_mut() {
        for (uri, edits) in changes {
            for text_edit in edits.iter_mut() {
                modify_outgoing_range_at_url(state, document, uri, &mut text_edit.range);
            }
        }
    }

    if let Some(document_changes) = edit.document_changes.as_mut() {
        match document_changes {
            DocumentChanges::Edits(edits) => {
                for versioned_edit in edits.iter_mut() {
                    let uri = &versioned_edit.text_document.uri;
                    for text_edit in &mut versioned_edit.edits {
                        match text_edit {
                            OneOf::Left(l) => {
                                modify_outgoing_range_at_url(state, document, uri, &mut l.range);
                            }
                            OneOf::Right(r) => {
                                modify_outgoing_range_at_url(
                                    state,
                                    document,
                                    uri,
                                    &mut r.text_edit.range,
                                );
                            }
                        }
                    }
                }
            }
            DocumentChanges::Operations(ops) => {
                for op in ops.iter_mut() {
                    match op {
                        DocumentChangeOperation::Edit(edit) => {
                            let uri = &edit.text_document.uri;
                            for text_edit in &mut edit.edits {
                                match text_edit {
                                    OneOf::Left(l) => {
                                        modify_outgoing_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut l.range,
                                        );
                                    }
                                    OneOf::Right(r) => {
                                        modify_outgoing_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut r.text_edit.range,
                                        );
                                    }
                                }
                            }
                        }
                        DocumentChangeOperation::Op(_) => {
                            // File operations don't have positions to modify
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn modify_incoming_workspace_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspWorkspaceEdit,
) {
    use async_lsp::lsp_types::{DocumentChangeOperation, DocumentChanges};

    if let Some(changes) = edit.changes.as_mut() {
        for (uri, edits) in changes {
            for text_edit in edits.iter_mut() {
                modify_incoming_range_at_url(state, document, uri, &mut text_edit.range);
            }
        }
    }

    if let Some(document_changes) = edit.document_changes.as_mut() {
        match document_changes {
            DocumentChanges::Edits(edits) => {
                for versioned_edit in edits.iter_mut() {
                    let uri = &versioned_edit.text_document.uri;
                    for text_edit in &mut versioned_edit.edits {
                        match text_edit {
                            OneOf::Left(l) => {
                                modify_incoming_range_at_url(state, document, uri, &mut l.range);
                            }
                            OneOf::Right(r) => {
                                modify_incoming_range_at_url(
                                    state,
                                    document,
                                    uri,
                                    &mut r.text_edit.range,
                                );
                            }
                        }
                    }
                }
            }
            DocumentChanges::Operations(ops) => {
                for op in ops.iter_mut() {
                    match op {
                        DocumentChangeOperation::Edit(edit) => {
                            let uri = &edit.text_document.uri;
                            for text_edit in &mut edit.edits {
                                match text_edit {
                                    OneOf::Left(l) => {
                                        modify_incoming_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut l.range,
                                        );
                                    }
                                    OneOf::Right(r) => {
                                        modify_incoming_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut r.text_edit.range,
                                        );
                                    }
                                }
                            }
                        }
                        DocumentChangeOperation::Op(_) => {
                            // File operations don't have positions to modify
                        }
                    }
                }
            }
        }
    }
}
