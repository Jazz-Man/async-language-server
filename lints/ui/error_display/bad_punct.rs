#[derive(Debug, thiserror::Error)]
enum E {
    #[error("failed to read the document.")]
    Read,
}

fn main() {
    let _ = E::Read;
}
