use gpui::{App, Bounds, DisplayId, Pixels, WindowBounds, point, px, size};
use nyaterm_core::{AppRuntime, MainWindowState};
use nyaterm_store::{
    BootstrapSnapshot, LoadBootstrap, LoadMainWindowState, StoreConfig, StoreRuntime, StoreTask,
};

const DEFAULT_MAIN_WINDOW_WIDTH: f32 = 1280.;
const DEFAULT_MAIN_WINDOW_HEIGHT: f32 = 800.;

pub struct AppShellStartup {
    pub(super) store_runtime: Option<StoreRuntime>,
    pub(super) pending_bootstrap: Option<StoreTask<BootstrapSnapshot>>,
    pub(super) recovery: Option<StartupRecovery>,
    main_window_state: Option<MainWindowState>,
}

pub(super) struct StartupRecovery {
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MainWindowPlacement {
    pub display_id: Option<DisplayId>,
    pub window_bounds: WindowBounds,
}

#[derive(Debug, Clone)]
struct DisplayGeometry {
    id: DisplayId,
    uuid: Option<String>,
    visible_bounds: Bounds<Pixels>,
}

impl AppShellStartup {
    pub fn prepare(runtime: &AppRuntime) -> Self {
        let store_runtime = match StoreRuntime::spawn(StoreConfig {
            config_dir: runtime.config_dir().to_path_buf(),
            portable_key_path: runtime.portable_key_path().map(ToOwned::to_owned),
        }) {
            Ok(runtime) => runtime,
            Err(error) => {
                return Self {
                    store_runtime: None,
                    pending_bootstrap: None,
                    recovery: Some(StartupRecovery {
                        category: "worker_start".to_string(),
                        message: error.to_string(),
                    }),
                    main_window_state: None,
                };
            }
        };

        let main_window_state = match store_runtime
            .blocking_client()
            .request(0, LoadMainWindowState)
        {
            Ok(event) => match event.outcome {
                Ok(state) => state,
                Err(error) => {
                    tracing::warn!(
                        category = error.category(),
                        "main window state could not be restored"
                    );
                    None
                }
            },
            Err(error) => {
                tracing::warn!(category = %error, "main window state request failed");
                None
            }
        };

        match store_runtime.ui_client().try_submit(0, LoadBootstrap) {
            Ok(task) => Self {
                store_runtime: Some(store_runtime),
                pending_bootstrap: Some(task),
                recovery: None,
                main_window_state,
            },
            Err(error) => Self {
                store_runtime: None,
                pending_bootstrap: None,
                recovery: Some(StartupRecovery {
                    category: "request_submit".to_string(),
                    message: error.to_string(),
                }),
                main_window_state,
            },
        }
    }

    pub fn main_window_placement(&self, cx: &App) -> MainWindowPlacement {
        let displays = cx
            .displays()
            .into_iter()
            .map(|display| DisplayGeometry {
                id: display.id(),
                uuid: display.uuid().ok().map(|uuid| uuid.to_string()),
                visible_bounds: display.visible_bounds(),
            })
            .collect::<Vec<_>>();
        let primary_id = cx.primary_display().map(|display| display.id());
        resolve_main_window_placement(self.main_window_state.as_ref(), &displays, primary_id, cx)
    }
}

fn resolve_main_window_placement(
    state: Option<&MainWindowState>,
    displays: &[DisplayGeometry],
    primary_id: Option<DisplayId>,
    cx: &App,
) -> MainWindowPlacement {
    let default = || MainWindowPlacement {
        display_id: None,
        window_bounds: WindowBounds::Windowed(Bounds::centered(
            None,
            size(
                px(DEFAULT_MAIN_WINDOW_WIDTH),
                px(DEFAULT_MAIN_WINDOW_HEIGHT),
            ),
            cx,
        )),
    };
    resolve_saved_window_placement(state, displays, primary_id).unwrap_or_else(default)
}

fn resolve_saved_window_placement(
    state: Option<&MainWindowState>,
    displays: &[DisplayGeometry],
    primary_id: Option<DisplayId>,
) -> Option<MainWindowPlacement> {
    let state = state.filter(|state| state.validate().is_ok())?;
    let saved = Bounds {
        origin: point(
            px(state.restore_bounds.x as f32),
            px(state.restore_bounds.y as f32),
        ),
        size: size(
            px(state.restore_bounds.width as f32),
            px(state.restore_bounds.height as f32),
        ),
    };

    let primary = primary_id
        .and_then(|id| displays.iter().find(|display| display.id == id))
        .or_else(|| displays.first());
    let (target, preserve_origin) = if let Some(uuid) = state.display_uuid {
        let uuid = uuid.to_string();
        match displays
            .iter()
            .find(|display| display.uuid.as_deref() == Some(uuid.as_str()))
        {
            Some(display) => (Some(display), true),
            None => (primary, false),
        }
    } else if let Some(display) = displays
        .iter()
        .filter_map(|display| {
            let area = intersection_area(saved, display.visible_bounds);
            (area > 0.).then_some((display, area))
        })
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(display, _)| display)
    {
        (Some(display), true)
    } else {
        (primary, false)
    };
    let target = target?;

