fn main() {
    // async alternatives are not sync IO; nothing to flag
    let _ = "tokio::fs::read is not in the deny set";
}
