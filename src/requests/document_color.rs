#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::DocumentColorParams,
    response = Vec<async_lsp::lsp_types::ColorInformation>,
    document(text_document),
    outgoing(crate::requests::conversion::modify_outgoing_color_informations),
)]
pub(crate) struct DocumentColorRequest;

#[cfg(test)]
mod tests {
    use async_lsp::lsp_types::{Color, ColorInformation};

    use crate::requests::{DocumentColorRequest, Request};
    use crate::testing::{same_line, state_with_documents};

    #[test]
    fn document_color_ranges_convert_outgoing() {
        let (state, _plain, emoji) = state_with_documents();
        let document = state.document(&emoji).expect("emoji document is tracked");
        let mut response = vec![ColorInformation {
            range: same_line(0, 4, 5),
            color: Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
        }];

        <DocumentColorRequest as Request>::modify_response(&state, &document, &mut response);

        assert_eq!(response[0].range, same_line(0, 2, 3));
    }
}