    let bounds = clamp_window_bounds(saved, target.visible_bounds, preserve_origin);
    let window_bounds = if state.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    Some(MainWindowPlacement {
        display_id: Some(target.id),
        window_bounds,
    })
}

fn intersection_area(left: Bounds<Pixels>, right: Bounds<Pixels>) -> f32 {
    let x1 = f32::from(left.origin.x).max(f32::from(right.origin.x));
    let y1 = f32::from(left.origin.y).max(f32::from(right.origin.y));
    let x2 = (f32::from(left.origin.x) + f32::from(left.size.width))
        .min(f32::from(right.origin.x) + f32::from(right.size.width));
    let y2 = (f32::from(left.origin.y) + f32::from(left.size.height))
        .min(f32::from(right.origin.y) + f32::from(right.size.height));
    (x2 - x1).max(0.) * (y2 - y1).max(0.)
}

fn clamp_window_bounds(
    saved: Bounds<Pixels>,
    visible: Bounds<Pixels>,
    preserve_origin: bool,
) -> Bounds<Pixels> {
    let visible_width = f32::from(visible.size.width).max(1.);
    let visible_height = f32::from(visible.size.height).max(1.);
    let width = f32::from(saved.size.width).clamp(1., visible_width);
    let height = f32::from(saved.size.height).clamp(1., visible_height);
    let visible_x = f32::from(visible.origin.x);
    let visible_y = f32::from(visible.origin.y);
    let (x, y) = if preserve_origin {
        (
            f32::from(saved.origin.x).clamp(visible_x, visible_x + visible_width - width),
            f32::from(saved.origin.y).clamp(visible_y, visible_y + visible_height - height),
        )
    } else {
        (
            visible_x + (visible_width - width) / 2.,
            visible_y + (visible_height - height) / 2.,
        )
    };
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width), px(height)),
    }
}

pub(super) const MAIN_WINDOW_STATE_SAVE_DEBOUNCE: std::time::Duration =
    std::time::Duration::from_millis(100);

#[derive(Default)]
pub(super) struct MainWindowStateController {
    latest: Option<MainWindowState>,
    latest_generation: u64,
    persisted_generation: u64,
    timer_armed: bool,
}

impl MainWindowStateController {
    pub(super) fn record(&mut self, state: MainWindowState) -> u64 {
        if self.latest.as_ref() == Some(&state) {
            return self.latest_generation;
        }
        self.latest_generation = self.latest_generation.saturating_add(1);
        self.latest = Some(state);
        self.latest_generation
    }

    pub(super) fn arm_timer(&mut self) -> bool {
        if self.timer_armed || !self.is_dirty() {
            return false;
        }
        self.timer_armed = true;
        true
    }

    pub(super) fn take_debounced_save(&mut self) -> Option<(u64, MainWindowState)> {
        self.timer_armed = false;
        self.latest
            .clone()
            .map(|state| (self.latest_generation, state))
            .filter(|(generation, _)| *generation > self.persisted_generation)
    }

