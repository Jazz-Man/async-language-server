//! One owner of batch parallelism: runs a fallible per-item future over
//! `items` with at most `width` in flight, returning results in input
//! order. Errors keep the caller's first-error-in-input-order semantics:
//! every item runs to completion, then the first `Err` (by input order)
//! propagates — for `workspace/diagnostic` that reproduces the serial
//! loop's all-or-nothing outcome, including `CONTENT_MODIFIED`.

use std::future::Future;

use futures::stream::{StreamExt as _, iter};

/// Runs `f` over `items` with at most `width` futures in flight and
/// returns the results in input order.
///
/// Every item runs to completion even when some fail; the error that
/// comes first in input order is the one propagated.
///
/// # Errors
///
/// Returns the first per-item error in input order.
pub(crate) async fn for_each_bounded<T, R, E, F, Fut>(
    items: Vec<T>,
    width: usize,
    f: F,
) -> Result<Vec<R>, E>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = Result<R, E>>,
{
    let mut indexed: Vec<(usize, Result<R, E>)> = iter(items.into_iter().enumerate())
        .map(|(index, item)| {
            let future = f(item);
            async move { (index, future.await) }
        })
        .buffer_unordered(width.max(1))
        .collect()
        .await;
    indexed.sort_unstable_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::for_each_bounded;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Semaphore, mpsc};

    async fn ok(value: u32) -> Result<u32, &'static str> {
        tokio::task::yield_now().await;
        Ok(value)
    }

    #[tokio::test]
    async fn results_return_in_input_order_regardless_of_completion() {
        // Later items finish first; order must still follow input.
        let items = vec![0u32, 1, 2, 3];
        let results = for_each_bounded(items, 4, |item| async move {
            for _ in 0..(4 - item) {
                tokio::task::yield_now().await;
            }
            ok(item).await
        })
        .await;
        assert_eq!(results, Ok(vec![0, 1, 2, 3]));
    }

    #[tokio::test]
    async fn width_bounds_concurrent_items() {
        let permits = Arc::new(Semaphore::new(0));
        let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
        let items = vec!['a', 'b', 'c'];
        let handle = tokio::spawn(for_each_bounded(items, 2, {
            let permits = Arc::clone(&permits);
            move |item| {
                let entered_tx = entered_tx.clone();
                let permits = Arc::clone(&permits);
                async move {
                    entered_tx.send(item).expect("channel open");
                    // Block until the test releases the cohort: item 'a' and
                    // 'b' enter, 'c' must not while both are parked.
                    permits.acquire().await.expect("semaphore open").forget();
                    Ok(item)
                }
            }
        }));
        entered_rx.recv().await.expect("first item enters");
        entered_rx.recv().await.expect("second item enters");
        assert!(
            tokio::time::timeout(Duration::from_millis(250), entered_rx.recv())
                .await
                .is_err(),
            "the third item must wait for a width slot",
        );
        permits.add_permits(1);
        permits.add_permits(10);
        let results: Result<Vec<char>, ()> = handle.await.expect("task joins");
        assert_eq!(results, Ok(vec!['a', 'b', 'c']));
    }
}
