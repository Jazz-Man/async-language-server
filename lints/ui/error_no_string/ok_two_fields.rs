#[derive(Debug, thiserror::Error)]
enum E {
    #[error("json-rpc error {code}: {message}")]
    Rpc { code: i32, message: String },
}

fn main() {
    let _ = E::Rpc {
        code: 0,
        message: String::new(),
    };
}
