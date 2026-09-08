use std::path::Path;

fn main() {
    let _ = std::fs::read_to_string("x.txt");
    let _ = Path::new("x.txt").exists();
}
