//! One cancellable playback resolve. A manual selection promotes matching lookahead work or
//! cancels unrelated work; background preparation never displaces foreground playback.
use std::sync::{Arc, Mutex};

use futures_util::future::{AbortHandle, Abortable, BoxFuture, Shared};
use futures_util::FutureExt;

type Pending<T> = Shared<BoxFuture<'static, Option<T>>>;

struct Job<K, T: Clone> {
    key: K,
    owner: Arc<()>,
    foreground: bool,
    abort: AbortHandle,
    future: Pending<T>,
}

pub struct Resolver<K, T: Clone> {
    job: Mutex<Option<Job<K, T>>>,
}

impl<K, T: Clone> Default for Resolver<K, T> {
    fn default() -> Self {
        Self { job: Mutex::new(None) }
    }
}

impl<K: Eq, T: Clone + Send + Sync + 'static> Resolver<K, T> {
    pub fn clear(&self) {
        if let Some(job) = self.job.lock().unwrap().take() {
            job.abort.abort();
        }
    }

    /// Synchronous admission lets the caller validate queue/generation under its queue lock.
    /// The returned future owns no queue guard. Dropping obsolete callers is safe.
    pub fn request(
        &self,
        key: K,
        foreground: bool,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Option<(Arc<()>, Pending<T>, bool)> {
        let mut slot = self.job.lock().unwrap();
        if let Some(job) = slot.as_mut() {
            if job.key == key {
                job.foreground |= foreground;
                return Some((job.owner.clone(), job.future.clone(), true));
            }
            if !foreground {
                return None;
            }
            job.abort.abort();
        }
        let (abort, registration) = AbortHandle::new_pair();
        let future = async move { Abortable::new(work, registration).await.ok() }.boxed().shared();
        let owner = Arc::new(());
        *slot = Some(Job { key, owner: owner.clone(), foreground, abort, future: future.clone() });
        Some((owner, future, false))
    }

    pub fn finish(&self, owner: &Arc<()>) {
        let mut slot = self.job.lock().unwrap();
        if slot.as_ref().is_some_and(|job| Arc::ptr_eq(&job.owner, owner)) {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn distant_radio_selection_does_not_resolve_intermediate_rows() {
        let resolver = super::Resolver::default();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (_, near, _) =
            resolver.request((1, 1), false, std::future::pending::<usize>()).unwrap();
        let count = calls.clone();
        let (_, distant, _) = resolver
            .request((1, 20), true, async move {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                20
            })
            .unwrap();
        assert_eq!(near.await, None);
        assert_eq!(distant.await, Some(20));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let (_, stale, _) =
            resolver.request((1, 19), true, std::future::pending::<usize>()).unwrap();
        let (_, replaced, _) = resolver.request((2, 20), true, async { 99 }).unwrap();
        assert_eq!(stale.await, None);
        assert_eq!(replaced.await, Some(99));
    }
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn manual_selection_promotes_the_same_inflight_successor() {
        let resolver = Resolver::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let (_, background, _) = resolver
            .request((7, "song"), false, async move {
                counted.fetch_add(1, Ordering::SeqCst);
                42
            })
            .unwrap();
        let (owner, selected, reused) = resolver.request((7, "song"), true, async { 99 }).unwrap();
        assert!(reused);
        assert_eq!(tokio::join!(background, selected), (Some(42), Some(42)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        resolver.finish(&owner);
        assert!(resolver.job.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn rapid_selection_cancels_old_work_and_background_cannot_replace_it() {
        let resolver = Resolver::default();
        let (old_owner, old, _) =
            resolver.request("a", true, std::future::pending::<u8>()).unwrap();
        let (_, middle, _) = resolver.request("b", true, async { 2 }).unwrap();
        let (owner, latest, _) = resolver.request("c", true, async { 3 }).unwrap();
        resolver.finish(&old_owner);
        assert!(resolver.request("background", false, async { 4 }).is_none());
        assert_eq!(tokio::join!(old, middle, latest), (None, None, Some(3)));
        resolver.finish(&owner);
    }

    #[tokio::test]
    async fn queue_replacement_and_session_reset_do_not_reuse_old_results() {
        let resolver = Resolver::default();
        let (_, old, _) = resolver.request((1, "same-id"), false, async { 1 }).unwrap();
        let (_, new, reused) = resolver.request((2, "same-id"), true, async { 2 }).unwrap();
        assert!(!reused);
        assert_eq!(old.await, None);
        resolver.clear();
        assert_eq!(new.await, None);
    }

    #[tokio::test]
    async fn failures_are_not_retained_as_a_cache() {
        let resolver = Resolver::default();
        let (owner, failed, _) =
            resolver.request("song", true, async { Err::<u8, _>("offline") }).unwrap();
        assert_eq!(failed.await, Some(Err("offline")));
        resolver.finish(&owner);
        let (_, retry, reused) = resolver.request("song", true, async { Ok(1) }).unwrap();
        assert!(!reused);
        assert_eq!(retry.await, Some(Ok(1)));
    }
}
