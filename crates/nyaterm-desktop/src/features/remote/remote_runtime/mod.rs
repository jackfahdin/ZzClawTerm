mod auto_refresh;
mod panel_layout;

pub(in crate::features) use auto_refresh::remote_refresh_due;
mod docker;
mod gpu;
mod helpers;
mod process;
mod stats;
