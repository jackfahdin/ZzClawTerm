use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    Window, div, point, prelude::*, px,
};
use zzclawterm_core::{
    AppRuntime, RuntimeMode, StartWorkspaceMode, WorkspaceId, test_support::TestTempDir,
};

use crate::app_shell::{AppShellStartup, DesktopController};
use crate::features::{
    ZzClawTermApp,
    shell::{SessionTabDragPayload, SessionTabDragPreview},
    test_support::app_with_visible_local_session,
};

struct DragFixture {
    app: Entity<ZzClawTermApp>,
    controller: Entity<DesktopController>,
    is_target: bool,
    release_seen: Rc<Cell<Option<bool>>>,
}

impl Render for DragFixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let payload = self.app.update(cx, |app, cx| {
            window.focus(app.terminal.input_focus(), cx);
            SessionTabDragPayload {
                source_workspace_id: app.workspace_id,
                root_tab_id: "tab".into(),
                source_revision: app.workspace_revision(),
                session_id: "tab".into(),
                order_index: 0,
                source_group_id: None,
                display_name: "Test".into(),
                kind_label: "Local",
                kind_icon: "icons/terminal.svg",
                preview_background: 0,
                preview_border: 0,
                preview_text: 0,
                preview_text_muted: 0,
                preview_accent: 0,
            }
        });
        let drag_app = self.app.clone();
        let focus = self.app.read(cx).terminal.input_focus().clone();
        let key_app = self.app.clone();
        let layer = self
            .app
            .update(cx, |app, cx| app.session_tab_drag_listener_layer(cx));
        let mut root = div()
            .id("drag-fixture")
            .track_focus(&focus)
            .size_full()
            .child(layer)
            .capture_key_down(move |event, window, cx| {
                key_app.update(cx, |app, cx| {
                    app.cancel_session_tab_drag_on_escape(event, window, cx);
                });
            });
        if self.is_target {
            let drop_app = self.app.clone();
            let controller = self.controller.clone();
            let release_seen = self.release_seen.clone();
            root = root.on_drop(move |payload: &SessionTabDragPayload, _, cx| {
                release_seen.set(controller.update(cx, |controller, _| {
                    controller.update_tab_drag(|drag| {
                        drag.pending_release().map(|release| release.outside_source)
                    })
                }));
                drop_app.update(cx, |app, cx| app.accept_session_tab_drop(payload, cx));
            });
        }
        root.child(div().id("drag-source").w(px(100.)).h(px(36.)).on_drag(
            payload,
            move |payload, offset, window, cx| {
                drag_app.update(cx, |app, cx| {
                    app.begin_session_tab_drag(payload, offset, window, cx)
                });
                cx.new(|_| SessionTabDragPreview::new(payload.clone(), offset))
            },
        ))
        .child(
            div()
                .id("unrelated-drag")
                .w(px(100.))
                .h(px(36.))
                .on_drag(123_u32, |_, _, _, cx| cx.new(|_| EmptyPreview)),
        )
    }
}

struct EmptyPreview;
impl Render for EmptyPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn controller(root: &TestTempDir, cx: &mut TestAppContext) -> Entity<DesktopController> {
    let runtime = AppRuntime::from_parts_for_test(
        RuntimeMode::Portable,
        root.path().into(),
        root.path().join("config"),
        root.path().join("logs"),
        root.path().join("cache"),
        None,
    );
    let startup = AppShellStartup::prepare(&runtime);
    cx.new(|cx| DesktopController::new(runtime, startup, cx))
}

