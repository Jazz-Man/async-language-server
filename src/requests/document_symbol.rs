#[cfg(test)]
#[allow(
    deprecated,
    reason = "fixtures construct `DocumentSymbol`, whose upstream `deprecated` \
              field is marked `#[deprecated]` yet still required in struct literals"
)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentSymbol as LspDocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
        PartialResultParams, SymbolKind, TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::requests::{DocumentSymbol, Request};
    use crate::testing::{conversion_tests, line_position, same_line, state_with_documents};

    conversion_tests! {
        document_symbol_nested_outgoing_utf8_becomes_utf16: DocumentSymbol {
            params: |uri| DocumentSymbolParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            response: |_plain, _emoji| {
                Some(DocumentSymbolResponse::Nested(vec![LspDocumentSymbol {
                    name: "f".into(),
                    detail: None,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: same_line(0, 4, 4),
                    selection_range: same_line(0, 4, 4),
                    children: None,
                }]))
            },
            outgoing: |r| match r.as_ref().expect("response present") {
                DocumentSymbolResponse::Nested(symbols) => symbols[0].range.start,
                DocumentSymbolResponse::Flat(_) => panic!("expected nested symbols"),
            },
            returns: line_position(0, 2),
        }
    }

    #[test]
    fn document_symbol_nested_children_convert_recursively() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = Some(DocumentSymbolResponse::Nested(vec![LspDocumentSymbol {
            name: "f".into(),
            detail: None,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: same_line(0, 4, 6),
            selection_range: same_line(0, 4, 5),
            children: Some(vec![LspDocumentSymbol {
                name: "x".into(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: same_line(0, 5, 6),
                selection_range: same_line(0, 4, 5),
                children: None,
            }]),
        }]));

        <DocumentSymbol as Request>::modify_response(&state, &document, &mut response);

        let DocumentSymbolResponse::Nested(symbols) = response.expect("response present") else {
            panic!("expected nested symbols");
        };
        assert_eq!(symbols[0].range, same_line(0, 2, 4));
        assert_eq!(symbols[0].selection_range, same_line(0, 2, 3));
        let children = symbols[0].children.as_ref().expect("children present");
        assert_eq!(children[0].range, same_line(0, 3, 4));
        assert_eq!(children[0].selection_range, same_line(0, 2, 3));
    }
}
