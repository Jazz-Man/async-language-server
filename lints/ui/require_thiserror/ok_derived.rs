#[derive(Debug, thiserror::Error)]
enum DerivedError {
    #[error("e")]
    A,
}

fn main() {
    let _ = DerivedError::A;
}
