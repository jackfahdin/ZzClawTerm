use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Background, Context, Entity, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext,
    VisualTestContext, Window, deferred, div, point, px, rgb,
};

use super::{compact_app, compact_draw};
use crate::features::ai::panel::AiPanel;
use crate::test_support::{TestAppCleanup, TestConfigDir};

struct ModelMenuDividerHost {
    panel: Entity<AiPanel>,
    divider_hovered: Rc<Cell<bool>>,
    divider_pressed: Rc<Cell<bool>>,
}

impl Render for ModelMenuDividerHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let hovered = self.divider_hovered.clone();
        let pressed = self.divider_pressed.clone();
        div()
            .w(px(320.))
            .h(px(500.))
            .relative()
            .child(self.panel.clone())
            // Cover the panel to exercise the menu's overlap with a resize
            // handle regardless of the menu's measured position or height.
            .child(deferred(
                div()
                    .id("divider")
                    .debug_selector(|| "model-menu-divider".to_string())
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(320.))
                    .h(px(500.))
                    .bg(rgb(0xff1234))
                    .on_hover(move |value, _, _| hovered.set(*value))
                    .on_mouse_down(MouseButton::Left, move |_, _, _| pressed.set(true)),
            ))
    }
}

#[test]
fn model_menu_owns_pointer_above_deferred_dividers() {
    let root = TestConfigDir::new("zzclawterm-ai-model-menu-divider");
    let mut cx = TestAppContext::single();
    let _cleanup = TestAppCleanup::new(&cx);
    let app = compact_app(&mut cx, root.path());
    let panel = app.read_with(&cx, |app, _| app.ai_panel.clone());
    let hovered = Rc::new(Cell::new(false));
    let pressed = Rc::new(Cell::new(false));
    let (_, vcx) = cx.add_window_view({
        let hovered = hovered.clone();
        let pressed = pressed.clone();
        move |_, _| ModelMenuDividerHost {
            panel,
            divider_hovered: hovered,
            divider_pressed: pressed,
        }
    });
    let vcx: &mut VisualTestContext = vcx;
    compact_draw(&app, vcx);
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.ai.toggle_discovery_menu(0);
            app.flush_ai_panel_snapshot(cx);
        });
    });
    compact_draw(&app, vcx);

    let menu = vcx.debug_bounds("ai-model-menu").unwrap();
    vcx.update(|window, cx| {
        let surface = app
            .read(cx)
            .ai_panel
            .read(cx)
            .snapshot()
            .unwrap()
            .chrome
            .surface;
        let scale = window.scale_factor();
        let quads = window.painted_quads();
        let divider_quad = quads
            .iter()
            .find(|quad| quad.background == Background::from(rgb(0xff1234)))
            .expect("divider must be painted");
        let menu_quad = quads
            .iter()
            .find(|quad| {
                quad.background == Background::from(surface)
                    && quad.bounds.map(|value| px(value.0 / scale)) == menu
            })
            .expect("model menu background must be painted");
        assert!(menu_quad.order > divider_quad.order);
    });
    let outside = point(px(8.), px(8.));
    for (position, expected_hover) in [(outside, true), (menu.center(), false)] {
        vcx.simulate_mouse_move(position, None, Modifiers::default());
        compact_draw(&app, vcx);
        assert_eq!(
            hovered.get(),
            expected_hover,
            "pointer={position:?}, menu={menu:?}, divider={:?}",
            vcx.debug_bounds("model-menu-divider")
        );
    }
    let model_row = point(menu.center().x, menu.bottom() - px(22.));
    vcx.simulate_click(model_row, Modifiers::default());
    compact_draw(&app, vcx);
    assert!(!pressed.get(), "the menu must not start a divider drag");
    assert!(
        vcx.debug_bounds("ai-model-menu").is_none(),
        "selecting a model must close the menu"
    );

    vcx.simulate_click(outside, Modifiers::default());
    assert!(
        pressed.get(),
        "the divider must still work outside the menu"
    );
}
