//! Bounded background ownership transfer for blocking resource destruction.
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex};

struct State<T> {
    jobs: VecDeque<T>,
    closed: bool,
}

pub struct Reaper<T> {
    shared: Arc<(Mutex<State<T>>, Condvar)>,
    capacity: usize,
}

impl<T: Send + 'static> Reaper<T> {
    pub fn new(name: &str, capacity: usize, run: fn(T)) -> io::Result<Self> {
        let reaper = Self {
            shared: Arc::new((
                Mutex::new(State {
                    jobs: VecDeque::new(),
                    closed: false,
                }),
                Condvar::new(),
            )),
            capacity,
        };
        for index in 0..2 {
            let shared = Arc::clone(&reaper.shared);
            crate::runtime::worker::spawn(format!("{name}-{index}"), move || {
                loop {
                    let job = {
                        let (lock, changed) = &*shared;
                        let mut state = lock.lock().unwrap_or_else(|p| p.into_inner());
                        while state.jobs.is_empty() && !state.closed {
                            state = changed.wait(state).unwrap_or_else(|p| p.into_inner());
                        }
                        match state.jobs.pop_front() {
                            Some(job) => job,
                            None => return,
                        }
                    };
                    run(job);
                }
            })?;
        }
        Ok(reaper)
    }

    /// The factory is called only after capacity is reserved. A failed submission
    /// must never move resources out of their authoritative session registry.
    pub fn try_submit_with(&self, take: impl FnOnce() -> Option<T>) -> io::Result<()> {
        let (lock, changed) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| io::Error::other("reaper lock poisoned"))?;
        if state.closed || state.jobs.len() >= self.capacity {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "session reaper is full",
            ));
        }
        state.jobs.try_reserve(1).map_err(io::Error::other)?;
        if let Some(job) = take() {
            state.jobs.push_back(job);
            changed.notify_one();
        }
        Ok(())
    }
}

impl<T> Drop for Reaper<T> {
    fn drop(&mut self) {
        let (lock, changed) = &*self.shared;
        lock.lock().unwrap_or_else(|p| p.into_inner()).closed = true;
        changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::Reaper;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn full_queue_never_takes_ownership_from_the_caller() {
        type Job = (mpsc::Sender<()>, mpsc::Receiver<()>);
        let reaper = Reaper::new("reaper-capacity-test", 1, |(started, release): Job| {
            started.send(()).unwrap();
            let _ = release.recv();
        })
        .unwrap();
        let (started, ready) = mpsc::channel();
        let mut releases = Vec::new();
        for _ in 0..2 {
            let (release, wait) = mpsc::channel();
            reaper
                .try_submit_with(|| Some((started.clone(), wait)))
                .unwrap();
            ready.recv_timeout(Duration::from_secs(1)).unwrap();
            releases.push(release);
        }
        let (release, wait) = mpsc::channel();
        reaper
            .try_submit_with(|| Some((started.clone(), wait)))
            .unwrap();
        let mut taken = false;
        assert_eq!(
            reaper
                .try_submit_with(|| {
                    taken = true;
                    None
                })
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!taken);
        drop(releases);
        drop(release);
    }

    #[test]
    fn worker_initialization_failure_returns_an_error() {
        for stage in 0..2 {
            let _failure = crate::runtime::worker::fail_after(stage);
            assert!(Reaper::<()>::new("reaper-start-failure", 1, |_| {}).is_err());
        }
    }
}
