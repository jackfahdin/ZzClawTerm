use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, MouseDownEvent,
    MouseUpEvent, Render, TestAppContext, VisualTestContext, Window, div, point, prelude::*, px,
};
use zzclawterm_core::{
    AppRuntime, QuickCommand, QuickCommandCategory, RuntimeMode, test_support::TestTempDir,
};
use zzclawterm_ui::zzclaw_root;

use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
use crate::features::ZzClawTermApp;
use crate::models::QuickCommandViewMode;

struct QuickCommandsFixture {
    app: Entity<ZzClawTermApp>,
}

impl Render for QuickCommandsFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self.app.update(cx, |app, cx| {
            app.quick_commands_panel(cx).into_any_element()
        });
        div().w(px(800.)).h(px(500.)).child(panel)
    }
}

fn command(id: &str, text: &str) -> QuickCommand {
    serde_json::from_value(serde_json::json!({
        "id": id, "label": id, "command": text,
    }))
    .unwrap()
}

fn app(cx: &mut TestAppContext, root: &TestTempDir) -> Entity<ZzClawTermApp> {
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
    app.update(cx, |app, cx| app.sync_component_theme(cx));
    app
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn right_click(cx: &mut VisualTestContext, position: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Right,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    draw(cx);
}

#[test]
fn quick_command_context_menu_copies_each_row_and_resets_to_blank_in_every_view() {
    for mode in [
        QuickCommandViewMode::List,
        QuickCommandViewMode::Compact,
        QuickCommandViewMode::Tile,
    ] {
        let root = TestTempDir::new("zzclawterm-quick-command-context");
        let mut cx = TestAppContext::single();
        let app = app(&mut cx, &root);
        app.update(&mut cx, |app, _| {
            app.commands.replace_quick_command_catalog(
                vec![
                    command("first", "  printf 'first'\n"),
                    command("second", "pwd\nprintf 'second'"),
                ],
                vec![QuickCommandCategory {
                    id: "docker".into(),
                    name: "Docker".into(),
                    parent_id: None,
                    sort_order: 0,
                }],
            );
            app.commands.set_quick_view_mode(mode);
        });
        let fixture_app = app.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            cx.observe(&fixture_app, |_, _, cx| cx.notify()).detach();
            let fixture = cx.new(|_| QuickCommandsFixture { app: fixture_app });
            zzclaw_root(fixture, window, cx)
        });
        draw(cx);

        for (selector, text) in [
            ("quick-command-row-first", "  printf 'first'\n"),
            ("quick-command-row-second", "pwd\nprintf 'second'"),
        ] {
            let row = cx.debug_bounds(selector).unwrap();
            right_click(cx, row.center());
            cx.simulate_keystrokes("down down enter");
            draw(cx);
            cx.update(|_, cx| {
                assert_eq!(
                    cx.read_from_clipboard()
                        .and_then(|item| item.text())
                        .as_deref(),
                    Some(text),
                    "{mode:?}: copied the wrong row"
                );
            });
        }

        // Select without rendering first: the action must use current state, and
        // the blank-area capture must clear the previous row's menu target.
        app.update(cx, |app, _| {
            app.commands.select_quick_category("docker".into())
        });
        let pane = cx.debug_bounds("quick-command-pane").unwrap();
        right_click(cx, point(pane.center().x, pane.bottom() - px(20.)));
        cx.simulate_keystrokes("down enter");
        draw(cx);
        app.read_with(cx, |app, _| {
            let editor = app
                .commands
                .quick_editor()
                .expect("blank menu should create a command");
            assert_eq!(editor.category_id.as_deref(), Some("docker"));
            assert!(editor.command.is_empty());
        });
    }
}

#[test]
fn quick_command_toolbar_and_empty_state_create_in_an_empty_selected_category() {
    for selector in ["quick-command-add", "quick-command-empty-add"] {
        let root = TestTempDir::new("zzclawterm-quick-command-toolbar-category");
        let mut cx = TestAppContext::single();
        let app = app(&mut cx, &root);
        app.update(&mut cx, |app, _| {
            app.commands.replace_quick_command_catalog(
                vec![command("elsewhere", "pwd")],
                vec![QuickCommandCategory {
                    id: "docker".into(),
                    name: "Docker".into(),
                    parent_id: None,
                    sort_order: 0,
                }],
            );
            app.commands.select_quick_category("docker".into());
        });
        let fixture_app = app.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let fixture = cx.new(|_| QuickCommandsFixture { app: fixture_app });
            zzclaw_root(fixture, window, cx)
        });
        draw(cx);
        let button = cx
            .debug_bounds(selector)
            .expect("empty category should offer creation even with commands elsewhere");
        cx.simulate_click(button.center(), Modifiers::default());
        draw(cx);
        app.read_with(cx, |app, _| {
            assert_eq!(
                app.commands.quick_editor().unwrap().category_id.as_deref(),
                Some("docker")
            );
        });
    }
}

#[test]
fn quick_command_category_blank_menu_creates_a_root_category() {
    let root = TestTempDir::new("zzclawterm-quick-command-category-blank");
    let mut cx = TestAppContext::single();
    let app = app(&mut cx, &root);
    app.update(&mut cx, |app, _| {
        app.commands.replace_quick_command_catalog(
            Vec::new(),
            vec![QuickCommandCategory {
                id: "docker".into(),
                name: "Docker".into(),
                parent_id: None,
                sort_order: 0,
            }],
        );
        app.commands.select_quick_category("docker".into());
    });
    let fixture_app = app.clone();
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let fixture = cx.new(|_| QuickCommandsFixture { app: fixture_app });
        zzclaw_root(fixture, window, cx)
    });
    draw(cx);
    let blank = cx.debug_bounds("quick-command-category-blank").unwrap();
    right_click(cx, blank.center());
    cx.simulate_keystrokes("down enter");
    draw(cx);
    app.read_with(cx, |app, _| {
        let create = app
            .commands
            .quick_category_create()
            .expect("blank sidebar should open category creation");
        assert_eq!(create.parent_id, None);
    });
}
