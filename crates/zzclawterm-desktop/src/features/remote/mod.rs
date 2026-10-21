//! Remote host operations: Docker, processes and stats runtime.

mod job_state;
mod list_window;
mod remote_runtime;

pub(in crate::features) use list_window::{ACCELERATOR_PROCESS_VIEWPORT_ROWS, max_list_offset};
mod state;

pub(in crate::features) use state::{
    DockerDerivedItems, DockerPresentationState, GpuPresentationState, NpuPresentationState,
    ProcessPresentationState, ProcessSortColumns, RemoteOpsFeatureFocus, RemoteOpsFeatureState,
    StatsPresentationState,
};
