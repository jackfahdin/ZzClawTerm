use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, Render, ScrollStrategy, TestAppContext, VisualTestContext, Window,
    div, point, prelude::*, px,
};
use zzclawterm_core::{
    AppRuntime, AssetSortKey, AssetViewMode, Group, RuntimeMode, SavedConnection, SessionsConfig,
    test_support::TestTempDir,
};
use zzclawterm_store::{StoreDomain, store_mutation};
use zzclawterm_ui::{ZzClawDialogWindowExt as _, ZzClawInputEvent, zzclaw_root};

use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
use crate::features::ZzClawTermApp;

use super::{asset_action_left_in_decoration, responsive_asset_card_columns};

struct AssetFixture {
    app: Entity<ZzClawTermApp>,
}

impl Render for AssetFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.app.update(cx, |app, cx| app.asset_workspace_state(cx));
        div()
            .track_focus(self.app.read(cx).terminal.input_focus())
            .w(px(1000.))
            .max_w_full()
            .h(px(700.))
            .flex()
            .flex_col()
            .child(body)
    }
}

fn connection(id: &str) -> SavedConnection {
    // Invalid arguments fail before a PTY or network connection can be started.
    serde_json::from_value(serde_json::json!({
        "id": id, "name": id,
        "type": "local_terminal", "shell_args": "'",
    }))
    .unwrap()
}

fn app(cx: &mut TestAppContext, root: &TestTempDir, mode: AssetViewMode) -> Entity<ZzClawTermApp> {
    let runtime = AppRuntime::from_parts_for_test(
        RuntimeMode::Portable,
        root.path().to_path_buf(),
        root.path().join("config"),
        root.path().join("logs"),
        root.path().join("cache"),
        None,
    );
    let stores = UiStoreHandles {
        startup_restore: cx.new(|_| StartupRestoreStore::default()),
        overlays: cx.new(|_| OverlayStore::default()),
    };
    let app = cx.new(|cx| ZzClawTermApp::new(runtime, stores, cx));
    app.update(cx, |app, cx| {
        app.sync_component_theme(cx);
        app.start_workspace.set_view_mode(mode);
        app.connection_state.replace_loaded(
            vec![
                connection("first"),
                connection("second"),
                connection("third"),
            ],
            vec![Group {
                id: "destination".into(),
                name: "Destination".into(),
                parent_id: None,
                sort_order: 0,
                created_at_ms: None,
                updated_at_ms: None,
            }],
        );
        let ids = vec!["first".into(), "second".into(), "third".into()];
        app.connection_state
            .select_list_connection("first".into(), &ids, false, false);
        app.connection_state
            .select_list_connection("second".into(), &ids, true, false);
    });
    let sessions = app.read_with(cx, |app, _| SessionsConfig {
        connections: app.connection_state.connections().to_vec(),
        groups: app.connection_state.groups().to_vec(),
        ..SessionsConfig::default()
    });
    let store = app.read_with(cx, |app, _| app.store_blocking_client());
    store
        .request(
            0,
            store_mutation(StoreDomain::Connections, move |store| {
                store.replace_sessions(&sessions)
            }),
        )
        .unwrap()
        .outcome
        .unwrap();
    app
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn host<'a>(cx: &'a mut TestAppContext, app: &Entity<ZzClawTermApp>) -> &'a mut VisualTestContext {
    let app = app.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        let fixture = cx.new(|_| AssetFixture { app });
        zzclaw_root(fixture, window, cx)
    });
    draw(cx);
    cx
}

fn click(cx: &mut VisualTestContext, position: Point<Pixels>, button: MouseButton, count: usize) {
    cx.simulate_event(MouseDownEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count: count,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count: count,
    });
    draw(cx);
}

fn item_position(cx: &mut VisualTestContext, mode: AssetViewMode, id: &str) -> Point<Pixels> {
    let kind = if mode == AssetViewMode::List {
        "row"
    } else {
        "card"
    };
    let bounds = cx
        .debug_bounds(Box::leak(format!("asset-{kind}-{id}").into_boxed_str()))
        .unwrap();
    point(bounds.left() + px(80.), bounds.top() + px(30.))
}

