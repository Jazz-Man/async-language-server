//! Centralized position-encoding conversion for the `Request` hooks.
//!
//! Two verb families live here: `convert_*` helpers are
//! direction-parameterized (`Direction::Incoming` = client encoding to
//! UTF-8, before the handler; `Direction::Outgoing` = UTF-8 to the client
//! encoding, after), while the remaining `modify_*` helpers are
//! fixed-direction composites that mix per-document and per-URL conversion —
//! no pure direction pins over a `convert_*` helper remain.

use async_lsp::lsp_types::{
    CompletionTextEdit as LspCompletionTextEdit, Diagnostic as LspDiagnostic,
    DocumentDiagnosticReportKind, GotoDefinitionResponse as LspGotoDefinitionResponse,
    Location as LspLocation, LocationLink as LspLocationLink, OneOf, Position as LspPosition,
    Range as LspRange, TextEdit as LspTextEdit, Url, WorkspaceEdit as LspWorkspaceEdit,
};

use crate::{
    server::{Document, ServerState},
    text_utils::{Encoding, position_to_encoding},
};

use super::Request;

/// Direction of an encoding conversion between the client's negotiated
/// position encoding and the crate-internal UTF-8.
///
/// [`Direction::Incoming`] converts values from the client encoding to UTF-8
/// (request params, before the handler); [`Direction::Outgoing`] converts them
/// back (responses, after the handler).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Direction {
    /// Client encoding → UTF-8.
    Incoming,
    /// UTF-8 → client encoding.
    Outgoing,
}

pub(crate) fn convert_position(
    state: &ServerState,
    document: &Document,
    position: &mut LspPosition,
    direction: Direction,
) {
    let (source, target) = match direction {
        Direction::Incoming => (state.get_position_encoding(), Encoding::UTF8),
        Direction::Outgoing => (Encoding::UTF8, state.get_position_encoding()),
    };
    *position = position_to_encoding(&document.text, *position, source, target);
}

pub(crate) fn convert_position_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    position: &mut LspPosition,
    direction: Direction,
) {
    if url == fallback.url() {
        convert_position(state, fallback, position, direction);
    } else if let Some(document) = state.document(url) {
        convert_position(state, &document, position, direction);
    } else {
        convert_position(state, fallback, position, direction);
    }
}

pub(crate) fn convert_range(
    state: &ServerState,
    document: &Document,
    range: &mut LspRange,
    direction: Direction,
) {
    convert_position(state, document, &mut range.start, direction);
    convert_position(state, document, &mut range.end, direction);
}

pub(crate) fn convert_range_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    range: &mut LspRange,
    direction: Direction,
) {
    convert_position_at_url(state, fallback, url, &mut range.start, direction);
    convert_position_at_url(state, fallback, url, &mut range.end, direction);
}

pub(crate) fn convert_location(
    state: &ServerState,
    document: &Document,
    loc: &mut LspLocation,
    direction: Direction,
) {
    let uri = loc.uri.clone();
    convert_range_at_url(state, document, &uri, &mut loc.range, direction);
}

pub(crate) fn convert_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspTextEdit,
    direction: Direction,
) {
    convert_range(state, document, &mut edit.range, direction);
}

pub(crate) fn convert_completion_text_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspCompletionTextEdit,
    direction: Direction,
) {
    match edit {
        LspCompletionTextEdit::Edit(edit) => convert_text_edit(state, document, edit, direction),
        LspCompletionTextEdit::InsertAndReplace(edit) => {
            convert_range(state, document, &mut edit.insert, direction);
            convert_range(state, document, &mut edit.replace, direction);
        }
    }
}

/// Converts each item of an optional response vector with `convert_item`,
/// threading `direction` to every item; a `None` response is left as-is.
pub(crate) fn convert_optional_vec<T>(
    state: &ServerState,
    document: &Document,
    items: &mut Option<Vec<T>>,
    direction: Direction,
    convert_item: fn(&ServerState, &Document, &mut T, Direction),
) {
    if let Some(items) = items {
        for item in items {
            convert_item(state, document, item, direction);
        }
    }
}

