mod docker;
mod docker_view;
mod panels;
mod process;
mod process_view;
mod stats_view;

pub(in crate::features) use panels::{RemoteMonitorKind, RemotePanels};
pub(in crate::features) use process::{ProcessDisplayMode, process_display_mode};
