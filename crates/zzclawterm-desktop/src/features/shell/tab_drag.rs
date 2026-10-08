use gpui::{
    AnyElement, Context, IntoElement as _, KeyDownEvent, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, Styled as _, Window, canvas,
};

use super::tab_mouse::SessionTabDragPayload;
use crate::app_shell::tab_drag::{TabDragSource, release_at, screen_bounds};
use crate::features::ZzClawTermApp;

#[cfg(test)]
mod tests;

impl ZzClawTermApp {
    pub(in crate::features) fn session_tab_drag_listener_layer(
        &self,
        cx: &Context<Self>,
    ) -> AnyElement {
        let app = cx.entity().downgrade();
        // Window-level listeners must be registered during paint, not render.
        // The canvas has no hitbox and contributes no pixels or flow layout.
        canvas(
            |_, _, _| (),
            move |_, (), window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.install_session_tab_drag_listeners(window, cx)
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full()
        .into_any_element()
    }

    pub(in crate::features) fn begin_session_tab_drag(
        &mut self,
        payload: &SessionTabDragPayload,
        grab_offset: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // The new workspace has one strip tab. Include the strip's offset in
        // the window so its first tab, rather than the window corner, follows
        // the grab point (including the sidebar and title-bar space).
        let grab_offset = self.shell.session_tab_strip_scroll().bounds().origin + grab_offset;
        if let Some(controller) = &self.desktop_controller {
            let _ = controller.update(cx, |controller, _| {
                controller.update_tab_drag(|drag| {
                    drag.begin(TabDragSource {
                        workspace_id: payload.source_workspace_id,
                        root_tab_id: payload.root_tab_id.clone(),
                        revision: payload.source_revision,
                        grab_offset,
                        screen_bounds: screen_bounds(window),
                        normal_size: drag.normal_size(payload.source_workspace_id),
                    });
                });
            });
        }
        self.update_session_tab_drag(payload.session_id.clone(), cx);
    }

    pub(in crate::features) fn accept_session_tab_drop(
        &self,
        payload: &SessionTabDragPayload,
        cx: &mut Context<Self>,
    ) {
        if let Some(controller) = &self.desktop_controller {
            let _ = controller.update(cx, |controller, _| {
                controller.update_tab_drag(|drag| {
                    drag.accept(
                        payload.source_workspace_id,
                        &payload.root_tab_id,
                        payload.source_revision,
                    );
                });
            });
        }
    }

    pub(in crate::features) fn cancel_session_tab_drag_on_escape(
        &self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key != "escape" {
            return false;
        }
        let cancelled = self.desktop_controller.as_ref().is_some_and(|controller| {
            controller
                .update(cx, |controller, _| {
                    controller.update_tab_drag(|drag| drag.cancel())
                })
                .unwrap_or(false)
        });
        if cancelled {
            cx.stop_active_drag(window);
            cx.stop_propagation();
        }
        cancelled
    }

    pub(in crate::features) fn install_session_tab_drag_listeners(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(controller) = self.desktop_controller.clone() else {
            return;
        };
        let workspace_id = self.workspace_id;
        if !window.is_maximized() && !window.is_fullscreen() {
            let _ = controller.update(cx, |controller, _| {
                controller.remember_tab_window_size(workspace_id, window.viewport_size());
            });
        }
        let release_controller = controller.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if !phase.capture() || event.button != MouseButton::Left || !cx.has_active_drag() {
                return;
            }
            let id = release_controller.update(cx, |controller, _| {
                let source = controller.tab_drag_source()?;
                let release = release_at(source, workspace_id, window, event.position);
                controller.update_tab_drag(|drag| drag.released(release))
            });
            if let Ok(Some(id)) = id {
                let controller = release_controller.clone();
                // Drop handlers run in the bubble phase. GPUI then clears its
                // generic drag before this callback evaluates our candidate.
                cx.defer(move |cx| {
                    let _ = controller.update(cx, |controller, cx| {
                        controller.finish_tab_drag_release(id, cx)
                    });
                });
            }
        });
        window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
            if phase.capture() {
                // An interrupted drag must not turn a subsequent, unrelated
                // drag (files, connections, selection) into a tab detach.
                let _ = controller.update(cx, |controller, _| {
                    controller.update_tab_drag(|drag| drag.cancel())
                });
            }
        });
    }
}
