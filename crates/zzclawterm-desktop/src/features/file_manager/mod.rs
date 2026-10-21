mod operations;
mod pane;
mod runtime;
mod state;
mod tabs;
mod view;
mod workspace;

#[allow(unused_imports)]
pub(in crate::features) use pane::{PaneEndpoint, PaneId};
#[allow(unused_imports)]
pub(in crate::features) use state::{FileManagerFeatureState, FileManagerJobTarget};
#[allow(unused_imports)]
pub(in crate::features) use workspace::{FileManagerSeed, FileManagerWorkspace};
