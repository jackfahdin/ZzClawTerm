//! A release is a candidate until the ordinary drop handlers have had their turn.
//! Losing GPUI's active drag, by itself, is only cancellation.

use std::collections::{HashMap, HashSet};

use gpui::{App, Bounds, DisplayId, Pixels, Point, Size, Window, WindowBounds, point, px, size};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use zzclawterm_core::WorkspaceId;

use super::window_state::{MainWindowPlacement, clamp_window_bounds};

#[derive(Clone)]
pub(crate) struct TabDragSource {
    pub workspace_id: WorkspaceId,
    pub root_tab_id: String,
    pub revision: u64,
    pub grab_offset: Point<Pixels>,
    pub screen_bounds: Option<Bounds<Pixels>>,
    pub normal_size: Size<Pixels>,
}

#[derive(Clone, Copy)]
pub(crate) struct TabDragRelease {
    /// Physical screen pixels on Windows, GPUI screen coordinates elsewhere.
    pub screen_position: Option<Point<Pixels>>,
    pub outside_source: bool,
}

struct ActiveTabDrag {
    id: u64,
    source: TabDragSource,
    release: Option<TabDragRelease>,
}

#[derive(Default)]
pub(crate) struct TabDragCoordinator {
    next_id: u64,
    active: Option<ActiveTabDrag>,
    normal_sizes: HashMap<WorkspaceId, Size<Pixels>>,
    failed_windows: HashSet<WorkspaceId>,
}

impl TabDragCoordinator {
    pub fn remember_normal_size(&mut self, workspace: WorkspaceId, size: Size<Pixels>) {
        self.normal_sizes.insert(workspace, size);
    }

    pub fn normal_size(&self, workspace: WorkspaceId) -> Size<Pixels> {
        self.normal_sizes
            .get(&workspace)
            .copied()
            .unwrap_or(size(px(1280.), px(800.)))
    }

    pub fn begin(&mut self, source: TabDragSource) {
        self.next_id += 1;
        self.active = Some(ActiveTabDrag {
            id: self.next_id,
            source,
            release: None,
        });
    }

    pub fn source(&self) -> Option<&TabDragSource> {
        self.active.as_ref().map(|drag| &drag.source)
    }

    #[cfg(test)]
    pub fn pending_release(&self) -> Option<TabDragRelease> {
        self.active.as_ref()?.release
    }

    pub fn released(&mut self, release: TabDragRelease) -> Option<u64> {
        let drag = self.active.as_mut()?;
        if drag.release.is_some() {
            return None;
        }
        drag.release = Some(release);
        Some(drag.id)
    }

    pub fn accept(&mut self, workspace: WorkspaceId, root_tab_id: &str, revision: u64) {
        if self.active.as_ref().is_some_and(|drag| {
            drag.source.workspace_id == workspace
                && drag.source.root_tab_id == root_tab_id
                && drag.source.revision == revision
        }) {
            self.active = None;
        }
    }

    pub fn finish(&mut self, id: u64) -> Option<(TabDragSource, TabDragRelease)> {
        if self.active.as_ref()?.id != id {
            return None;
        }
        let drag = self.active.take()?;
        let release = drag.release?;
        release.outside_source.then_some((drag.source, release))
    }

    pub fn cancel(&mut self) -> bool {
        self.active.take().is_some()
    }

    pub fn cancel_without_release(&mut self, workspace: WorkspaceId) {
        if self
            .active
            .as_ref()
            .is_some_and(|drag| drag.source.workspace_id == workspace && drag.release.is_none())
        {
            self.active = None;
        }
    }

    pub fn close(&mut self, workspace: WorkspaceId) {
        self.normal_sizes.remove(&workspace);
        self.failed_windows.remove(&workspace);
        self.cancel_workspace(workspace);
    }

    pub fn cancel_workspace(&mut self, workspace: WorkspaceId) {
        if self
            .source()
            .is_some_and(|source| source.workspace_id == workspace)
        {
            self.cancel();
        }
    }

