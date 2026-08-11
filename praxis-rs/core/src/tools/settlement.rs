use std::future::Future;

use tokio_util::sync::CancellationToken;

/// Gives cancellation a deterministic linearization point against tool completion.
///
/// When both branches are ready, cancellation owns the terminal outcome. Once a
/// completion has been observed while the token is still live, later cancellation
/// cannot rewrite it.
pub(super) async fn settle_with_cancellation<T, F, C>(
    future: F,
    cancellation_token: &CancellationToken,
    cancelled: C,
) -> T
where
    F: Future<Output = T>,
    C: FnOnce() -> T,
{
    tokio::pin!(future);
    tokio::select! {
        biased;
        _ = cancellation_token.cancelled() => cancelled(),
        output = &mut future => {
            if cancellation_token.is_cancelled() {
                cancelled()
            } else {
                output
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    use super::*;

    #[tokio::test]
    async fn cancellation_wins_when_completion_is_already_ready() {
        let cancel = CancellationToken::new();
        cancel.cancel();

        let outcome =
            settle_with_cancellation(async { "completed" }, &cancel, || "cancelled").await;

        assert_eq!(outcome, "cancelled");
    }

    #[tokio::test]
    async fn completion_is_committed_once_while_token_is_live() {
        let cancel = CancellationToken::new();

        let outcome =
            settle_with_cancellation(async { "completed" }, &cancel, || "cancelled").await;
        cancel.cancel();

        assert_eq!(outcome, "completed");
    }

    #[tokio::test]
    async fn cancellation_drops_the_unfinished_tool_future() {
        struct DropSignal(Arc<AtomicBool>);

        impl Drop for DropSignal {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let signal = DropSignal(Arc::clone(&dropped));
        let cancel = CancellationToken::new();
        cancel.cancel();

        let outcome = settle_with_cancellation(
            async move {
                let _signal = signal;
                pending::<&'static str>().await
            },
            &cancel,
            || "cancelled",
        )
        .await;

        assert_eq!(outcome, "cancelled");
        assert!(dropped.load(Ordering::Acquire));
    }
}
