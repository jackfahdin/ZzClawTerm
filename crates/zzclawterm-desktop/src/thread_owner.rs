//! Central construction for named threads with caller-owned join handles.

use std::io;
use std::thread::{self, JoinHandle};

pub(crate) fn spawn_joinable(
    name: impl Into<String>,
    run: impl FnOnce() + Send + 'static,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.into()).spawn(run)
}
