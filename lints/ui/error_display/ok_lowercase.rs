#[derive(Debug, thiserror::Error)]
enum E {
    #[error("failed to read the document")]
    Read,
    #[error("document {path} is invalid")]
    Invalid { path: std::path::PathBuf },
}

fn main() {
    let _ = (
        E::Read,
        E::Invalid {
            path: std::path::PathBuf::new(),
        },
    );
}
