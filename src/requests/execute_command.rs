#[lsp_macros::lsp_request(
    params = async_lsp::lsp_types::ExecuteCommandParams,
    response = Option<async_lsp::lsp_types::LSPAny>,
)]
pub(crate) struct ExecuteCommandRequest;