fn wait_for(
    cx: &mut VisualTestContext,
    app: &Entity<ZzClawTermApp>,
    predicate: impl Fn(&ZzClawTermApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        draw(cx);
        if app.read_with(cx, |app, _| predicate(app)) {
            break;
        }
        assert!(Instant::now() < deadline, "asset operation did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn assets_connect_only_on_left_double_click_and_keep_editor_buttons_separate() {
    for mode in [AssetViewMode::List, AssetViewMode::Cards] {
        let root = TestTempDir::new("zzclawterm-assets-double-click");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app(&mut cx, &root, mode);
        let cx = host(&mut cx, &app);
        for count in [1, 2, 3] {
            app.update(cx, |app, _| app.shell.set_status("untouched"));
            let position = item_position(cx, mode, "first");
            click(cx, position, MouseButton::Left, count);
            app.read_with(cx, |app, _| {
                if count == 2 {
                    assert_eq!(
                        app.shell.status(),
                        rust_i18n::t!("dialog.shellArgsUnclosedQuote")
                    );
                } else {
                    assert_eq!(app.shell.status(), "untouched");
                }
            });
        }
        let position = item_position(cx, mode, "first");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down enter");
        draw(cx);
        cx.update(|window, cx| {
            assert!(app.read(cx).terminal.input_focus().is_focused(window));
        });
        app.update(cx, |app, _| app.shell.set_status("untouched"));
        let edit = cx.debug_bounds("asset-edit-first").unwrap();
        if mode == AssetViewMode::Cards {
            let card = cx.debug_bounds("asset-card-first").unwrap();
            let footer = cx.debug_bounds("asset-card-footer-first").unwrap();
            let actions = cx.debug_bounds("asset-card-actions-first").unwrap();
            assert!(
                edit.left() >= card.left() && edit.right() <= card.right(),
                "card={card:?}, footer={footer:?}, actions={actions:?}, edit={edit:?}"
            );
        }
        click(cx, edit.center(), MouseButton::Left, 2);
        app.read_with(cx, |app, _| {
            assert_ne!(
                app.shell.status(),
                rust_i18n::t!("dialog.shellArgsUnclosedQuote")
            );
            assert_eq!(
                app.connection_state
                    .active_editor_draft()
                    .unwrap()
                    .id
                    .as_deref(),
                Some("first")
            );
            assert_eq!(app.session.session_order_len(), 0);
        });
    }
}

#[test]
fn asset_context_targets_follow_rows_fixed_actions_sort_and_search_and_reset_on_blank() {
    for mode in [AssetViewMode::List, AssetViewMode::Cards] {
        let root = TestTempDir::new("zzclawterm-assets-context-target");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app(&mut cx, &root, mode);
        let cx = host(&mut cx, &app);
        for id in ["first", "second", "third"] {
            let position = item_position(cx, mode, id);
            click(cx, position, MouseButton::Right, 1);
            app.read_with(cx, |app, _| {
                assert_eq!(app.start_workspace.context_connection_id(), Some(id));
                assert_eq!(app.connection_state.selected_connections().len(), 2);
            });
            cx.simulate_keystrokes("escape");
            draw(cx);
        }
        if mode == AssetViewMode::List {
            let actions = cx.debug_bounds("asset-actions-second").unwrap();
            click(cx, actions.center(), MouseButton::Right, 1);
            app.read_with(cx, |app, _| {
                assert_eq!(app.start_workspace.context_connection_id(), Some("second"))
            });
            cx.simulate_keystrokes("escape");
        } else {
            cx.simulate_resize(gpui::size(px(640.), px(768.)));
            draw(cx);
            app.read_with(cx, |app, _| {
                assert_eq!(app.start_workspace.card_columns(), 2)
            });
        }
        app.update(cx, |app, cx| {
            app.start_workspace.cycle_sort(AssetSortKey::Name);
            app.start_workspace.cycle_sort(AssetSortKey::Name);
            app.start_workspace.search_field().update(cx, |_, cx| {
                cx.emit(ZzClawInputEvent::Changed("second".into()));
            });
            cx.notify();
        });
        draw(cx);
        let position = item_position(cx, mode, "second");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down down enter");
        draw(cx);
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.connection_state
                    .active_editor_draft()
                    .unwrap()
                    .id
                    .as_deref(),
                Some("second")
            );
        });
        app.update(cx, |app, cx| app.close_connection_editor(cx));
        draw(cx);
        let selector = if mode == AssetViewMode::List {
            "asset-table"
        } else {
            "asset-cards"
        };
        let bounds = cx.debug_bounds(selector).unwrap();
        click(
            cx,
            point(bounds.left() + px(20.), bounds.bottom() - px(25.)),
            MouseButton::Right,
            1,
        );
        app.read_with(cx, |app, _| {
            assert_eq!(app.start_workspace.context_connection_id(), None)
        });
        cx.simulate_keystrokes("down enter");
        draw(cx);
        app.read_with(cx, |app, _| {
            assert!(!app.connection_state.editor_has_draft())
        });
    }
}

