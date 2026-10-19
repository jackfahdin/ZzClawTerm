use std::io;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_secs(2);
const DEADLINE: Duration = Duration::from_secs(10);

struct Clock {
    hello: bool,
    since: Instant,
    last_tick: Instant,
    next_ping: Instant,
    sequence: u64,
    pending: Option<u64>,
}

pub(crate) struct Heartbeat {
    clock: Mutex<Clock>,
    changed: Condvar,
    stopped: AtomicBool,
    failed: AtomicBool,
    blocked: AtomicBool,
}

impl Default for Heartbeat {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            clock: Mutex::new(Clock {
                hello: false,
                since: now,
                last_tick: now,
                next_ping: now,
                sequence: 0,
                pending: None,
            }),
            changed: Condvar::new(),
            stopped: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            blocked: AtomicBool::new(false),
        }
    }
}

impl Heartbeat {
    pub(crate) fn hello(&self) {
        self.clock.lock().unwrap_or_else(|p| p.into_inner()).hello = true;
    }
    pub(crate) fn pong(&self, sequence: u64) {
        let mut clock = self.clock.lock().unwrap_or_else(|p| p.into_inner());
        if clock.pending == Some(sequence) {
            clock.pending = None;
            clock.since = Instant::now();
            clock.next_ping = Instant::now() + INTERVAL;
        }
    }
    pub(crate) fn blocked(&self, blocked: bool) {
        self.blocked.store(blocked, Ordering::Release);
    }
    pub(crate) fn fail_once(&self) -> bool {
        !self.failed.swap(true, Ordering::AcqRel)
    }
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.changed.notify_all();
    }

    fn tick(&self, now: Instant) -> Result<Option<u64>, ()> {
        let mut clock = self.clock.lock().unwrap_or_else(|p| p.into_inner());
        // Sleep and long scheduling gaps require a fresh probe and full grace.
        if now.saturating_duration_since(clock.last_tick) > INTERVAL * 2
            || self.blocked.load(Ordering::Acquire)
        {
            clock.since = now;
            clock.pending = None;
            clock.next_ping = now;
        }
        clock.last_tick = now;
        if (!clock.hello || clock.pending.is_some())
            && now.saturating_duration_since(clock.since) >= DEADLINE
        {
            return Err(());
        }
        if clock.hello
            && clock.pending.is_none()
            && now >= clock.next_ping
            && !self.blocked.load(Ordering::Acquire)
        {
            clock.sequence = clock.sequence.wrapping_add(1);
            clock.pending = Some(clock.sequence);
            clock.since = now;
            return Ok(Some(clock.sequence));
        }
        Ok(None)
    }
}

pub(crate) struct Supervisor {
    heartbeat: Arc<Heartbeat>,
    worker: Option<JoinHandle<()>>,
}

impl Supervisor {
    pub(crate) fn spawn(
        heartbeat: Arc<Heartbeat>,
        child: Arc<Mutex<Option<Child>>>,
        ping: impl Fn(u64) + Send + 'static,
        timeout: impl Fn() + Send + 'static,
    ) -> io::Result<Self> {
        let state = Arc::clone(&heartbeat);
        let worker = zzclawterm_core::runtime::worker::spawn(
            "remote-helper-supervisor".into(),
            move || {
                loop {
                    if state.stopped.load(Ordering::Acquire) {
                        return;
                    }
                    if state.failed.load(Ordering::Acquire) {
                        break;
                    }
                    match state.tick(Instant::now()) {
                        Ok(Some(sequence)) => ping(sequence),
                        Ok(None) => {}
                        Err(()) => {
                            timeout();
                            break;
                        }
                    }
                    let clock = state.clock.lock().unwrap_or_else(|p| p.into_inner());
                    let _ = state
                        .changed
                        .wait_timeout(clock, Duration::from_millis(100));
                }
                // Only this generation's process handle is reachable by the supervisor.
                let mut child = child.lock().unwrap_or_else(|p| p.into_inner());
                crate::helper_process::cleanup_child(&mut child, &mut None);
            },
        )?;
        Ok(Self {
            heartbeat,
            worker: Some(worker),
        })
    }

    pub(crate) fn stop(&mut self) {
        self.heartbeat.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{DEADLINE, Heartbeat, INTERVAL};
    use std::time::{Duration, Instant};
    #[test]
    fn wrong_pongs_do_not_extend_timeout_and_backpressure_and_sleep_get_grace() {
        let hb = Heartbeat::default();
        hb.hello();
        let start = Instant::now();
        let sequence = hb.tick(start).unwrap().unwrap();
        hb.pong(sequence + 1);
        for second in 1..10 {
            assert!(hb.tick(start + Duration::from_secs(second)).is_ok());
        }
        assert!(hb.tick(start + DEADLINE).is_err());
        hb.blocked(true);
        assert!(hb.tick(start + DEADLINE + INTERVAL).is_ok());
        hb.blocked(false);
        assert!(hb.tick(start + Duration::from_secs(100)).unwrap().is_some());
    }
    #[test]
    fn supervisor_reaps_hung_process_once_during_handshake_and_after_hello() {
        use super::Supervisor;
        use std::process::{Command, Stdio};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        for hello in [false, true] {
            #[cfg(windows)]
            let mut command = {
                use std::os::windows::process::CommandExt;
                let mut command = Command::new("cmd.exe");
                command.args(["/d", "/c", "set /p wait="]);
                command.creation_flags(0x0800_0000);
                command
            };
            #[cfg(not(windows))]
            let mut command = {
                let mut command = Command::new("sh");
                command.args(["-c", "read wait"]);
                command
            };
            let child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let child = Arc::new(Mutex::new(Some(child)));
            let heartbeat = Arc::new(Heartbeat::default());
            if hello {
                heartbeat.hello();
                heartbeat.tick(Instant::now()).unwrap();
            }
            // Expire only the response clock, without simulating suspension.
            heartbeat.clock.lock().unwrap().since = Instant::now() - DEADLINE;
            let failures = Arc::new(AtomicUsize::new(0));
            let timeout_failures = failures.clone();
            let timeout_heartbeat = heartbeat.clone();
            let mut supervisor = Supervisor::spawn(
                heartbeat,
                child.clone(),
                |_| {},
                move || {
                    if timeout_heartbeat.fail_once() {
                        timeout_failures.fetch_add(1, Ordering::SeqCst);
                    }
                },
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if child.lock().unwrap().is_none() {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "supervisor did not reap the process"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            supervisor.stop();
            assert_eq!(failures.load(Ordering::SeqCst), 1);
        }
    }
}
