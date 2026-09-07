use async_lsp::{ErrorCode, ResponseError};

struct DomainError;

impl From<DomainError> for ResponseError {
    fn from(_: DomainError) -> Self {
        Self::new(ErrorCode::INTERNAL_ERROR, "boom")
    }
}

fn main() {
    let error = DomainError;
    let _wire = ResponseError::from(error);
}
