#[derive(Debug, thiserror::Error)]
enum E {
    #[error("unknown failure")]
    Unknown(String),
}

fn main() {
    let _ = E::Unknown(String::new());
}
