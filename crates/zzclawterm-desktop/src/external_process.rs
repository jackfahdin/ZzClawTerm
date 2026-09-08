//! Shared child-process and pipe-reader ownership for external Agent CLIs.

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use crate::thread_owner::spawn_joinable;

pub(crate) struct ExternalAgentChild {
    child: Child,
    readers: Vec<JoinHandle<()>>,
}

impl ExternalAgentChild {
    pub fn new(child: Child) -> Self {
        Self {
            child,
            readers: Vec::with_capacity(2),
        }
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub fn capture_output(
        &mut self,
        worker_name: &'static str,
        stdout: ChildStdout,
        stderr: ChildStderr,
        log_stderr: impl Fn(String) + Send + 'static,
    ) -> std::io::Result<Receiver<Result<String, String>>> {
        let (line_tx, line_rx) = mpsc::channel();
        self.readers.push(spawn_joinable(
            format!("{worker_name}-stdout"),
            move || {
                for line in BufReader::new(stdout).lines() {
                    if line_tx
                        .send(line.map_err(|error| error.to_string()))
                        .is_err()
                    {
                        break;
                    }
                }
            },
        )?);
        self.readers.push(spawn_joinable(
            format!("{worker_name}-stderr"),
            move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log_stderr(line);
                }
            },
        )?);
        Ok(line_rx)
    }
}

impl Drop for ExternalAgentChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
