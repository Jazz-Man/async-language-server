#[derive(Debug, thiserror::Error)]
enum E {
    #[error("unknown failure")]
    Unknown { message: String },
}

fn main() {
    let _ = E::Unknown {
        message: String::new(),
    };
}
