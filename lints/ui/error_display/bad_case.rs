#[derive(Debug, thiserror::Error)]
enum E {
    #[error("Failed to read the document")]
    Read,
}

fn main() {
    let _ = E::Read;
}
