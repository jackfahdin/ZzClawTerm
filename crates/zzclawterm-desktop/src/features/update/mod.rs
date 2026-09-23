//! Native application update state and runtime.

pub(in crate::features) mod download;
pub(crate) mod install;
mod state;
mod update_runtime;

pub(crate) use state::{UpdateCheckKind, UpdateEvent, UpdatePhase, UpdateStore};
