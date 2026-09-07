use async_lsp::{ErrorCode, ResponseError};

fn main() {
    let _e = ResponseError::new(ErrorCode::INTERNAL_ERROR, "boom");
}
