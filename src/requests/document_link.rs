use async_lsp::lsp_types::{
    DocumentLink as LspDocumentLink, DocumentLinkParams as LspDocumentLinkParams,
};

use crate::server::{Document, ServerState};

use super::{
    Request,
    conversion::{Direction, convert_range},
};

pub struct DocumentLink;

impl Request for DocumentLink {
    type Params = LspDocumentLinkParams;
    type Response = Option<Vec<LspDocumentLink>>;

    request_extract_url!(text_document);

    fn modify_response(state: &ServerState, document: &Document, response: &mut Self::Response) {
        if let Some(links) = response.as_mut() {
            for link in links.iter_mut() {
                convert_range(state, document, &mut link.range, Direction::Outgoing);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{
        DocumentLink as LspDocumentLink, DocumentLinkParams, PartialResultParams,
        TextDocumentIdentifier, WorkDoneProgressParams,
    };

    use crate::testing::{conversion_tests, line_position, same_line};

    use super::DocumentLink;

    conversion_tests! {
        document_link_ranges_convert_outgoing: DocumentLink {
            params: |uri| DocumentLinkParams {
                text_document: TextDocumentIdentifier::new(uri),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
            response: |plain, _emoji| Some(vec![LspDocumentLink {
                range: same_line(0, 4, 4),
                target: Some(plain),
                tooltip: None,
                data: None,
            }]),
            outgoing: |r| r.as_ref().expect("links present")[0].range.start,
            returns: line_position(0, 2),
        }
    }
}
