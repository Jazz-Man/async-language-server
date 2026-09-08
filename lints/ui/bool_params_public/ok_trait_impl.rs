pub trait Conf {
    fn configure(&self, verbose: bool);
}

pub struct S;

impl Conf for S {
    fn configure(&self, verbose: bool) {
        let _ = verbose;
    }
}

fn main() {
    S.configure(true);
}
