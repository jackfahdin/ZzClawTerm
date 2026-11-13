//! Desktop adapters for native updater installation and progress UI.

pub(crate) use zzclawterm_updater::install::{
    PreparedUpdate, cleanup_update_work_dir, launch_installer, take_update_cleanup_path,
};

#[cfg(windows)]
mod progress;

pub fn run_update_helper_if_requested() -> bool {
    #[cfg(windows)]
    return zzclawterm_updater::install::run_update_helper_if_requested(progress::run);
    #[cfg(not(windows))]
    zzclawterm_updater::install::run_update_helper_if_requested(|_, operation| {
        operation(&mut |_| {})
    })
}