#[test]
fn asset_copy_move_and_delete_ignore_connection_panel_multi_selection() {
    for mode in [AssetViewMode::List, AssetViewMode::Cards] {
        let root = TestTempDir::new("zzclawterm-assets-single-target");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app(&mut cx, &root, mode);
        let cx = host(&mut cx, &app);
        let position = item_position(cx, mode, "third");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down down down down enter");
        wait_for(cx, &app, |app| {
            app.connection_state.connections().len() == 4
        });
        app.read_with(cx, |app, _| {
            assert_eq!(app.connection_state.selected_connections().len(), 2);
            let copied = app
                .connection_state
                .connections()
                .iter()
                .find(|item| !["first", "second", "third"].contains(&item.id.as_str()))
                .unwrap();
            assert!(copied.name.starts_with("third"));
        });
        let position = item_position(cx, mode, "third");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down down down down down right down enter");
        wait_for(cx, &app, |app| {
            app.connection_state
                .connection_by_id("third")
                .unwrap()
                .group_id
                .as_deref()
                == Some("destination")
        });
        app.read_with(cx, |app, _| {
            assert!(
                app.connection_state
                    .connection_by_id("first")
                    .unwrap()
                    .group_id
                    .is_none()
            );
            assert!(
                app.connection_state
                    .connection_by_id("second")
                    .unwrap()
                    .group_id
                    .is_none()
            );
        });
        let position = item_position(cx, mode, "third");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down down down down down down enter");
        draw(cx);
        cx.update(|window, cx| assert!(window.has_active_nya_dialog(cx)));
        cx.simulate_keystrokes("escape");
        draw(cx);
        cx.update(|window, cx| assert!(!window.has_active_nya_dialog(cx)));
        let position = item_position(cx, mode, "third");
        click(cx, position, MouseButton::Right, 1);
        cx.simulate_keystrokes("down down down down down down enter");
        draw(cx);
        let confirm = cx.debug_bounds("nya-dialog-action-button").unwrap();
        cx.simulate_click(confirm.center(), Modifiers::default());
        wait_for(cx, &app, |app| {
            app.connection_state.connection_by_id("third").is_none()
        });
        app.read_with(cx, |app, _| {
            assert!(app.connection_state.connection_by_id("first").is_some());
            assert!(app.connection_state.connection_by_id("second").is_some());
        });
    }
}

#[test]
fn asset_virtual_list_context_uses_the_current_scrolled_connection() {
    for mode in [AssetViewMode::List, AssetViewMode::Cards] {
        let root = TestTempDir::new("zzclawterm-assets-scroll-target");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app(&mut cx, &root, mode);
        app.update(&mut cx, |app, _| {
            let connections = (0..100)
                .map(|i| connection(&format!("host-{i:03}")))
                .collect();
            app.connection_state.replace_loaded(connections, Vec::new());
        });
        let cx = host(&mut cx, &app);
        app.update(cx, |app, cx| {
            let scroll = if mode == AssetViewMode::List {
                app.start_workspace.list_scroll()
            } else {
                app.start_workspace.card_scroll()
            };
            scroll.scroll_to_item(20, ScrollStrategy::Top);
            cx.notify();
        });
        draw(cx);
        let index = app.read_with(cx, |app, _| {
            if mode == AssetViewMode::List {
                20
            } else {
                20 * app.start_workspace.card_columns()
            }
        });
        let id = format!("host-{index:03}");
        let position = item_position(cx, mode, &id);
        click(cx, position, MouseButton::Right, 1);
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.start_workspace.context_connection_id(),
                Some(id.as_str())
            )
        });
    }
}

#[test]
fn asset_menu_rechecks_a_connection_removed_after_opening() {
    for mode in [AssetViewMode::List, AssetViewMode::Cards] {
        let root = TestTempDir::new("zzclawterm-assets-stale-menu");
        let mut cx = TestAppContext::single();
        let _cleanup = crate::test_support::TestAppCleanup::new(&cx);
        let app = app(&mut cx, &root, mode);
        let cx = host(&mut cx, &app);
        let position = item_position(cx, mode, "first");
        click(cx, position, MouseButton::Right, 1);
        app.update(cx, |app, _| {
            app.connection_state
                .replace_loaded(vec![connection("second")], Vec::new());
        });
        cx.simulate_keystrokes("down enter");
        draw(cx);
        app.read_with(cx, |app, _| {
            assert_eq!(app.shell.status(), "connection is no longer available");
            assert_eq!(app.session.session_order_len(), 0);
        });
    }
}

#[test]
fn asset_card_columns_follow_available_viewport_width() {
    assert_eq!(responsive_asset_card_columns(f32::NAN), 1);
    assert_eq!(responsive_asset_card_columns(631.), 1);
    assert_eq!(responsive_asset_card_columns(632.), 2);
    assert_eq!(responsive_asset_card_columns(939.), 2);
    assert_eq!(responsive_asset_card_columns(940.), 3);
    assert_eq!(responsive_asset_card_columns(1920.), 3);
}

#[test]
fn asset_actions_stay_fixed_at_the_viewport_right_edge() {
    let viewport_width = px(960.);

    for horizontal_scroll in [px(0.), px(-120.), px(-480.)] {
        let decoration_left = asset_action_left_in_decoration(viewport_width, horizontal_scroll);
        assert_eq!(
            horizontal_scroll + decoration_left,
            px(856.),
            "the 104px action column must stay pinned to the 960px viewport"
        );
    }
}
