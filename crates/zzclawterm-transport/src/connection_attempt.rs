use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// One dial attempt, shared by the pending tab, jump chain and authentication prompts.
#[derive(Clone, Debug, Default)]
pub struct ConnectionAttempt(Arc<AttemptState>);

#[derive(Debug, Default)]
struct AttemptState {
    cancelled: AtomicBool,
    shell: AtomicU8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellAvailability {
    #[default]
    Unknown,
    Available,
    Unavailable,
}

#[derive(Debug, thiserror::Error)]
#[error("This SSH connection does not support shell commands (SFTP-only server)")]
pub struct ShellUnavailable;

impl ConnectionAttempt {
    pub fn shell_availability(&self) -> ShellAvailability {
        match self.0.shell.load(Ordering::Acquire) {
            1 => ShellAvailability::Available,
            2 => ShellAvailability::Unavailable,
            _ => ShellAvailability::Unknown,
        }
    }

    pub(crate) fn ensure_shell_available(&self) -> Result<(), ShellUnavailable> {
        if self.shell_availability() == ShellAvailability::Unavailable {
            Err(ShellUnavailable)
        } else {
            Ok(())
        }
    }

    pub(crate) fn record_shell_reply(&self, accepted: bool) {
        self.0
            .shell
            .store(if accepted { 1 } else { 2 }, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("connection attempt cancelled".into())
        } else {
            Ok(())
        }
    }
    pub async fn until_cancelled<T>(
        &self,
        operation: impl std::future::Future<Output = T>,
    ) -> Result<T, String> {
        self.check()?;
        tokio::select! {
            result = operation => { self.check()?; Ok(result) },
            _ = async { while !self.is_cancelled() { tokio::time::sleep(Duration::from_millis(25)).await; } } => Err("connection attempt cancelled".into()),
        }
    }
    pub fn receive<T>(&self, receiver: &mpsc::Receiver<T>, timeout: Duration) -> Result<T, String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.check()?;
            if Instant::now() >= deadline {
                return Err("connection prompt timed out".into());
            }
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(value) => {
                    self.check()?;
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("connection prompt closed".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionAttempt;
    use std::time::Duration;

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_dial() {
        let attempt = ConnectionAttempt::default();
        let cancel = attempt.clone();
        let task =
            tokio::spawn(
                async move { attempt.until_cancelled(std::future::pending::<()>()).await },
            );
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }
}

#[cfg(test)]
mod capability_tests {
    use super::{ConnectionAttempt, ShellAvailability};

    #[test]
    fn shell_capability_is_shared_only_within_one_connection_attempt() {
        let first = ConnectionAttempt::default();
        let clone = first.clone();
        assert_eq!(first.shell_availability(), ShellAvailability::Unknown);
        first.record_shell_reply(true);
        assert_eq!(clone.shell_availability(), ShellAvailability::Available);
        first.record_shell_reply(false);
        assert!(clone.ensure_shell_available().is_err());
        assert_eq!(
            ConnectionAttempt::default().shell_availability(),
            ShellAvailability::Unknown
        );
    }

    #[tokio::test]
    async fn timeout_and_cancellation_do_not_claim_the_server_rejected_shell() {
        let attempt = ConnectionAttempt::default();
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(1),
            attempt.until_cancelled(std::future::pending::<()>()),
        )
        .await;
        assert_eq!(attempt.shell_availability(), ShellAvailability::Unknown);
        attempt.cancel();
        assert_eq!(attempt.shell_availability(), ShellAvailability::Unknown);
    }
}
