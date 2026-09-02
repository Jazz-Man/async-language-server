//! Centralized position-encoding conversion for the `Request` hooks.
//!
//! Two verb families live here: `convert_*` helpers are
//! direction-parameterized (`Direction::Incoming` = client encoding to
//! UTF-8, before the handler; `Direction::Outgoing` = UTF-8 to the client
//! encoding, after), while the remaining `modify_*` helpers are
//! fixed-direction composites that mix per-document and per-URL conversion —
//! no pure direction pins over a `convert_*` helper remain.

use async_lsp::lsp_types::{
    CallHierarchyIncomingCall as LspCallHierarchyIncomingCall,
    CallHierarchyItem as LspCallHierarchyItem,
    CallHierarchyOutgoingCall as LspCallHierarchyOutgoingCall, CodeLens as LspCodeLens,
    ColorInformation as LspColorInformation, ColorPresentation as LspColorPresentation,
    CompletionTextEdit as LspCompletionTextEdit, Diagnostic as LspDiagnostic,
    DocumentDiagnosticReportKind, DocumentHighlight as LspDocumentHighlight,
    DocumentLink as LspDocumentLink, FoldingRange as LspFoldingRange,
    GotoDefinitionResponse as LspGotoDefinitionResponse, Hover as LspHover,
    InlayHint as LspInlayHint, InlayHintLabel as LspInlayHintLabel,
    LinkedEditingRanges as LspLinkedEditingRanges, Location as LspLocation,
    LocationLink as LspLocationLink, OneOf, ParameterLabel as LspParameterLabel,
    Position as LspPosition, PrepareRenameResponse as LspPrepareRenameResponse, Range as LspRange,
    SignatureHelp as LspSignatureHelp, TextEdit as LspTextEdit,
    TypeHierarchyItem as LspTypeHierarchyItem, Url, WorkspaceEdit as LspWorkspaceEdit,
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

/// Converts a call hierarchy item's two ranges between encodings, against
/// the item's own document when tracked, falling back to `document`.
pub(crate) fn convert_call_hierarchy_item(
    state: &ServerState,
    document: &Document,
    item: &mut LspCallHierarchyItem,
    direction: Direction,
) {
    let uri = item.uri.clone();
    convert_range_at_url(state, document, &uri, &mut item.range, direction);
    convert_range_at_url(state, document, &uri, &mut item.selection_range, direction);
}

/// Converts a type hierarchy item's two ranges between encodings, against
/// the item's own document when tracked, falling back to `document`.
pub(crate) fn convert_type_hierarchy_item(
    state: &ServerState,
    document: &Document,
    item: &mut LspTypeHierarchyItem,
    direction: Direction,
) {
    let uri = item.uri.clone();
    convert_range_at_url(state, document, &uri, &mut item.range, direction);
    convert_range_at_url(state, document, &uri, &mut item.selection_range, direction);
}

/// Converts an incoming call's `from` item and every range in `from_ranges`
/// between encodings — the ranges sit in the caller's document, so they
/// convert against the `from` item's own document when tracked, falling
/// back to `document`.
pub(crate) fn convert_call_hierarchy_incoming_call(
    state: &ServerState,
    document: &Document,
    call: &mut LspCallHierarchyIncomingCall,
    direction: Direction,
) {
    convert_call_hierarchy_item(state, document, &mut call.from, direction);
    for range in &mut call.from_ranges {
        convert_range_at_url(state, document, &call.from.uri, range, direction);
    }
}

/// Converts an outgoing call's `to` item and every range in `from_ranges`
/// between encodings. The ranges sit in the caller's — the request's —
/// document, so they convert against `document` directly; only the `to`
/// item's own ranges follow its URI.
pub(crate) fn convert_call_hierarchy_outgoing_call(
    state: &ServerState,
    document: &Document,
    call: &mut LspCallHierarchyOutgoingCall,
    direction: Direction,
) {
    convert_call_hierarchy_item(state, document, &mut call.to, direction);
    for range in &mut call.from_ranges {
        convert_range(state, document, range, direction);
    }
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
    convert_item: impl Fn(&ServerState, &Document, &mut T, Direction),
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

/// Converts a hover's optional range from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_hover(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspHover>,
) {
    if let Some(hover) = response
        && let Some(range) = hover.range.as_mut()
    {
        convert_range(state, document, range, Direction::Outgoing);
    }
}

/// Converts each location of a references response from UTF-8 to the
/// client encoding.
pub(crate) fn modify_outgoing_locations(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspLocation>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_location,
    );
}

