use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, div, px,
};

use crate::root::zzclaw_root;
use crate::text::{ZzClawTextFormat, ZzClawTextScope, ZzClawTextState};
use crate::{ZzClawMarkdown, ZzClawSelectableText};

struct TextFixture {
    first: ZzClawTextState,
    second: ZzClawTextState,
    scope: ZzClawTextScope,
}

impl Render for TextFixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(280.))
            .h(px(200.))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "first-text".into())
                    .child(self.scope.wrap(ZzClawMarkdown::from_state(&self.first))),
            )
            .child(
                div().debug_selector(|| "second-text".into()).child(
                    self.scope
                        .wrap(ZzClawSelectableText::from_state(&self.second)),
                ),
            )
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn literal_text_and_code_preserve_whitespace_and_markup(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = "\n  中文 😀 <tag>& `code`\n\tsecond line  \n\n";
    for format in [ZzClawTextFormat::Plain, ZzClawTextFormat::Code] {
        let state = cx.update(|cx| ZzClawTextState::new(format, text, cx));
        cx.run_until_parked();
        cx.update(|cx| {
            state.inner.update(cx, |state, cx| state.select_all(cx));
            assert_eq!(state.selected_text(cx), text);
        });
    }
}

#[gpui::test]
fn scope_copies_only_its_members_and_keeps_document_order(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let first = ZzClawTextState::new(ZzClawTextFormat::Markdown, "**first**", cx);
        let second = ZzClawTextState::new(ZzClawTextFormat::Plain, "  second  ", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![first.clone(), second.clone()]);
        TextFixture {
            first,
            second,
            scope,
        }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    vcx.update(|window, cx| {
        let view = fixture.read(cx);
        let first = view.first.clone();
        let second = view.second.clone();
        let scope = view.scope.clone();
        let unrelated = ZzClawTextState::new(ZzClawTextFormat::Markdown, "unrelated", cx);
        first.inner.update(cx, |state, cx| state.select_all(cx));
        second.inner.update(cx, |state, cx| state.select_all(cx));
        unrelated.inner.update(cx, |state, cx| state.select_all(cx));
        assert_eq!(scope.selected_text(window, cx), "first\n  second  ");
    });
}

#[gpui::test]
fn replacing_streamed_content_invalidates_copy_and_append_keeps_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(|cx| ZzClawTextState::new(ZzClawTextFormat::Markdown, "first\n\n", cx));
    cx.run_until_parked();
    cx.update(|cx| state.inner.update(cx, |state, cx| state.select_all(cx)));
    cx.update(|cx| state.set_text("first\n\nsecond", cx));
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(!state.inner.read(cx).selected_text().is_empty());
        assert_eq!(state.selected_text(cx), "first");
        state.set_text(&"replacement".repeat(1024), cx);
        assert_eq!(state.selected_text(cx), "");
    });
}

#[test]
fn code_fences_cannot_be_closed_by_literal_command_content() {
    let document = super::text_document(ZzClawTextFormat::Code, "echo ```\n````\n");
    assert_eq!(document, "`````\necho ```\n````\n");
}

#[gpui::test]
fn mouse_drag_selects_across_surfaces_and_keyboard_copy_keeps_literal_whitespace(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let first = ZzClawTextState::new(ZzClawTextFormat::Markdown, "**first**", cx);
        let second = ZzClawTextState::new(ZzClawTextFormat::Plain, "  second  ", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![first.clone(), second.clone()]);
        TextFixture {
            first,
            second,
            scope,
        }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    let first = vcx.debug_bounds("first-text").unwrap();
    let second = vcx.debug_bounds("second-text").unwrap();
    let start = gpui::point(first.left(), first.bottom() - px(6.));
    let end = gpui::point(second.right() - px(1.), second.top() + px(6.));
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    draw(vcx);
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(vcx);
    vcx.update(|window, cx| {
        let scope = fixture.read(cx).scope.clone();
        assert_eq!(scope.selected_text(window, cx), "first\n  second  ");
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "first\n  second  "
        )
    });
}

struct VirtualTextFixture {
    states: Vec<ZzClawTextState>,
    scope: ZzClawTextScope,
    scroll: gpui::Entity<crate::chat::ZzClawMessageScrollerState>,
}

