//! Virtual-list window sizes for the remote panels.
//!
//! These live here rather than in the views because the offsets they bound are
//! authoritative state: `RemoteOpsFeatureState` clamps a stored scroll offset whenever
//! the list behind it changes length, and it needs the viewport height to know what the
//! maximum offset is. The views import the same constants for their own windowing, so
//! there is one definition per list rather than one per reader.
//!
//! Docker's two were previously declared twice each -- `DOCKER_VIEWPORT_ROWS = 16` in
//! `docker/containers.rs` beside a bare `VIEWPORT_ROWS = 16` in `docker_view.rs`, and
//! the same for the resource list's 14 -- so the clamp and the window it was clamping
//! for agreed only by coincidence.

/// Container rows visible in the Docker containers list.
pub(in crate::features) const DOCKER_VIEWPORT_ROWS: usize = 16;

/// Rows visible in the Docker images/volumes/networks lists.
pub(in crate::features) const DOCKER_RESOURCE_VIEWPORT_ROWS: usize = 14;

/// Rows visible in the process table.
pub(in crate::features) const PROCESS_VIEWPORT_ROWS: usize = 28;

/// Rows visible in a GPU/NPU card's process list.
pub(in crate::features) const ACCELERATOR_PROCESS_VIEWPORT_ROWS: usize = 6;

/// The largest scroll offset that still shows a full viewport.
///
/// `min` before `saturating_sub` so a list shorter than the viewport pins to zero
/// rather than going negative.
pub(in crate::features) fn max_list_offset(total: usize, viewport_rows: usize) -> usize {
    total.saturating_sub(viewport_rows.min(total))
}

#[cfg(test)]
mod tests {
    use super::max_list_offset;

    #[test]
    fn a_list_shorter_than_the_viewport_pins_to_the_top() {
        assert_eq!(max_list_offset(0, 16), 0);
        assert_eq!(max_list_offset(1, 16), 0);
        assert_eq!(max_list_offset(16, 16), 0);
    }

    #[test]
    fn a_longer_list_can_scroll_by_the_overflow() {
        assert_eq!(max_list_offset(17, 16), 1);
        assert_eq!(max_list_offset(100, 16), 84);
    }
}
