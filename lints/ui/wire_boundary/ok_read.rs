use async_lsp::ResponseError;

fn describe(error: &ResponseError) -> String {
    format!("{}: {}", error.code, error.message)
}

fn main() {
    // Reading a wire error is fine — the rule is about construction. A value
    // can only come from the wire boundary; none is constructed here.
    let error: Option<ResponseError> = None;
    if let Some(error) = error {
        println!("{}", describe(&error));
    }
}
