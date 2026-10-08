use std::time::{Duration, Instant};

use gpui::{
    Context, Entity, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};
use zzclawterm_core::test_support::TestTempDir;
use zzclawterm_store::ConnectionStore;

use crate::features::{ZzClawTermApp, test_support::app_with_visible_local_session};
use crate::models::SecurityAuthTab;

struct SecurityListFixture {
    app: Entity<ZzClawTermApp>,
    width: f32,
}

impl Render for SecurityListFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.app.update(cx, |app, cx| {
            let palette = app.theme_palette();
            match app.security.auth_tab() {
                SecurityAuthTab::Passwords => app.security_passwords_body(palette, cx),
                SecurityAuthTab::Keys => app.security_keys_body(palette, cx),
                SecurityAuthTab::Credentials => app.security_credentials_body(palette, cx),
                _ => unreachable!(),
            }
        });
        div()
            .w(px(self.width))
            .h(px(500.))
            .flex()
            .flex_col()
            .child(body)
    }
}

fn hosted<'a>(
    cx: &'a mut TestAppContext,
    root: &TestTempDir,
    tab: SecurityAuthTab,
    width: f32,
) -> (Entity<ZzClawTermApp>, &'a mut VisualTestContext) {
    let store = ConnectionStore::open(root.join("config")).unwrap();
    for (index, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
        let raw = serde_json::json!({
            "id":id, "name":id, "username":"user", "sort_order":index as i32
        });
        store
            .save_password(serde_json::from_value(raw.clone()).unwrap())
            .unwrap();
        store
            .save_ssh_key(serde_json::from_value(raw.clone()).unwrap())
            .unwrap();
        store
            .save_credential(serde_json::from_value(raw).unwrap())
            .unwrap();
    }
    let keys = store.list_ssh_keys().unwrap();
    let passwords = store.list_passwords().unwrap();
    let credentials = store.list_credentials().unwrap();
    drop(store);
    let app = app_with_visible_local_session(cx, root.path(), "security-test");
    app.update(cx, |app, cx| {
        app.sync_component_theme(cx);
        app.shell.set_left_panel_width_for_test(width);
        assert_eq!(app.shell.left_panel_width(), width);
        app.security
            .replace_catalog(keys, Vec::new(), passwords, credentials);
        app.security.set_auth_tab(tab);
    });
    let fixture_app = app.clone();
    let (_, vcx) = cx.add_window_view(move |_, _| SecurityListFixture {
        app: fixture_app,
        width,
    });
    draw(vcx);
    (app, vcx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

fn drag_over_second_row(
    cx: &mut VisualTestContext,
    tab: SecurityAuthTab,
) -> gpui::Point<gpui::Pixels> {
    let (source_id, target_id) = match tab {
        SecurityAuthTab::Passwords => ("security-drag-Pwd-a", "security-row-Pwd-b"),
        SecurityAuthTab::Keys => ("security-drag-Keys-a", "security-row-Keys-b"),
        SecurityAuthTab::Credentials => ("security-drag-Cred-a", "security-row-Cred-b"),
        _ => unreachable!(),
    };
    let source = cx.debug_bounds(source_id).unwrap().center();
    let bounds = cx.debug_bounds(target_id).unwrap();
    let target = point(source.x + px(20.), bounds.top() + bounds.size.height * 0.75);
    cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        source + point(px(8.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    target
}

#[test]
fn all_security_lists_drag_and_save_in_regular_and_compact_layouts() {
    for tab in [
        SecurityAuthTab::Passwords,
        SecurityAuthTab::Keys,
        SecurityAuthTab::Credentials,
    ] {
        for width in [180., 320.] {
            let root = TestTempDir::new("zzclawterm-security-drag");
            let mut cx = TestAppContext::single();
            let (app, cx) = hosted(&mut cx, &root, tab, width);
            let target = drag_over_second_row(cx, tab);
            cx.read(|cx| {
                assert!(cx.has_active_drag());
                let target = app.read(cx).security.drop_target().unwrap();
                assert_eq!(target.tab, tab);
                assert_eq!(target.id, "b");
                assert!(target.after);
            });
            cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if cx.read(|cx| !app.read(cx).security.reorder_busy()) {
                    break;
                }
                assert!(Instant::now() < deadline, "sorting job timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            cx.read(|cx| {
                let security = &app.read(cx).security;
                assert!(security.drop_target().is_none());
                let ids = match tab {
                    SecurityAuthTab::Passwords => security
                        .passwords()
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .collect::<Vec<_>>(),
                    SecurityAuthTab::Keys => security
                        .ssh_keys()
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .collect::<Vec<_>>(),
                    SecurityAuthTab::Credentials => security
                        .credentials()
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .collect::<Vec<_>>(),
                    _ => unreachable!(),
                };
                assert_eq!(ids, vec!["b", "a", "c", "d"]);
            });
        }
    }
}

#[test]
fn cancelled_drags_clear_insertion_marks_and_cross_tab_drops_do_not_sort() {
    let root = TestTempDir::new("zzclawterm-security-drag-cancel");
    let mut cx = TestAppContext::single();
    let (app, cx) = hosted(&mut cx, &root, SecurityAuthTab::Passwords, 320.);
    drag_over_second_row(cx, SecurityAuthTab::Passwords);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(cx.stop_active_drag(window));
    });
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
    cx.read(|cx| assert!(app.read(cx).security.drop_target().is_none()));
    draw(cx);
    let target = drag_over_second_row(cx, SecurityAuthTab::Passwords);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.security.set_auth_tab(SecurityAuthTab::Keys);
            cx.notify();
        });
    });
    draw(cx);
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    cx.read(|cx| {
        let security = &app.read(cx).security;
        assert!(security.drop_target().is_none());
        assert!(!security.reorder_busy());
        assert_eq!(security.ssh_keys()[0].id, "a");
    });
}