/// Converts each link's range of a documentLink response from UTF-8 to
/// the client encoding.
pub(crate) fn modify_outgoing_document_links(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspDocumentLink>>,
) {
    if let Some(links) = response {
        for link in links {
            convert_range(state, document, &mut link.range, Direction::Outgoing);
        }
    }
}

/// Converts each highlight's range of a documentHighlight response from
/// UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_document_highlights(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspDocumentHighlight>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        |state, document, highlight, direction| {
            convert_range(state, document, &mut highlight.range, direction);
        },
    );
}

/// Converts each folding range's optional character columns from UTF-8 to
/// the client encoding; line numbers are encoding-independent.
pub(crate) fn modify_outgoing_folding_ranges(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspFoldingRange>>,
) {
    if let Some(ranges) = response {
        for range in ranges {
            convert_folding_range(state, document, range, Direction::Outgoing);
        }
    }
}

fn convert_folding_range(
    state: &ServerState,
    document: &Document,
    range: &mut LspFoldingRange,
    direction: Direction,
) {
    if let Some(character) = range.start_character.as_mut() {
        let mut position = LspPosition {
            line: range.start_line,
            character: *character,
        };
        convert_position(state, document, &mut position, direction);
        *character = position.character;
    }
    if let Some(character) = range.end_character.as_mut() {
        let mut position = LspPosition {
            line: range.end_line,
            character: *character,
        };
        convert_position(state, document, &mut position, direction);
        *character = position.character;
    }
}

/// Converts each range of a linkedEditingRange response from UTF-8 to the
/// client encoding.
pub(crate) fn modify_outgoing_linked_editing_ranges(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspLinkedEditingRanges>,
) {
    if let Some(ranges) = response {
        for range in &mut ranges.ranges {
            convert_range(state, document, range, Direction::Outgoing);
        }
    }
}

/// Converts each code lens's range from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_code_lenses(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspCodeLens>>,
) {
    if let Some(lenses) = response {
        for lens in lenses {
            convert_range(state, document, &mut lens.range, Direction::Outgoing);
        }
    }
}

/// Converts each color information's range from UTF-8 to the client
/// encoding. The documentColor result is a bare vector.
pub(crate) fn modify_outgoing_color_informations(
    state: &ServerState,
    document: &Document,
    response: &mut Vec<LspColorInformation>,
) {
    for information in response {
        convert_range(state, document, &mut information.range, Direction::Outgoing);
    }
}

/// Converts each presentation's edits from UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_color_presentations(
    state: &ServerState,
    document: &Document,
    response: &mut Vec<LspColorPresentation>,
) {
    for presentation in response {
        if let Some(edit) = presentation.text_edit.as_mut() {
            convert_text_edit(state, document, edit, Direction::Outgoing);
        }
        if let Some(edits) = presentation.additional_text_edits.as_mut() {
            for edit in edits {
                convert_text_edit(state, document, edit, Direction::Outgoing);
            }
        }
    }
}

/// Converts each item's ranges of a prepareCallHierarchy response from
/// UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_call_hierarchy_items(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspCallHierarchyItem>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_call_hierarchy_item,
    );
}

/// Converts each item's ranges of a prepareTypeHierarchy response from
/// UTF-8 to the client encoding.
pub(crate) fn modify_outgoing_type_hierarchy_items(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspTypeHierarchyItem>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_type_hierarchy_item,
    );
}

/// Converts each edit's range of a formatting-family response from UTF-8
/// to the client encoding.
pub(crate) fn modify_outgoing_text_edits(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspTextEdit>>,
) {
    convert_optional_vec(
        state,
        document,
        response,
        Direction::Outgoing,
        convert_text_edit,
    );
}

/// Converts a rename response's workspace edit from UTF-8 to the client
/// encoding (per-URL against tracked documents, falling back to the
/// request document).
pub(crate) fn modify_outgoing_workspace_edit(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspWorkspaceEdit>,
) {
    if let Some(edit) = response {
        convert_workspace_edit(state, document, edit, Direction::Outgoing);
    }
}