    pub(super) fn complete_save(&mut self, generation: u64, succeeded: bool) {
        if succeeded {
            self.persisted_generation = self.persisted_generation.max(generation);
        }
    }

    pub(super) fn latest_for_shutdown(&self) -> Option<MainWindowState> {
        self.latest.clone()
    }

    fn is_dirty(&self) -> bool {
        self.latest.is_some() && self.latest_generation > self.persisted_generation
    }
}

pub(super) fn capture_main_window_state(
    window: &gpui::Window,
    cx: &gpui::App,
) -> Option<MainWindowState> {
    let display_uuid = window.display(cx).and_then(|display| display.uuid().ok());
    main_window_state_from_bounds(window.inner_window_bounds(), display_uuid)
}

fn main_window_state_from_bounds(
    bounds: WindowBounds,
    display_uuid: Option<uuid::Uuid>,
) -> Option<MainWindowState> {
    let (bounds, maximized) = match bounds {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) => (bounds, true),
        WindowBounds::Fullscreen(bounds) => (bounds, false),
    };
    let state = MainWindowState::new(
        display_uuid,
        nyaterm_core::MainWindowBounds {
            x: f32::from(bounds.origin.x).round() as i32,
            y: f32::from(bounds.origin.y).round() as i32,
            width: f32::from(bounds.size.width).round() as i32,
            height: f32::from(bounds.size.height).round() as i32,
        },
        maximized,
    );
    state.validate().is_ok().then_some(state)
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, DisplayId, WindowBounds, point, px, size};
    use nyaterm_core::{MainWindowBounds, MainWindowState};

    use super::{
        DisplayGeometry, MainWindowStateController, clamp_window_bounds, intersection_area,
        main_window_state_from_bounds, resolve_saved_window_placement,
    };

    fn display(id: u64, x: f32, y: f32, width: f32, height: f32) -> DisplayGeometry {
        DisplayGeometry {
            id: DisplayId::new(id),
            uuid: Some(format!("display-{id}")),
            visible_bounds: Bounds {
                origin: point(px(x), px(y)),
                size: size(px(width), px(height)),
            },
        }
    }

    #[test]
    fn clamp_preserves_visible_negative_monitor_coordinates() {
        let saved = Bounds {
            origin: point(px(-1400.), px(100.)),
            size: size(px(1200.), px(800.)),
        };
        let visible = display(1, -1920., 0., 1920., 1080.).visible_bounds;
        assert_eq!(clamp_window_bounds(saved, visible, true), saved);
    }

    #[test]
    fn clamp_shrinks_and_moves_offscreen_bounds_into_visible_area() {
        let saved = Bounds {
            origin: point(px(2200.), px(900.)),
            size: size(px(2400.), px(1400.)),
        };
        let visible = display(1, 0., 0., 1920., 1040.).visible_bounds;
        assert_eq!(
            clamp_window_bounds(saved, visible, true),
            Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1920.), px(1040.)),
            }
        );
    }

    #[test]
    fn missing_display_fallback_centers_the_saved_size() {
        let saved = Bounds {
            origin: point(px(2400.), px(100.)),
            size: size(px(1200.), px(800.)),
        };
        let visible = display(1, 0., 0., 1920., 1040.).visible_bounds;
        assert_eq!(
            clamp_window_bounds(saved, visible, false).origin,
            point(px(360.), px(120.))
        );
    }

    #[test]
    fn intersection_area_selects_the_monitor_containing_most_of_the_window() {
        let saved = Bounds {
            origin: point(px(1700.), px(100.)),
            size: size(px(800.), px(600.)),
        };
        let first = display(1, 0., 0., 1920., 1080.);
        let second = display(2, 1920., 0., 1920., 1080.);
        assert!(
            intersection_area(saved, second.visible_bounds)
                > intersection_area(saved, first.visible_bounds)
        );
    }

    #[test]
    fn state_shape_supports_maximized_restore_bounds() {
        let state = MainWindowState::new(
            None,
            MainWindowBounds {
                x: 40,
                y: 60,
                width: 1280,
                height: 800,
            },
            true,
        );
        let bounds = Bounds {
            origin: point(
                px(state.restore_bounds.x as f32),
                px(state.restore_bounds.y as f32),
            ),
            size: size(
                px(state.restore_bounds.width as f32),
                px(state.restore_bounds.height as f32),
            ),
        };
        assert!(matches!(
            WindowBounds::Maximized(bounds),
            WindowBounds::Maximized(_)
        ));
    }

    #[test]
    fn saved_display_uuid_restores_on_that_display_and_preserves_maximized() {
        let uuid = uuid::Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("uuid");
        let primary = display(1, 0., 0., 1920., 1080.);
        let mut secondary = display(2, 1920., 0., 1920., 1080.);
        secondary.uuid = Some(uuid.to_string());
        let state = MainWindowState::new(
            Some(uuid),
            MainWindowBounds {
                x: 2100,
                y: 120,
                width: 1200,
                height: 800,
            },
            true,
        );

        let placement = resolve_saved_window_placement(
            Some(&state),
            &[primary, secondary],
            Some(DisplayId::new(1)),
        )
        .expect("placement");

        assert_eq!(placement.display_id, Some(DisplayId::new(2)));
        assert!(matches!(
            placement.window_bounds,
            WindowBounds::Maximized(_)
        ));
        assert_eq!(
            placement.window_bounds.get_bounds().origin,
            point(px(2100.), px(120.))
        );
    }

    #[test]
    fn removed_saved_display_centers_restore_bounds_on_primary() {
        let missing = uuid::Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("uuid");
        let primary = display(1, 0., 0., 1920., 1040.);
        let state = MainWindowState::new(
            Some(missing),
            MainWindowBounds {
                x: 2200,
                y: 50,
                width: 1200,
                height: 800,
            },
            false,
        );

        let placement =
            resolve_saved_window_placement(Some(&state), &[primary], Some(DisplayId::new(1)))
                .expect("placement");

        assert_eq!(placement.display_id, Some(DisplayId::new(1)));
        assert_eq!(
            placement.window_bounds.get_bounds().origin,
            point(px(360.), px(120.))
        );
    }

    #[test]
    fn controller_coalesces_changes_and_stale_completion_keeps_newer_state_dirty() {
        let first = MainWindowState::new(
            None,
            MainWindowBounds {
                x: 0,
                y: 0,
                width: 1000,
                height: 700,
            },
            false,
        );
        let second = MainWindowState::new(
            None,
            MainWindowBounds {
                x: 40,
                y: 60,
                width: 1200,
                height: 800,
            },
            false,
        );
        let mut controller = MainWindowStateController::default();
        let first_generation = controller.record(first);
        assert!(controller.arm_timer());
        assert!(!controller.arm_timer());
        let second_generation = controller.record(second.clone());
        let (saved_generation, saved) = controller.take_debounced_save().expect("save");
        assert_eq!(saved_generation, second_generation);
        assert_eq!(saved, second);

        let third = MainWindowState::new(
            None,
            MainWindowBounds {
                x: 80,
                y: 90,
                width: 1200,
                height: 800,
            },
            false,
        );
        controller.record(third);
        controller.complete_save(first_generation, true);
        assert!(controller.arm_timer());
    }

    #[test]
    fn fullscreen_is_saved_as_windowed_restore_bounds() {
        let bounds = Bounds {
            origin: point(px(40.), px(60.)),
            size: size(px(1280.), px(800.)),
        };
        let fullscreen = main_window_state_from_bounds(WindowBounds::Fullscreen(bounds), None)
            .expect("fullscreen restore state");
        let maximized = main_window_state_from_bounds(WindowBounds::Maximized(bounds), None)
            .expect("maximized restore state");

        assert!(!fullscreen.maximized);
        assert!(maximized.maximized);
        assert_eq!(fullscreen.restore_bounds.x, 40);
        assert_eq!(fullscreen.restore_bounds.height, 800);
    }
}
