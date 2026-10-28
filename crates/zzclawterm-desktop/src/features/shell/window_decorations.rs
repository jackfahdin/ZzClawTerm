use gpui::Context;
use zzclawterm_core::models::window_decoration::WindowDecoration;

use crate::features::ZzClawTermApp;
use crate::window_chrome::request_window_decoration_refresh;

impl ZzClawTermApp {
    pub(crate) fn requires_client_decorations(&self, handle: gpui::AnyWindowHandle) -> bool {
        self.transfer.owns_external_sync_window(handle)
    }

    pub(crate) fn window_decoration_preview(&self) -> Option<WindowDecoration> {
        self.shell
            .has_settings_draft()
            .then(|| self.settings.summary().window_decorations.clone())
    }

    pub(in crate::features) fn set_window_decorations(
        &mut self,
        decorations: WindowDecoration,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.set_window_decorations(decorations) {
            return;
        }
        request_window_decoration_refresh(cx);
        self.save_appearance_settings(cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Context, IntoElement, Render, TestAppContext, Window, WindowDecorations,
        div, px, size,
    };
    use zzclawterm_core::models::window_decoration::WindowDecoration;
    use zzclawterm_core::{AppRuntime, RuntimeMode, WorkspaceId, test_support::TestTempDir};

    use crate::app_shell::{
        AppShellStartup, DesktopController, DesktopControllerGlobal, GlobalStateMutation,
    };
    use crate::features::test_support::app_with_visible_local_session;
    use crate::window_chrome::{request_window_decoration_refresh, requested_window_decorations};

    struct EmptyWindow;

    impl Render for EmptyWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[test]
    fn all_windows_resolve_the_settings_owners_preview_and_cancel_restores_committed_state() {
        let root = TestTempDir::new("zzclawterm-window-decoration-controller");
        let app_root = TestTempDir::new("zzclawterm-window-decoration-app");
        let second_root = TestTempDir::new("zzclawterm-window-decoration-second");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.path().into(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let startup = AppShellStartup::prepare(&runtime);
        let controller = cx.new(|cx| DesktopController::new(runtime, startup, cx));
        let owner = app_with_visible_local_session(&mut cx, app_root.path(), "first");
        let other = app_with_visible_local_session(&mut cx, second_root.path(), "second");
        let process_state = cx.read(|cx| owner.read(cx).process_state.clone());
        let mut shells = Vec::new();
        let mut handles = Vec::new();
        let owner_id = WorkspaceId::new();
        for (id, app) in [
            (owner_id, owner.clone()),
            (WorkspaceId::new(), other.clone()),
        ] {
            let handle = cx.open_window(size(px(800.), px(600.)), |_, _| EmptyWindow);
            let shell = controller.update(&mut cx, |controller, cx| {
                controller.register_tab_drag_test_workspace(id, app, handle.into(), cx)
            });
            handles.push(handle);
            shells.push(shell);
        }
        controller.update(&mut cx, |controller, cx| {
            controller.set_decoration_test_process_state(process_state.clone());
            assert!(!controller.activate_or_claim_settings(owner_id, cx));
        });
        cx.update(|cx| cx.set_global(DesktopControllerGlobal(controller.clone())));
        owner.update(&mut cx, |app, cx| {
            app.begin_settings_draft(cx);
            app.set_window_decorations(WindowDecoration::Server, cx);
        });
        // Opening a child during the preview reads the same process preference.
        let child = cx.update(|cx| {
            let spec = crate::features::view_widgets::ChildWindowSpec::document(
                "Child".into(),
                400.,
                300.,
            );
            let options = crate::features::view_widgets::child_window_options(&spec, None, cx);
            #[cfg(target_os = "linux")]
            assert_eq!(options.window_decorations, Some(WindowDecorations::Server));
            #[cfg(not(target_os = "linux"))]
            assert!(options.window_decorations.is_none());
            let prompt = crate::features::view_widgets::ChildWindowSpec::topmost_prompt(
                "Prompt".into(),
                400.,
                300.,
            );
            let prompt_options =
                crate::features::view_widgets::child_window_options(&prompt, None, cx);
            #[cfg(target_os = "linux")]
            assert_eq!(
                prompt_options.window_decorations,
                Some(WindowDecorations::Client)
            );
            #[cfg(not(target_os = "linux"))]
            assert!(prompt_options.window_decorations.is_none());
            cx.open_window(options, |_, cx| cx.new(|_| EmptyWindow))
                .unwrap()
        });
        handles.push(child);
        for handle in &handles {
            handle
                .update(&mut cx, |_, _, cx| {
                    assert_eq!(requested_window_decorations(cx), WindowDecorations::Server);
                })
                .unwrap();
        }
        // A different workspace's stale committed projection cannot undo the preview.
        other.update(&mut cx, |app, cx| {
            app.apply_gpui_settings(app.settings.summary().clone(), cx);
        });
        owner.update(&mut cx, |app, cx| {
            app.set_window_decorations(WindowDecoration::Client, cx);
            app.set_window_decorations(WindowDecoration::Server, cx);
        });
        cx.update(|cx| {
            request_window_decoration_refresh(cx);
            cx.defer(|cx| {
                assert_eq!(requested_window_decorations(cx), WindowDecorations::Server);
            });
        });
        cx.run_until_parked();
        owner.update(&mut cx, |app, cx| app.cancel_settings(cx));
        cx.update(|cx| {
            assert_eq!(requested_window_decorations(cx), WindowDecorations::Client);
        });
        // A restored committed value applies when the draft no longer exists.
        process_state.update(&mut cx, |state, cx| {
            let mut settings = state.snapshot().settings.clone();
            settings.window_decorations = WindowDecoration::Server;
            state.mutate(GlobalStateMutation::UpdateSettings(Box::new(settings)), cx);
        });
        cx.update(|cx| {
            assert_eq!(requested_window_decorations(cx), WindowDecorations::Server);
        });
        process_state.update(&mut cx, |state, cx| {
            let mut settings = state.snapshot().settings.clone();
            settings.window_decorations = WindowDecoration::Unknown("future".into());
            state.mutate(GlobalStateMutation::UpdateSettings(Box::new(settings)), cx);
        });
        cx.update(|cx| {
            assert_eq!(requested_window_decorations(cx), WindowDecorations::Client);
        });
        controller.update(&mut cx, |controller, cx| {
            controller.release_settings_owner(owner_id, cx)
        });
        drop(shells);
        cx.run_until_parked();
    }
}