impl Render for VirtualTextFixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        let states = self.states.clone();
        let scope = self.scope.clone();
        div().w(px(280.)).h(px(240.)).child(
            self.scope.retain_selection(
                crate::chat::ZzClawMessageScroller::new(
                    "virtual-text-fixture",
                    self.scroll.clone(),
                    move |index, _, _| {
                        let scope = scope.clone();
                        div()
                            .debug_selector(move || format!("virtual-text-{index}"))
                            .h(px(80.))
                            .child(scope.wrap(ZzClawSelectableText::from_state(&states[index])))
                    },
                )
                .size_full(),
            ),
        )
    }
}

#[gpui::test]
fn dragging_across_virtual_rows_preserves_endpoints_and_copies_unmounted_middle_rows(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let states: Vec<_> = (0..24)
            .map(|index| {
                ZzClawTextState::new(ZzClawTextFormat::Plain, &format!("row {index}  "), cx)
            })
            .collect();
        let scope = ZzClawTextScope::default();
        scope.replace_members(states.clone());
        let scroll = cx.new(|cx| crate::chat::ZzClawMessageScrollerState::new(states.len(), cx));
        scroll.update(cx, |scroll, cx| {
            scroll.scroll_to_item(0, cx);
        });
        VirtualTextFixture {
            states,
            scope,
            scroll,
        }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    let first = vcx.debug_bounds("virtual-text-0").unwrap();
    let next = vcx.debug_bounds("virtual-text-1").unwrap();
    let start = gpui::point(first.left(), first.top() + px(6.));
    let next_end = gpui::point(next.right() - px(1.), next.top() + px(6.));
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(next_end, Some(gpui::MouseButton::Left), Default::default());
    draw(vcx);
    let scroll = vcx.update(|_, cx| fixture.read(cx).scroll.clone());
    vcx.update(|_, cx| {
        scroll.update(cx, |scroll, cx| {
            scroll.scroll_to_item(21, cx);
        })
    });
    draw(vcx);
    assert!(vcx.debug_bounds("virtual-text-0").is_none());
    let last = vcx.debug_bounds("virtual-text-23").unwrap();
    let end = gpui::point(last.right() - px(1.), last.top() + px(6.));
    vcx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    draw(vcx);
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(vcx);
    let expected = (0..24)
        .map(|index| format!("row {index}  "))
        .collect::<Vec<_>>()
        .join("\n");
    vcx.update(|window, cx| {
        assert_eq!(
            fixture.read(cx).scope.clone().selected_text(window, cx),
            expected
        )
    });
    vcx.update(|_, cx| {
        scroll.update(cx, |scroll, cx| {
            scroll.scroll_to_item(10, cx);
        })
    });
    draw(vcx);
    vcx.update(|window, cx| {
        assert_eq!(
            fixture.read(cx).scope.clone().selected_text(window, cx),
            expected
        )
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), expected));
}

#[gpui::test]
fn markdown_copy_matches_rendered_unicode_links_lists_tables_and_multiline_code(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let source = "# 中文 😀\n\n**bold** [link](https://example.com)\n\n- first\n- second\n\n| A | B |\n| --- | --- |\n| 中文 | 😀 |\n\n```rust\n  let x = 1;\n\tlet y = 2;  \n```";
    let state = cx.update(|cx| ZzClawTextState::new(ZzClawTextFormat::Markdown, source, cx));
    cx.run_until_parked();
    cx.update(|cx| {
        state.select_all(cx);
        let text = state.selected_text(cx);
        assert!(text.contains("中文 😀\n"));
        assert!(text.contains("bold link\n"));
        assert!(text.contains("first\nsecond\n"));
        assert!(text.contains("A B\n中文 😀\n"));
        assert!(text.contains("  let x = 1;\n\tlet y = 2;  "), "{text:?}");
        assert!(!text.contains("https://") && !text.contains("**") && !text.contains("```"));
    });
}

