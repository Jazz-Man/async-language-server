#[derive(Debug, thiserror::Error)]
enum E {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn main() {
    let _ = E::Io(std::io::Error::other("boom"));
}
