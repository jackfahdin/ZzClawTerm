use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, ParentElement as _, Render, Styled as _, TestAppContext, Window,
    div, point, px, size,
};
use zzclawterm_core::{FuzzyResult, get_tracked_command};

use super::CommandSuggestionSearchRequest;
use crate::features::ZzClawTermApp;
use crate::features::test_support::app_with_visible_local_session;
use crate::models::{CommandSuggestionItem, CommandSuggestionState};
use crate::test_support::{TestAppCleanup, TestConfigDir};

struct SuggestionHost {
    app: Entity<ZzClawTermApp>,
    other_focus: FocusHandle,
    right_clicks: usize,
    left_clicks: usize,
}

impl Render for SuggestionHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay = self.app.update(cx, |app, cx| {
            app.ensure_terminal_focus_reporting(window, cx);
            app.command_suggestions_overlay(cx).into_any_element()
        });
        div()
            .size_full()
            .relative()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.left_clicks += 1;
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, _| {
                    this.right_clicks += 1;
                }),
            )
            .child(div().track_focus(self.app.read(cx).terminal.input_focus()))
            .child(div().track_focus(&self.other_focus))
            .child(overlay)
    }
}

fn open_suggestions(
    app: &Entity<ZzClawTermApp>,
    cx: &mut TestAppContext,
) -> CommandSuggestionSearchRequest {
    cx.update_entity(app, |app, cx| {
        app.sync_component_theme(cx);
        app.terminal.layout.session_surface_bounds.insert(
            "s1".into(),
            Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.))),
        );
        app.note_shell_editing_input(b"echo", cx);
        let request = app
            .prepare_command_suggestion_search(
                app.terminal.assist.command_suggestion_search_gen,
                cx,
            )
            .expect("typing can start a suggestion search");
        app.terminal.assist.command_suggestions = Some(CommandSuggestionState {
            session_id: "s1".into(),
            draft: "echo".into(),
            items: vec![CommandSuggestionItem {
                command: "echo example".into(),
                display: "echo example".into(),
                source: "history".into(),
                score: 1,
                indices: Vec::new(),
            }],
            selected_index: None,
            cursor_row: 0,
            cursor_col: 0,
        });
        request
    })
}

fn assert_dismissed_without_losing_input(
    app: &mut ZzClawTermApp,
    request: CommandSuggestionSearchRequest,
    cx: &mut Context<ZzClawTermApp>,
) {
    assert!(!app.terminal.command_suggestions_open());
    assert_eq!(
        get_tracked_command(app.terminal.editing.assist_input()),
        "echo"
    );
    app.publish_command_suggestion_search(
        request,
        vec![FuzzyResult {
            command: "echo example".into(),
            display: "echo example".into(),
            source: "history".into(),
            score: 1,
            indices: Vec::new(),
        }],
        Duration::ZERO,
        cx,
    );
    assert!(
        !app.terminal.command_suggestions_open(),
        "late results must stay dismissed"
    );
}

#[test]
fn left_click_outside_dismisses_suggestions_without_blurring_terminal() {
    let dir = TestConfigDir::new("zzclawterm-suggestion-click-dismiss");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, dir.path(), "s1");
    let request = open_suggestions(&app, &mut cx);
    let (host, cx) = cx.add_window_view({
        let app = app.clone();
        move |_, cx| SuggestionHost {
            app,
            other_focus: cx.focus_handle(),
            right_clicks: 0,
            left_clicks: 0,
        }
    });
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        window.activate_window();
        let focus = app.read(cx).terminal.input_focus().clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert!(app.read_with(cx, |app, _| app.terminal.input.focus_active));
    let inside = app.read_with(cx, |app, _| {
        let placement = app
            .suggestion_overlay_position_for_session(Some("s1"), 0, 0, 380., 76.)
            .unwrap();
        point(px(placement.x + 10.), px(placement.y + 10.))
    });
    cx.simulate_event(MouseDownEvent {
        position: inside,
        button: MouseButton::Left,
        ..Default::default()
    });
    assert!(app.read_with(cx, |app, _| app.terminal.command_suggestions_open()));
    let before = host.read_with(cx, |host, _| host.right_clicks);
    cx.simulate_event(MouseDownEvent {
        position: point(px(700.), px(500.)),
        button: MouseButton::Right,
        ..Default::default()
    });
    assert_eq!(host.read_with(cx, |host, _| host.right_clicks), before + 1);
    assert!(
        app.read_with(cx, |app, _| app.terminal.command_suggestions_open()),
        "right-click outside must not dismiss suggestions"
    );
    let before = host.read_with(cx, |host, _| host.left_clicks);
    cx.simulate_event(MouseDownEvent {
        position: point(px(700.), px(500.)),
        button: MouseButton::Left,
        ..Default::default()
    });
    assert_eq!(host.read_with(cx, |host, _| host.left_clicks), before + 1);
    assert!(cx.update(|window, cx| app.read(cx).terminal.input_focus().is_focused(window)));
    app.update(cx, |app, cx| {
        assert_dismissed_without_losing_input(app, request, cx)
    });
}

#[test]
fn terminal_focus_loss_dismisses_suggestions_and_late_results() {
    let dir = TestConfigDir::new("zzclawterm-suggestion-blur-dismiss");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let app = app_with_visible_local_session(&mut cx, dir.path(), "s1");
    let request = open_suggestions(&app, &mut cx);
    let (host, cx) = cx.add_window_view({
        let app = app.clone();
        move |_, cx| SuggestionHost {
            app,
            other_focus: cx.focus_handle(),
            right_clicks: 0,
            left_clicks: 0,
        }
    });
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        window.activate_window();
        let focus = app.read(cx).terminal.input_focus().clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert!(app.read_with(cx, |app, _| app.terminal.input.focus_active));
    assert!(app.read_with(cx, |app, _| app.terminal.command_suggestions_open()));
    cx.update(|window, cx| window.focus(&host.read(cx).other_focus.clone(), cx));
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert!(!app.read_with(cx, |app, _| app.terminal.input.focus_active));
    app.update(cx, |app, cx| {
        assert_dismissed_without_losing_input(app, request, cx)
    });
}