    pub fn queue_failed_window(&mut self, workspace: WorkspaceId) {
        self.failed_windows.insert(workspace);
    }

    pub fn next_failed_window(&mut self) -> Option<WorkspaceId> {
        let workspace = self.failed_windows.iter().next().copied()?;
        self.failed_windows.remove(&workspace);
        Some(workspace)
    }
}

pub(crate) fn screen_position(window: &Window, local: Point<Pixels>) -> Option<Point<Pixels>> {
    let handle = HasWindowHandle::window_handle(window).ok()?.as_raw();
    if matches!(handle, RawWindowHandle::Wayland(_)) {
        return None;
    }
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        let mut position = POINT {
            x: (f32::from(local.x) * window.scale_factor()).round() as i32,
            y: (f32::from(local.y) * window.scale_factor()).round() as i32,
        };
        // Mouse-up coordinates belong to the receiving HWND, which can have a
        // different DPI from the source. Convert before selecting the monitor.
        if unsafe { ClientToScreen(handle.hwnd.get() as _, &mut position) } == 0 {
            return None;
        }
        return Some(point(px(position.x as f32), px(position.y as f32)));
    }
    Some(window.bounds().origin + local)
}

pub(crate) fn screen_bounds(window: &Window) -> Option<Bounds<Pixels>> {
    let origin = screen_position(window, point(px(0.), px(0.)))?;
    let scale = if cfg!(windows) {
        window.scale_factor()
    } else {
        1.
    };
    Some(Bounds::new(
        origin,
        window.viewport_size().map(|value| value * scale),
    ))
}

pub(crate) fn release_at(
    source: &TabDragSource,
    receiver: WorkspaceId,
    window: &Window,
    local: Point<Pixels>,
) -> TabDragRelease {
    let position = screen_position(window, local);
    let outside_source = if receiver == source.workspace_id {
        !Bounds::new(point(px(0.), px(0.)), window.viewport_size()).contains(&local)
    } else {
        match (position, source.screen_bounds) {
            (Some(position), Some(bounds)) => !bounds.contains(&position),
            // Wayland supplies surface-relative coordinates under the implicit
            // pointer grab. A release routed to another surface is outside the source.
            _ => true,
        }
    };
    TabDragRelease {
        screen_position: position,
        outside_source,
    }
}

pub(crate) fn detach_placement(
    source: &TabDragSource,
    release: TabDragRelease,
    cx: &App,
) -> Option<MainWindowPlacement> {
    let screen = release.screen_position?;
    let displays = cx.displays();
    #[cfg(windows)]
    let (display, logical_position) = {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
        };
        let monitor = unsafe {
            MonitorFromPoint(
                POINT {
                    x: f32::from(screen.x).round() as i32,
                    y: f32::from(screen.y).round() as i32,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        };
        let display = displays
            .iter()
            .find(|display| display.id() == DisplayId::new(monitor as u64))?;
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return None;
        }
        let scale = (info.rcMonitor.right - info.rcMonitor.left) as f32
            / f32::from(display.bounds().size.width);
        (display, logical_screen_position(screen, scale))
    };
    #[cfg(not(windows))]
    let (display, logical_position) = {
        let display = displays
            .iter()
            .find(|display| display.bounds().contains(&screen))
            .or_else(|| displays.first())?;
        (display, screen)
    };
    Some(placement_near_release(
        display.id(),
        display.visible_bounds(),
        logical_position,
        source.grab_offset,
        source.normal_size,
    ))
}

#[cfg(any(windows, test))]
fn logical_screen_position(physical: Point<Pixels>, scale: f32) -> Point<Pixels> {
    point(physical.x / scale, physical.y / scale)
}