#[test]
fn tab_drag_escape_cancels_before_release_and_unrelated_drags_stay_unrelated() {
    let root = TestTempDir::new("zzclawterm-tab-drag-escape");
    let mut cx = TestAppContext::single();
    let controller = controller(&root, &mut cx);
    let app_root = TestTempDir::new("zzclawterm-tab-drag-escape-app");
    let app = app_with_visible_local_session(&mut cx, app_root.path(), "tab");
    app.update(&mut cx, |app, _| {
        app.desktop_controller = Some(controller.downgrade())
    });
    let fixture_controller = controller.clone();
    let (_, cx) = cx.add_window_view(move |_, _| DragFixture {
        app,
        controller: fixture_controller,
        is_target: false,
        release_seen: Rc::new(Cell::new(None)),
    });
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_mouse_down(
        point(px(20.), px(16.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    // Starting with the first move already outside the viewport exercises the
    // on_drag path without ever hovering another tab.
    cx.simulate_mouse_move(
        point(px(-30.), px(16.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.read(|cx| assert!(controller.read(cx).tab_drag_source().is_some()));
    cx.simulate_keystrokes("escape");
    cx.read(|cx| {
        assert!(controller.read(cx).tab_drag_source().is_none());
        assert!(!cx.has_active_drag());
    });
    cx.simulate_mouse_up(
        point(px(-30.), px(16.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_down(
        point(px(20.), px(50.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(-30.), px(50.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.read(|cx| {
        assert!(cx.has_active_drag());
        assert!(controller.read(cx).tab_drag_source().is_none());
    });
    cx.simulate_mouse_up(
        point(px(-30.), px(50.)),
        MouseButton::Left,
        Modifiers::default(),
    );
}

#[test]
fn tab_drag_target_drop_observes_release_candidate_then_prevents_detach() {
    let root = TestTempDir::new("zzclawterm-tab-drag-drop-order");
    let mut cx = TestAppContext::single();
    let controller = controller(&root, &mut cx);
    let source_root = TestTempDir::new("zzclawterm-tab-drag-source");
    let target_root = TestTempDir::new("zzclawterm-tab-drag-target");
    let source = app_with_visible_local_session(&mut cx, source_root.path(), "tab");
    let target = app_with_visible_local_session(&mut cx, target_root.path(), "existing");
    target.update(&mut cx, |app, _| app.workspace_id = WorkspaceId::new());
    for app in [&source, &target] {
        app.update(&mut cx, |app, _| {
            app.desktop_controller = Some(controller.downgrade())
        });
    }
    let first_controller = controller.clone();
    let (source_window, source_cx) = cx.add_window_view(move |_, _| DragFixture {
        app: source,
        controller: first_controller,
        is_target: false,
        release_seen: Rc::new(Cell::new(None)),
    });
    source_cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    source_cx.simulate_mouse_down(
        point(px(20.), px(16.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    source_cx.simulate_mouse_move(
        point(px(40.), px(16.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    source_cx.read(|cx| assert!(controller.read(cx).tab_drag_source().is_some()));
    let release_seen = Rc::new(Cell::new(None));
    let observed = release_seen.clone();
    let second_controller = controller.clone();
    let (_, target_cx) = cx.add_window_view(move |_, _| DragFixture {
        app: target,
        controller: second_controller,
        is_target: true,
        release_seen: observed,
    });
    target_cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    target_cx.simulate_mouse_move(
        point(px(200.), px(100.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    target_cx.simulate_mouse_up(
        point(px(200.), px(100.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    assert_eq!(release_seen.get(), Some(true));
    target_cx.run_until_parked();
    target_cx.read(|cx| {
        assert!(!cx.has_active_drag());
        assert!(controller.read(cx).tab_drag_source().is_none());
    });
    let _ = source_window;
}

struct WorkspaceDragFixture {
    app: Entity<ZzClawTermApp>,
}

impl Render for WorkspaceDragFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(app.session_tab_drag_listener_layer(cx))
                .child(app.workspace_view(cx).into_any_element())
        })
    }
}

#[test]
fn tab_drag_into_empty_workspace_moves_session_without_opening_another_window() {
    for mode in [StartWorkspaceMode::Workbench, StartWorkspaceMode::Assets] {
        let root = TestTempDir::new("zzclawterm-empty-workspace-tab-drop");
        let source_root = TestTempDir::new("zzclawterm-empty-workspace-tab-source");
        let target_root = TestTempDir::new("zzclawterm-empty-workspace-tab-target");
        let mut cx = TestAppContext::single();
        let controller = controller(&root, &mut cx);
        let source = app_with_visible_local_session(&mut cx, source_root.path(), "tab");
        let target = app_with_visible_local_session(&mut cx, target_root.path(), "placeholder");
        target.update(&mut cx, |app, _| {
            app.session.remove_session_catalog("placeholder");
            app.workspace_id = WorkspaceId::new();
            app.start_workspace.set_mode(mode);
            assert!(!app.has_live_sessions());
        });
        source.update(&mut cx, |app, _| {
            app.terminal
                .seed_session_view("tab".into(), "retained contents".into(), "UTF-8");
        });
        // Keep the shells alive as the controller's authoritative app lookup;
        // the fixture windows render only the real workspace and drag listeners.
        let mut shells = Vec::new();
        let mut windows = Vec::new();
        for app in [&source, &target] {
            app.update(&mut cx, |app, cx| {
                app.desktop_controller = Some(controller.downgrade());
                app.sync_component_theme(cx);
            });
            let workspace_id = app.read_with(&cx, |app, _| app.workspace_id);
            let fixture_app = app.clone();
            let (_, visual_cx) = cx.add_window_view(move |_, cx| {
                cx.observe(&fixture_app, |_, _, cx| cx.notify()).detach();
                WorkspaceDragFixture { app: fixture_app }
            });
            let (shell, handle) = visual_cx.update(|window, cx| {
                let handle = window.window_handle();
                let shell = controller.update(cx, |controller, cx| {
                    controller.register_tab_drag_test_workspace(
                        workspace_id,
                        app.clone(),
                        handle,
                        cx,
                    )
                });
                (shell, handle)
            });
            visual_cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            shells.push(shell);
            windows.push(gpui::VisualTestContext::from_window(handle, &visual_cx.cx));
        }
        let source_cx = &mut windows[0];
        source_cx.simulate_mouse_down(
            point(px(40.), px(16.)),
            MouseButton::Left,
            Default::default(),
        );
        source_cx.simulate_mouse_move(
            point(px(80.), px(16.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        source_cx.read(|cx| assert!(controller.read(cx).tab_drag_source().is_some()));
        let target_cx = &mut windows[1];
        target_cx.simulate_mouse_move(
            point(px(200.), px(100.)),
            Some(MouseButton::Left),
            Default::default(),
        );
        target_cx.simulate_mouse_up(
            point(px(200.), px(100.)),
            MouseButton::Left,
            Default::default(),
        );
        target_cx.run_until_parked();
        target_cx.read(|cx| {
            assert!(controller.read(cx).tab_drag_source().is_none());
            assert_eq!(controller.read(cx).workspace_count(), 2);
            assert_eq!(cx.windows().len(), 2);
            assert!(!source.read(cx).has_live_sessions());
            let target = target.read(cx);
            assert_eq!(target.live_session_count(), 1);
            assert_eq!(target.session.active_id(), Some("tab"));
            assert_eq!(
                target.terminal.session_output("tab"),
                Some("retained contents")
            );
        });
    }
}