struct CodeFixture {
    states: [ZzClawTextState; 2],
    scope: ZzClawTextScope,
}
impl Render for CodeFixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(280.)).h(px(100.)).flex().flex_col().children(
            self.states.iter().enumerate().map(|(index, state)| {
                // Callers may create equivalent highlight arrays each frame. That
                // must not replace the renderer callback or reset native selections.
                let highlights = vec![(
                    2..5,
                    gpui::HighlightStyle {
                        color: Some(gpui::rgb(0x336699).into()),
                        ..Default::default()
                    },
                )]
                .into();
                div()
                    .debug_selector(move || format!("selectable-code-{index}"))
                    .child(self.scope.wrap(super::ZzClawSelectableCode::from_state(
                        state,
                        super::ZzClawTextStyle::default(),
                        highlights,
                    )))
            }),
        )
    }
}

#[gpui::test]
fn prehighlighted_commands_remain_selectable_across_repeated_draws(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let states = [
            ZzClawTextState::new(ZzClawTextFormat::Code, "  cmd first  ", cx),
            ZzClawTextState::new(ZzClawTextFormat::Code, "  cmd second  ", cx),
        ];
        let scope = ZzClawTextScope::default();
        scope.replace_members(states.to_vec());
        CodeFixture { states, scope }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    let first = vcx.debug_bounds("selectable-code-0").unwrap();
    let last = vcx.debug_bounds("selectable-code-1").unwrap();
    let start = gpui::point(first.left(), first.top() + px(6.));
    let end = gpui::point(last.right() - px(1.), last.top() + px(6.));
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    draw(vcx);
    draw(vcx);
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(vcx);
    vcx.update(|window, cx| {
        assert_eq!(
            fixture.read(cx).scope.clone().selected_text(window, cx),
            "  cmd first  \n  cmd second  "
        )
    });
}

struct InputTextFixture {
    text: ZzClawTextState,
    scope: ZzClawTextScope,
    input: gpui::Entity<crate::ZzClawInputState>,
}

impl Render for InputTextFixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(280.)).h(px(200.)).child(
            self.scope.retain_selection(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "input-fixture-text".into())
                            .child(self.scope.wrap(ZzClawMarkdown::from_state(&self.text))),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(32.))
                            .child(crate::ZzClawInput::new(&self.input)),
                    ),
            ),
        )
    }
}

#[gpui::test]
fn transcript_copy_delegates_to_focused_inputs_and_select_all_reaches_text(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let text = ZzClawTextState::new(ZzClawTextFormat::Markdown, "**transcript**", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![text.clone()]);
        let input = cx.new(|cx| crate::ZzClawInputState::new(cx, "question input"));
        InputTextFixture { text, scope, input }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    vcx.update(|window, cx| fixture.read(cx).text.clone().focus(window, cx));
    vcx.simulate_keystrokes("secondary-a secondary-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "transcript"
        )
    });
    vcx.update(|window, cx| {
        let input = fixture.read(cx).input.clone();
        let focus = input.read(cx).component_focus_handle(cx);
        window.focus(&focus, cx);
        input.update(cx, |input, cx| input.select_all(window, cx));
    });
    draw(vcx);
    vcx.simulate_keystrokes("secondary-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "question input"
        )
    });
}

#[gpui::test]
fn streaming_tail_selection_copies_original_range_and_outside_click_clears_highlight(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let first = ZzClawTextState::new(ZzClawTextFormat::Markdown, "first", cx);
        let second = ZzClawTextState::new(ZzClawTextFormat::Plain, "second", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![first.clone(), second.clone()]);
        TextFixture {
            first,
            second,
            scope,
        }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    let bounds = vcx.debug_bounds("first-text").unwrap();
    let start = gpui::point(bounds.left(), bounds.top() + px(6.));
    let end = gpui::point(bounds.right() - px(1.), bounds.top() + px(6.));
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(vcx);
    vcx.update(|_, cx| fixture.read(cx).first.clone().set_text("first\nsecond", cx));
    draw(vcx);
    vcx.update(|window, cx| {
        let scope = fixture.read(cx).scope.clone();
        assert_eq!(scope.selected_text(window, cx), "first");
        assert!(fixture.read(cx).first.applied_selection.borrow().is_some());
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "first"));
    vcx.simulate_click(gpui::point(px(350.), px(180.)), Default::default());
    draw(vcx);
    vcx.update(|window, cx| {
        let scope = fixture.read(cx).scope.clone();
        assert!(fixture.read(cx).first.applied_selection.borrow().is_none());
        assert_eq!(scope.selected_text(window, cx), "");
    });
}

