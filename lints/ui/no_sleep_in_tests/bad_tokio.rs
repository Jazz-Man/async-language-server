#[tokio::test]
async fn waits() {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}

fn main() {}
