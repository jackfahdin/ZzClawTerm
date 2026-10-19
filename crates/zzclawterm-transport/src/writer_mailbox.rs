use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::TransportWriterCommand;

pub(crate) const BYTE_LIMIT: usize = 8 * 1024 * 1024;
pub(crate) const ITEM_LIMIT: usize = 1024;

#[derive(Default)]
struct State {
    queue: VecDeque<TransportWriterCommand>,
    bytes: usize,
    peak_bytes: usize,
    items: usize,
    stopped: bool,
}

#[derive(Clone, Default)]
pub(crate) struct WriterMailbox(Arc<(Mutex<State>, Condvar)>);

pub(crate) struct WritePermit {
    mailbox: WriterMailbox,
    bytes: usize,
}

fn byte_cost(command: &TransportWriterCommand) -> usize {
    match command {
        TransportWriterCommand::Write(data) | TransportWriterCommand::TelnetControl(data) => {
            data.len()
        }
    }
}

impl WriterMailbox {
    pub(crate) fn peak_bytes(&self) -> usize {
        self.0
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .peak_bytes
    }

    pub(crate) fn wait_capacity(
        &self,
        bytes: usize,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> io::Result<()> {
        use std::sync::atomic::Ordering;
        if bytes > BYTE_LIMIT {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let (lock, changed) = &*self.0;
        let mut state = lock
            .lock()
            .map_err(|_| io::Error::other("writer mailbox poisoned"))?;
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if state.stopped {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            if bytes <= BYTE_LIMIT.saturating_sub(state.bytes) && state.items < ITEM_LIMIT {
                return Ok(());
            }
            state = changed
                .wait_timeout(state, Duration::from_millis(25))
                .map_err(|_| io::Error::other("writer mailbox poisoned"))?
                .0;
        }
    }

    pub(crate) fn send(&self, command: TransportWriterCommand) -> io::Result<()> {
        let bytes = byte_cost(&command);
        let (lock, changed) = &*self.0;
        let mut state = lock
            .lock()
            .map_err(|_| io::Error::other("writer mailbox poisoned"))?;
        if state.stopped {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "transport writer stopped",
            ));
        }
        if bytes > BYTE_LIMIT.saturating_sub(state.bytes) || state.items >= ITEM_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "transport input queue is full",
            ));
        }
        state.queue.try_reserve(1).map_err(io::Error::other)?;
        state.bytes += bytes;
        state.peak_bytes = state.peak_bytes.max(state.bytes);
        state.items += 1;
        state.queue.push_back(command);
        changed.notify_all();
        Ok(())
    }

    pub(crate) fn check_capacity(&self, bytes: usize) -> io::Result<()> {
        let state = self
            .0
            .0
            .lock()
            .map_err(|_| io::Error::other("writer mailbox poisoned"))?;
        if state.stopped {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "transport writer stopped",
            ));
        }
        if bytes > BYTE_LIMIT.saturating_sub(state.bytes) || state.items >= ITEM_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "transport input queue is full",
            ));
        }
        Ok(())
    }

    pub(crate) fn receive(
        &self,
        timeout: Option<Duration>,
    ) -> io::Result<Option<(TransportWriterCommand, WritePermit)>> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let (lock, changed) = &*self.0;
        let mut state = lock
            .lock()
            .map_err(|_| io::Error::other("writer mailbox poisoned"))?;
        loop {
            if state.stopped {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "transport writer stopped",
                ));
            }
            if let Some(command) = state.queue.pop_front() {
                let bytes = byte_cost(&command);
                return Ok(Some((
                    command,
                    WritePermit {
                        mailbox: self.clone(),
                        bytes,
                    },
                )));
            }
            if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Ok(None);
                }
                state = changed
                    .wait_timeout(state, remaining)
                    .map_err(|_| io::Error::other("writer mailbox poisoned"))?
                    .0;
            } else {
                state = changed
                    .wait(state)
                    .map_err(|_| io::Error::other("writer mailbox poisoned"))?;
            }
        }
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.0.0.lock().unwrap_or_else(|p| p.into_inner()).stopped
    }

    pub(crate) fn stop(&self) {
        let (lock, changed) = &*self.0;
        let mut state = lock.lock().unwrap_or_else(|p| p.into_inner());
        state.stopped = true;
        while let Some(command) = state.queue.pop_front() {
            state.bytes -= byte_cost(&command);
            state.items -= 1;
        }
        changed.notify_all();
    }
}