fn placement_near_release(
    display_id: DisplayId,
    visible: Bounds<Pixels>,
    position: Point<Pixels>,
    offset: Point<Pixels>,
    normal_size: Size<Pixels>,
) -> MainWindowPlacement {
    MainWindowPlacement {
        display_id: Some(display_id),
        window_bounds: WindowBounds::Windowed(clamp_window_bounds(
            Bounds::new(position - offset, normal_size),
            visible,
            true,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TabDragCoordinator, TabDragRelease, TabDragSource, logical_screen_position,
        placement_near_release,
    };
    use gpui::{Bounds, DisplayId, WindowBounds, point, px, size};
    use zzclawterm_core::WorkspaceId;

    fn source() -> TabDragSource {
        TabDragSource {
            workspace_id: WorkspaceId::new(),
            root_tab_id: "tab".into(),
            revision: 7,
            grab_offset: point(px(40.), px(16.)),
            screen_bounds: None,
            normal_size: size(px(800.), px(600.)),
        }
    }

    fn release(outside_source: bool) -> TabDragRelease {
        TabDragRelease {
            screen_position: None,
            outside_source,
        }
    }

    #[test]
    fn outside_release_detaches_once_and_is_not_lost_to_hover_cleanup() {
        let mut state = TabDragCoordinator::default();
        let source = source();
        state.begin(source.clone());
        let id = state.released(release(true)).unwrap();
        state.cancel_without_release(source.workspace_id);
        assert!(state.released(release(true)).is_none());
        assert!(state.finish(id).is_some());
        assert!(state.finish(id).is_none());
    }

    #[test]
    fn accepted_drop_wins_over_an_outside_release_even_if_transfer_will_fail() {
        let mut state = TabDragCoordinator::default();
        let source = source();
        state.begin(source.clone());
        let id = state.released(release(true)).unwrap();
        state.accept(source.workspace_id, &source.root_tab_id, source.revision);
        assert!(state.finish(id).is_none());
    }

    #[test]
    fn inside_release_and_escape_keep_the_source_tab() {
        let mut state = TabDragCoordinator::default();
        state.begin(source());
        let id = state.released(release(false)).unwrap();
        assert!(state.finish(id).is_none());
        state.begin(source());
        assert!(state.cancel());
        assert!(state.released(release(true)).is_none());
    }

    #[test]
    fn old_deferred_release_cannot_consume_a_new_drag() {
        let mut state = TabDragCoordinator::default();
        state.begin(source());
        let old_id = state.released(release(true)).unwrap();
        state.begin(source());
        assert!(state.finish(old_id).is_none());
        let new_id = state.released(release(true)).unwrap();
        assert!(state.finish(new_id).is_some());
    }

    #[test]
    fn interrupted_drag_and_closed_source_never_detach() {
        let mut state = TabDragCoordinator::default();
        let source = source();
        state.begin(source.clone());
        state.cancel_without_release(source.workspace_id);
        assert!(state.released(release(true)).is_none());
        state.begin(source.clone());
        let id = state.released(release(true)).unwrap();
        state.close(source.workspace_id);
        assert!(state.finish(id).is_none());
    }

    #[test]
    fn placement_preserves_grab_offset_and_stays_on_negative_coordinate_display() {
        let visible = Bounds::new(point(px(-1920.), px(0.)), size(px(1920.), px(1080.)));
        let placement = placement_near_release(
            DisplayId::new(1),
            visible,
            point(px(-1500.), px(200.)),
            point(px(40.), px(16.)),
            size(px(800.), px(600.)),
        );
        assert_eq!(
            placement.window_bounds,
            WindowBounds::Windowed(Bounds::new(
                point(px(-1540.), px(184.)),
                size(px(800.), px(600.))
            ))
        );
        let edge = placement_near_release(
            DisplayId::new(1),
            visible,
            point(px(-1.), px(1079.)),
            point(px(40.), px(16.)),
            size(px(800.), px(600.)),
        );
        assert_eq!(
            edge.window_bounds.get_bounds().origin,
            point(px(-800.), px(480.))
        );
    }

    #[test]
    fn mixed_dpi_placement_converts_screen_position_using_target_scale() {
        assert_eq!(
            logical_screen_position(point(px(3200.), px(-600.)), 2.),
            point(px(1600.), px(-300.))
        );
    }
}