/// Converts a resolve request's item against the given document snapshot,
/// in `direction`.
///
/// Resolve requests carry no document URL, so there is no request document:
/// the caller supplies the snapshot to convert against — in the usual
/// completion-then-resolve or code-action-then-resolve flow the sole tracked
/// document, captured once for the whole request. The item passes through
/// unchanged when the negotiated encoding is already UTF-8 or no snapshot
/// was supplied. `R` is the item's [`Request`]; the bound pins its params
/// and response types to the same item type `T`, which is what a resolve
/// round trip does.
pub(crate) fn convert_resolve_item<R, T>(
    state: &ServerState,
    document: Option<&Document>,
    item: &mut T,
    direction: Direction,
) where
    R: Request<Params = T, Response = T>,
{
    if state.get_position_encoding() == Encoding::UTF8 {
        return;
    }
    let Some(document) = document else {
        return;
    };
    match direction {
        Direction::Incoming => R::modify_params(state, document, item),
        Direction::Outgoing => R::modify_response(state, document, item),
    }
}

/// Converts a diagnostic's range and related locations between the client
/// encoding and UTF-8 against the given document snapshot.
pub(crate) fn convert_diagnostic(
    state: &ServerState,
    document: &Document,
    diag: &mut LspDiagnostic,
    direction: Direction,
) {
    convert_range(state, document, &mut diag.range, direction);
    if let Some(related) = diag.related_information.as_mut() {
        for info in related {
            convert_location(state, document, &mut info.location, direction);
        }
    }
}

pub(crate) fn modify_outgoing_diagnostic_at_url(
    state: &ServerState,
    fallback: &Document,
    url: &Url,
    diag: &mut LspDiagnostic,
) {
    convert_range_at_url(state, fallback, url, &mut diag.range, Direction::Outgoing);
    if let Some(related) = diag.related_information.as_mut() {
        for info in related {
            convert_location(state, fallback, &mut info.location, Direction::Outgoing);
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

pub(crate) fn modify_outgoing_location_link(
    state: &ServerState,
    document: &Document,
    link: &mut LspLocationLink,
) {
    if let Some(origin_range) = link.origin_selection_range.as_mut() {
        convert_range(state, document, origin_range, Direction::Outgoing);
    }

    convert_range_at_url(
        state,
        document,
        &link.target_uri,
        &mut link.target_range,
        Direction::Outgoing,
    );
    convert_range_at_url(
        state,
        document,
        &link.target_uri,
        &mut link.target_selection_range,
        Direction::Outgoing,
    );
}

/// Converts a goto definition/declaration response (both share the
/// [`LspGotoDefinitionResponse`] type; `GotoDeclarationResponse` is an alias)
/// from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_goto_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspGotoDefinitionResponse>,
) {
    if let Some(response) = response {
        match response {
            LspGotoDefinitionResponse::Scalar(loc) => {
                convert_location(state, document, loc, Direction::Outgoing);
            }
            LspGotoDefinitionResponse::Array(locations) => {
                for loc in locations.iter_mut() {
                    convert_location(state, document, loc, Direction::Outgoing);
                }
            }
            LspGotoDefinitionResponse::Link(links) => {
                for link in links.iter_mut() {
                    modify_outgoing_location_link(state, document, link);
                }
            }
        }
    }
}

pub(crate) fn convert_workspace_edit(
    state: &ServerState,
    document: &Document,
    edit: &mut LspWorkspaceEdit,
    direction: Direction,
) {
    use async_lsp::lsp_types::{DocumentChangeOperation, DocumentChanges};

    if let Some(changes) = edit.changes.as_mut() {
        for (uri, edits) in changes {
            for text_edit in edits.iter_mut() {
                convert_range_at_url(state, document, uri, &mut text_edit.range, direction);
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
                                convert_range_at_url(state, document, uri, &mut l.range, direction);
                            }
                            OneOf::Right(r) => {
                                convert_range_at_url(
                                    state,
                                    document,
                                    uri,
                                    &mut r.text_edit.range,
                                    direction,
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
                                        convert_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut l.range,
                                            direction,
                                        );
                                    }
                                    OneOf::Right(r) => {
                                        convert_range_at_url(
                                            state,
                                            document,
                                            uri,
                                            &mut r.text_edit.range,
                                            direction,
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