impl Drop for WritePermit {
    fn drop(&mut self) {
        let mut state = self.mailbox.0.0.lock().unwrap_or_else(|p| p.into_inner());
        state.bytes -= self.bytes;
        state.items -= 1;
        drop(state);
        self.mailbox.0.1.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::{BYTE_LIMIT, ITEM_LIMIT, WriterMailbox};
    use crate::TransportWriterCommand;
    use std::io::ErrorKind;

    #[test]
    fn in_flight_bytes_count_until_completion_and_stop_discards_pending() {
        let mailbox = WriterMailbox::default();
        mailbox
            .send(TransportWriterCommand::Write(vec![0; BYTE_LIMIT]))
            .unwrap();
        let (_, permit) = mailbox.receive(None).unwrap().unwrap();
        assert_eq!(
            mailbox
                .send(TransportWriterCommand::Write(vec![1]))
                .unwrap_err()
                .kind(),
            ErrorKind::WouldBlock
        );
        drop(permit);
        mailbox
            .send(TransportWriterCommand::Write(vec![1, 2]))
            .unwrap();
        mailbox.stop();
        assert_eq!(mailbox.0.0.lock().unwrap().bytes, 0);
        assert!(mailbox.receive(None).is_err());
    }

    #[test]
    fn item_limit_includes_controls_and_rejected_writes_do_not_reorder() {
        let mailbox = WriterMailbox::default();
        for index in 0..ITEM_LIMIT {
            mailbox
                .send(TransportWriterCommand::TelnetControl(vec![
                    (index % 256) as u8,
                ]))
                .unwrap();
        }
        assert_eq!(
            mailbox
                .send(TransportWriterCommand::Write(vec![0]))
                .unwrap_err()
                .kind(),
            ErrorKind::WouldBlock
        );
        for index in 0..ITEM_LIMIT {
            let (command, _permit) = mailbox.receive(None).unwrap().unwrap();
            assert!(
                matches!(command, TransportWriterCommand::TelnetControl(data) if data == vec![(index % 256) as u8])
            );
        }
    }
}

#[cfg(test)]
mod waiter_tests {
    use super::{BYTE_LIMIT, WriterMailbox};
    use crate::TransportWriterCommand;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn data_arrival_wakes_writer_even_while_a_larger_capacity_waiter_is_parked() {
        let mailbox = WriterMailbox::default();
        mailbox
            .send(TransportWriterCommand::Write(vec![0; BYTE_LIMIT / 2]))
            .unwrap();
        let (_, in_flight) = mailbox.receive(None).unwrap().unwrap();
        let capacity = mailbox.clone();
        let waiter =
            std::thread::spawn(move || capacity.wait_capacity(BYTE_LIMIT, &AtomicBool::new(false)));
        // This waiter remains ineligible when the one-byte write arrives.
        std::thread::sleep(Duration::from_millis(20));
        let reader = mailbox.clone();
        let (tx, rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            let result = reader.receive(Some(Duration::from_secs(2))).unwrap();
            tx.send(result.is_some()).unwrap();
        });
        std::thread::sleep(Duration::from_millis(20));
        mailbox
            .send(TransportWriterCommand::Write(vec![1]))
            .unwrap();
        let received = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(in_flight);
        waiter.join().unwrap().unwrap();
        writer.join().unwrap();
        assert!(
            received,
            "capacity waiter must not steal the data-arrival wake"
        );
    }
}
