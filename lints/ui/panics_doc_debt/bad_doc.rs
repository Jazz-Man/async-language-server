/// Does a thing.
///
/// # Panics
///
/// Panics if `x` is zero.
pub fn thing(x: u32) {
    println!("{}", 1 / x);
}

fn main() {
    thing(1);
}
