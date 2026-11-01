//! Latest-snapshot work: one worker, no backlog, and no stale displayed result.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, TryRecvError},
};

struct Pending<K, V> {
    key: K,
    cancelled: Arc<AtomicBool>,
    receiver: Mutex<mpsc::Receiver<V>>,
}

pub(crate) struct SnapshotJob<K, V> {
    wanted: Option<K>,
    completed: bool,
    result: Option<V>,
    pending: Option<Pending<K, V>>,
}

impl<K, V> Default for SnapshotJob<K, V> {
    fn default() -> Self {
        Self {
            wanted: None,
            completed: false,
            result: None,
            pending: None,
        }
    }
}

impl<K, V> Drop for SnapshotJob<K, V> {
    fn drop(&mut self) {
        if let Some(job) = &self.pending {
            job.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

impl<K: Clone + PartialEq, V: Send + 'static> SnapshotJob<K, V> {
    pub fn clear(&mut self) {
        if let Some(job) = &self.pending {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        self.wanted = None;
        self.completed = false;
        self.result = None;
        self.poll();
    }

    fn poll(&mut self) {
        let Some(job) = &self.pending else { return };
        let response = job
            .receiver
            .lock()
            .expect("measurement receiver")
            .try_recv();
        match response {
            Ok(value) => {
                if self.wanted.as_ref() == Some(&job.key) && !job.cancelled.load(Ordering::Relaxed)
                {
                    self.result = Some(value);
                    self.completed = true;
                }
                self.pending = None;
            }
            Err(TryRecvError::Disconnected) => self.pending = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    /// The factory runs only when starting work, so cloning large snapshots is
    /// avoided on ordinary frames. A cancelled worker must finish before the
    /// newest snapshot starts; intermediate requests are never queued.
    pub fn update<F, W>(&mut self, key: K, factory: F) -> Option<&V>
    where
        F: FnOnce() -> W,
        W: FnOnce(Arc<AtomicBool>) -> V + Send + 'static,
    {
        if self.wanted.as_ref() != Some(&key) {
            self.clear();
            self.wanted = Some(key.clone());
        }
        self.poll();
        if !self.completed && self.pending.is_none() {
            let (sender, receiver) = mpsc::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            let worker_cancelled = Arc::clone(&cancelled);
            let work = factory();
            std::thread::Builder::new()
                .name("visible-measurement".into())
                .spawn(move || {
                    let value = work(worker_cancelled);
                    let _ = sender.send(value);
                })
                .expect("start measurement worker");
            self.pending = Some(Pending {
                key,
                cancelled,
                receiver: Mutex::new(receiver),
            });
        }
        self.result.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(value) = check() {
                return value;
            }
            assert!(Instant::now() < deadline, "worker did not complete");
            std::thread::yield_now();
        }
    }

    #[test]
    fn superseded_work_is_cancelled_without_backlog_or_stale_results() {
        let mut job = SnapshotJob::default();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        assert!(
            job.update(1, || move |cancelled| {
                started_tx.send(Arc::clone(&cancelled)).unwrap();
                release_rx.recv().unwrap();
                100
            })
            .is_none()
        );
        let cancelled = started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(
            job.update(2, || |_| panic!("must not queue a second worker"))
                .is_none()
        );
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(
            job.update(3, || |_| panic!("must not queue intermediate work"))
                .is_none()
        );
        release_tx.send(()).unwrap();
        let value = wait_for(|| job.update(4, || |_| 400).copied());
        assert_eq!(value, 400);
        assert_eq!(
            job.update(4, || |_| panic!("must reuse completed snapshot")),
            Some(&400)
        );
    }

    #[test]
    fn disable_discards_completed_values_and_cancels_running_work() {
        let mut job = SnapshotJob::default();
        assert_eq!(wait_for(|| job.update(1, || |_| 100).copied()), 100);
        job.clear();
        assert!(job.result.is_none());
        assert_eq!(wait_for(|| job.update(1, || |_| 200).copied()), 200);
    }

    #[test]
    fn returning_to_a_cancelled_snapshot_cannot_accept_its_old_result() {
        let mut job = SnapshotJob::default();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        job.update(1, || {
            move |cancelled| {
                started_tx.send(cancelled).unwrap();
                release_rx.recv().unwrap();
                100
            }
        });
        let cancelled = started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        job.clear();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(job.update(1, || |_| 200).is_none());
        release_tx.send(()).unwrap();
        assert_eq!(wait_for(|| job.update(1, || |_| 200).copied()), 200);
    }
}