#[test]
fn insertion_mark_tracks_only_the_hovered_row_and_clears_on_source_or_outside() {
    for tab in [
        SecurityAuthTab::Passwords,
        SecurityAuthTab::Keys,
        SecurityAuthTab::Credentials,
    ] {
        for width in [180., 320.] {
            let root = TestTempDir::new("zzclawterm-security-drag-hover");
            let mut cx = TestAppContext::single();
            let (app, cx) = hosted(&mut cx, &root, tab, width);
            let (source_id, second_id, third_id, last_id) = match tab {
                SecurityAuthTab::Passwords => (
                    "security-drag-Pwd-a",
                    "security-row-Pwd-b",
                    "security-row-Pwd-c",
                    "security-row-Pwd-d",
                ),
                SecurityAuthTab::Keys => (
                    "security-drag-Keys-a",
                    "security-row-Keys-b",
                    "security-row-Keys-c",
                    "security-row-Keys-d",
                ),
                SecurityAuthTab::Credentials => (
                    "security-drag-Cred-a",
                    "security-row-Cred-b",
                    "security-row-Cred-c",
                    "security-row-Cred-d",
                ),
                _ => unreachable!(),
            };
            let source = cx.debug_bounds(source_id).unwrap().center();
            cx.simulate_mouse_down(source, MouseButton::Left, Modifiers::none());
            let start = source + point(px(8.), px(0.));
            cx.simulate_mouse_move(start, MouseButton::Left, Modifiers::none());
            cx.simulate_mouse_move(
                start + point(px(1.), px(0.)),
                MouseButton::Left,
                Modifiers::none(),
            );
            cx.read(|cx| {
                assert!(cx.has_active_drag());
                assert!(
                    app.read(cx).security.drop_target().is_none(),
                    "starting a drag over its source must not highlight the last row"
                );
            });
            for (selector, id, fraction, after) in [
                (second_id, "b", 0.25, false),
                (second_id, "b", 0.75, true),
                (third_id, "c", 0.25, false),
                (last_id, "d", 0.75, true),
                (second_id, "b", 0.25, false),
            ] {
                draw(cx);
                let bounds = cx.debug_bounds(selector).unwrap();
                let position = point(
                    source.x + px(20.),
                    bounds.top() + bounds.size.height * fraction,
                );
                cx.simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
                cx.read(|cx| {
                    let target = app.read(cx).security.drop_target().unwrap();
                    assert_eq!(target.tab, tab);
                    assert_eq!(target.id, id);
                    assert_eq!(target.after, after);
                });
            }
            cx.simulate_mouse_move(source, MouseButton::Left, Modifiers::none());
            cx.read(|cx| assert!(app.read(cx).security.drop_target().is_none()));
            draw(cx);
            let bounds = cx.debug_bounds(third_id).unwrap();
            let position = point(source.x + px(20.), bounds.center().y);
            cx.simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
            cx.read(|cx| assert_eq!(app.read(cx).security.drop_target().unwrap().id, "c"));
            let outside = point(px(width + 40.), position.y);
            cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
            cx.read(|cx| assert!(app.read(cx).security.drop_target().is_none()));
            cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
            cx.read(|cx| assert!(!app.read(cx).security.reorder_busy()));
        }
    }
}
