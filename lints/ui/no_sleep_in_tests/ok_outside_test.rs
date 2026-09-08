use std::{thread, time::Duration};

fn main() {
    // not test code: the lint deliberately does not fire here
    thread::sleep(Duration::from_millis(1));
}