#[gpui::test]
fn streamed_literal_fence_growth_preserves_the_selected_payload(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let first = ZzClawTextState::new(ZzClawTextFormat::Markdown, "first", cx);
        let second = ZzClawTextState::new(ZzClawTextFormat::Plain, "  output  ", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![first.clone(), second.clone()]);
        TextFixture {
            first,
            second,
            scope,
        }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    let bounds = vcx.debug_bounds("second-text").unwrap();
    let start = gpui::point(bounds.left(), bounds.top() + px(6.));
    let end = gpui::point(bounds.right() - px(1.), bounds.top() + px(6.));
    vcx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    draw(vcx);
    vcx.update(|_, cx| {
        fixture
            .read(cx)
            .second
            .clone()
            .set_text("  output  \nmore", cx)
    });
    draw(vcx);
    vcx.update(|_, cx| {
        let state = fixture.read(cx).second.clone();
        state.set_text("  output  \nmore\n````\n", cx);
        assert_eq!(state.selected_text(cx), "  output  ");
    });
    draw(vcx);
    vcx.update(|window, cx| {
        let scope = fixture.read(cx).scope.clone();
        assert_eq!(scope.selected_text(window, cx), "  output  ");
    });
    vcx.simulate_keystrokes("secondary-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "  output  "
        )
    });
    vcx.simulate_click(gpui::point(px(350.), px(180.)), Default::default());
    draw(vcx);
    vcx.update(|window, cx| {
        assert_eq!(fixture.read(cx).scope.clone().selected_text(window, cx), "");
        assert!(fixture.read(cx).second.applied_selection.borrow().is_none());
    });
}

struct MenuTextFixture {
    state: ZzClawTextState,
    scope: ZzClawTextScope,
}

impl Render for MenuTextFixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        let scope = self.scope.clone();
        div().w(px(280.)).h(px(200.)).child(
            self.scope.retain_selection(
                self.scope.wrap(crate::ZzClawContextMenu::new_dynamic(
                    div()
                        .debug_selector(|| "streaming-menu-text".into())
                        .child(ZzClawSelectableText::from_state(&self.state)),
                    move |window, cx| {
                        let text = scope.selected_text(window, cx);
                        vec![
                            crate::ZzClawMenuItem::action("Copy").on_click(move |_, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    text.clone(),
                                ));
                            }),
                        ]
                    },
                )),
            ),
        )
    }
}

#[gpui::test]
fn clicking_context_copy_preserves_streamed_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let fixture = cx.new(|cx| {
        let state = ZzClawTextState::new(ZzClawTextFormat::Plain, "  selected  ", cx);
        let scope = ZzClawTextScope::default();
        scope.replace_members(vec![state.clone()]);
        MenuTextFixture { state, scope }
    });
    let (_, vcx) = cx.add_window_view(|window, cx| zzclaw_root(fixture.clone(), window, cx));
    let vcx: &mut VisualTestContext = vcx;
    draw(vcx);
    vcx.update(|window, cx| {
        let state = fixture.read(cx).state.clone();
        state.select_all(cx);
        state.focus(window, cx);
        state.set_text("  selected  \nnext", cx);
    });
    draw(vcx);
    let bounds = vcx.debug_bounds("streaming-menu-text").unwrap();
    let position = gpui::point(bounds.left() + px(10.), bounds.top() + px(30.));
    vcx.simulate_mouse_down(position, gpui::MouseButton::Right, Default::default());
    vcx.simulate_mouse_up(position, gpui::MouseButton::Right, Default::default());
    draw(vcx);
    vcx.update(|window, cx| assert!(crate::nya_popup_is_active(window, cx)));
    vcx.simulate_click(position + gpui::point(px(30.), px(17.)), Default::default());
    draw(vcx);
    vcx.update(|window, cx| {
        assert!(!crate::nya_popup_is_active(window, cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "  selected  "
        );
        assert_eq!(
            fixture.read(cx).scope.clone().selected_text(window, cx),
            "  selected  "
        );
    });
}