/// Converts a prepareRename response's range from UTF-8 to the client
/// encoding; the placeholder and default-behavior variants carry no
/// positions.
pub(crate) fn modify_outgoing_prepare_rename_response(
    state: &ServerState,
    document: &Document,
    response: &mut Option<LspPrepareRenameResponse>,
) {
    if let Some(response) = response {
        match response {
            LspPrepareRenameResponse::Range(range)
            | LspPrepareRenameResponse::RangeWithPlaceholder { range, .. } => {
                convert_range(state, document, range, Direction::Outgoing);
            }
            LspPrepareRenameResponse::DefaultBehavior { .. } => {}
        }
    }
}

/// Converts each inlay hint's position and text-edit ranges from UTF-8 to
/// the client encoding against the request document, and each label-part
/// location against the document its URL points at.
pub(crate) fn modify_outgoing_inlay_hints(
    state: &ServerState,
    document: &Document,
    response: &mut Option<Vec<LspInlayHint>>,
) {
    let Some(hints) = response else { return };
    for hint in hints {
        convert_position(state, document, &mut hint.position, Direction::Outgoing);
        if let Some(edits) = hint.text_edits.as_mut() {
            for edit in edits {
                convert_text_edit(state, document, edit, Direction::Outgoing);
            }
        }
        if let LspInlayHintLabel::LabelParts(parts) = &mut hint.label {
            for part in parts {
                if let Some(location) = part.location.as_mut() {
                    convert_location(state, document, location, Direction::Outgoing);
                }
            }
        }
    }
}

/// Converts a signature help response's parameter label offsets from UTF-8
/// to the client encoding, recounting them against the containing signature
/// label string. `Simple` labels are substrings of the label and carry no
/// offsets.
pub(crate) fn modify_outgoing_signature_help(
    state: &ServerState,
    // Label offsets count code units of the label string itself, so no
    // document snapshot takes part in the conversion.
    _document: &Document,
    response: &mut Option<LspSignatureHelp>,
) {
    let Some(help) = response else { return };
    let encoding = state.get_position_encoding();
    for signature in &mut help.signatures {
        let Some(parameters) = signature.parameters.as_mut() else {
            continue;
        };
        for parameter in parameters {
            if let LspParameterLabel::LabelOffsets(offsets) = &mut parameter.label {
                convert_label_offsets(&signature.label, offsets, Encoding::UTF8, encoding);
            }
        }
    }
}

/// Recounts `[start, end]` code-unit offsets of `label` from one encoding
/// to another. Signature-help parameter labels are offsets into their
/// containing signature label string, so conversion needs only the string
/// itself, never the document.
fn convert_label_offsets(label: &str, offsets: &mut [u32; 2], from: Encoding, to: Encoding) {
    if from == to {
        return;
    }
    for offset in offsets {
        let target = usize::try_from(*offset).unwrap_or(usize::MAX);
        let mut seen_from: usize = 0;
        let mut converted: usize = 0;
        for ch in label.chars() {
            if seen_from >= target {
                break;
            }
            seen_from += match from {
                Encoding::UTF8 => ch.len_utf8(),
                Encoding::UTF16 => ch.len_utf16(),
                Encoding::UTF32 => 1,
            };
            converted += match to {
                Encoding::UTF8 => ch.len_utf8(),
                Encoding::UTF16 => ch.len_utf16(),
                Encoding::UTF32 => 1,
            };
        }
        *offset = u32::try_from(converted).unwrap_or(u32::MAX);
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

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, TextDocumentEdit,
        TextEdit, WorkspaceEdit,
    };

    use crate::requests::{Request, WillCreateFiles};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn workspace_edit_document_changes_edits_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: emoji,
                    version: None,
                },
                edits: vec![OneOf::Left(TextEdit {
                    range: same_line(0, 4, 4),
                    new_text: "x".into(),
                })],
            }])),
            ..WorkspaceEdit::default()
        });

        <WillCreateFiles as Request>::modify_response(&state, &document, &mut response);

        let edits = response
            .expect("edit present")
            .document_changes
            .expect("document changes present");
        let DocumentChanges::Edits(edits) = edits else {
            panic!("expected edits");
        };
        let [OneOf::Left(edit)] = edits[0].edits.as_slice() else {
            panic!("expected one left edit");
        };
        // Keyed at the emoji document: UTF-8 byte 4 converts to client 2.
        assert_eq!(edit.range, same_line(0, 2, 2));
    }
}
