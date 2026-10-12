//! One running job and one replaceable pending job, rather than a thread per input.
use std::sync::{Arc, Condvar, Mutex};

struct Queue<T> {
    pending: Option<T>,
    stopped: bool,
}

pub struct LatestWorker<T> {
    shared: Arc<(Mutex<Queue<T>>, Condvar)>,
}

impl<T: Send + 'static> LatestWorker<T> {
    pub fn new(name: &str, mut run: impl FnMut(T) + Send + 'static) -> std::io::Result<Self> {
        let shared = Arc::new((
            Mutex::new(Queue {
                pending: None,
                stopped: false,
            }),
            Condvar::new(),
        ));
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || loop {
                let job = {
                    let (lock, ready) = &*thread_shared;
                    let mut queue = lock.lock().unwrap_or_else(|error| error.into_inner());
                    while queue.pending.is_none() && !queue.stopped {
                        queue = ready.wait(queue).unwrap_or_else(|error| error.into_inner());
                    }
                    if queue.stopped {
                        return;
                    }
                    queue.pending.take().unwrap()
                };
                run(job);
            })?;
        Ok(Self { shared })
    }

    pub fn submit(&self, job: T) {
        let (lock, ready) = &*self.shared;
        let replaced = lock
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending
            .replace(job);
        drop(replaced);
        ready.notify_one();
    }
}

impl<T> Drop for LatestWorker<T> {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        let mut queue = lock.lock().unwrap_or_else(|error| error.into_inner());
        queue.stopped = true;
        queue.pending = None;
        ready.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn rapid_requests_keep_only_latest_pending_job() {
        let (started, events) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let worker = LatestWorker::new("latest-worker-test", move |job| {
            started.send(job).unwrap();
            if job == 0 {
                blocked.recv().unwrap();
            }
        })
        .unwrap();
        worker.submit(0);
        assert_eq!(events.recv_timeout(Duration::from_secs(2)).unwrap(), 0);
        for job in 1..=100 {
            worker.submit(job);
        }
        release.send(()).unwrap();
        assert_eq!(events.recv_timeout(Duration::from_secs(2)).unwrap(), 100);
        drop(worker);
        assert!(events.recv_timeout(Duration::from_secs(2)).is_err());
    }
}
