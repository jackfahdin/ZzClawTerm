use gpui::{
    App, Entity, Focusable as _, IntoElement, MouseButton, RenderOnce, Window, canvas, div,
    prelude::*, px, rgb, uniform_list,
};
use zzclawterm_core::hex_document::{HexCopyFormat, HexCursor, HexNibble};

use super::ZzClawHexEditorState;
use crate::{ThemePalette, ZzClawContextMenu, ZzClawMenuItem, ZzClawScrollable as _};

#[derive(IntoElement)]
pub struct ZzClawHexEditor {
    id: gpui::SharedString,
    state: Entity<ZzClawHexEditorState>,
    palette: ThemePalette,
    compact: bool,
    placeholder: Option<gpui::SharedString>,
    labels: [gpui::SharedString; 7],
}

const CELL_WIDTH: f32 = 22.;
const CELL_GAP: f32 = 4.;
const GROUP_GAP: f32 = 8.;
const ASCII_CELL_WIDTH: f32 = 10.;

fn hex_column_width(per_row: usize) -> f32 {
    let groups = (per_row / 4) as f32;
    groups * (4. * CELL_WIDTH + 3. * CELL_GAP) + (groups - 1.) * GROUP_GAP
}

impl ZzClawHexEditor {
    pub fn new(state: &Entity<ZzClawHexEditorState>, palette: ThemePalette) -> Self {
        Self {
            id: format!("hex-editor-{:?}", state.entity_id()).into(),
            state: state.clone(),
            palette,
            compact: false,
            placeholder: None,
            labels: [
                "Copy Hex".into(),
                "Copy Compact Hex".into(),
                "Copy as C Array".into(),
                "Copy as Escaped Bytes".into(),
                "Copy ASCII".into(),
                "Paste".into(),
                "Select All".into(),
            ],
        }
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Hint shown beside the cursor while the document is empty.
    pub fn placeholder(mut self, placeholder: impl Into<gpui::SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn id(mut self, id: impl Into<gpui::SharedString>) -> Self {
        self.id = id.into();
        self
    }

    pub fn menu_labels(mut self, labels: [gpui::SharedString; 7]) -> Self {
        self.labels = labels;
        self
    }
}

impl RenderOnce for ZzClawHexEditor {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = &self.state;
        let palette = self.palette;
        let width = state.read(cx).width;
        let per_row = if width >= 900. {
            16
        } else if width >= 600. {
            12
        } else {
            8
        };
        let ascii = !self.compact && width >= 430.;
        let showing_placeholder = self.placeholder.is_some() && {
            let doc = &state.read(cx).document;
            doc.bytes().is_empty() && doc.pending_nibble().is_none()
        };
        let offset = !self.compact && width >= 700.;
        let rows = state.read(cx).document.bytes().len() / per_row + 1;
        let scroll = state.read(cx).scroll.clone();
        let focus = state.read(cx).focus_handle(cx);
        let rows_state = state.clone();
        let placeholder = self.placeholder.clone();
        let list = uniform_list(
            "hex-editor.rows",
            rows,
            move |range: std::ops::Range<usize>, window, cx| {
                let layout = HexRowLayout {
                    per_row,
                    ascii,
                    offset,
                    focused: rows_state.read(cx).focus.is_focused(window),
                    palette,
                };
                range
                    .map(|row| hex_row(&rows_state, row, &layout, placeholder.as_ref(), cx))
                    .collect::<Vec<_>>()
            },
        )
        .flex_1()
        .min_h_0()
        .w_full()
        .track_scroll(&scroll);
        let key_state = state.clone();
        let mouse_state = state.clone();
        let up_state = state.clone();
        let out_state = state.clone();
        let bounds_state = state.clone();
        let selector = self.id.to_string();
        let editor = div()
            .id(self.id)
            .debug_selector(move || selector.clone())
            .track_focus(&focus)
            .key_context("ZzClawHexEditor")
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(palette.input))
            .text_color(rgb(palette.text))
            .text_size(px(12.))
            .font_family(if cfg!(target_os = "windows") {
                "Consolas"
            } else {
                "JetBrains Mono"
            })
            .on_key_down(move |event, _, cx| {
                if key_state.update(cx, |state, cx| state.key_down(event, cx)) {
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&mouse_state.read(cx).focus_handle(cx), cx);
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                up_state.update(cx, |state, _| state.dragging = None);
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                out_state.update(cx, |state, _| state.dragging = None);
            })
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let width = f32::from(bounds.size.width);
                        let state = bounds_state.clone();
                        window.defer(cx, move |_, cx| {
                            state.update(cx, |state, cx| {
                                state.bytes_per_row = per_row;
                                if (state.width - width).abs() > 1. {
                                    state.width = width;
                                    cx.notify();
                                }
                            });
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .when(!self.compact, |editor| {
                editor.child(
                    div()
                        .h(px(22.))
                        .flex_none()
                        .px_2()
                        .flex()
                        .gap_3()
                        .items_center()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_dimmed))
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .when(offset, |row| {
                            row.child(div().w(px(72.)).flex_none().child("OFFSET"))
                        })
                        .child(
                            div()
                                .w(px(hex_column_width(per_row)))
                                .flex_none()
                                .child("HEX"),
                        )
                        .when(ascii && !showing_placeholder, |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .pl_3()
                                    .border_l_1()
                                    .border_color(gpui::transparent_black())
                                    .child("ASCII"),
                            )
                        }),
                )
            })
            .child(list)
            .vertical_scrollbar(&scroll);
        let mut items = Vec::new();
        for (index, format) in [
            HexCopyFormat::Hex,
            HexCopyFormat::Compact,
            HexCopyFormat::CArray,
            HexCopyFormat::Escaped,
            HexCopyFormat::Ascii,
        ]
        .into_iter()
        .enumerate()
        {
            let state = state.clone();
            items.push(ZzClawMenuItem::action(self.labels[index].clone()).on_click(
                move |_, window, cx| {
                    state.update(cx, |state, cx| state.copy(format, cx));
                    window.focus(&state.read(cx).focus_handle(cx), cx);
                },
            ));
        }
        let paste_state = state.clone();
        items.push(ZzClawMenuItem::action(self.labels[5].clone()).on_click(
            move |_, window, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    paste_state.update(cx, |state, cx| state.paste(&text, cx));
                }
                window.focus(&paste_state.read(cx).focus_handle(cx), cx);
            },
        ));
        let select_state = state.clone();
        items.push(ZzClawMenuItem::action(self.labels[6].clone()).on_click(
            move |_, window, cx| {
                select_state.update(cx, |state, cx| state.select_all(cx));
                window.focus(&select_state.read(cx).focus_handle(cx), cx);
            },
        ));
        ZzClawContextMenu::new(editor, items)
    }
}

struct HexRowLayout {
    per_row: usize,
    ascii: bool,
    offset: bool,
    focused: bool,
    palette: ThemePalette,
}

fn hex_row(
    state: &Entity<ZzClawHexEditorState>,
    row: usize,
    layout: &HexRowLayout,
    placeholder: Option<&gpui::SharedString>,
    cx: &App,
) -> gpui::AnyElement {
    let HexRowLayout {
        per_row,
        ascii,
        offset,
        focused,
        palette,
    } = *layout;
    let doc = &state.read(cx).document;
    let start = row * per_row;
    let length = doc.bytes().len();
    let selection = doc.selection();
    let cursor = doc.cursor();
    let placeholder = placeholder.filter(|_| length == 0 && doc.pending_nibble().is_none());
    // An empty document shows only the cursor cell followed by the hint.
    let columns = if placeholder.is_some() { 1 } else { per_row };
    let mut hex = div().flex().flex_none().items_center().gap(px(GROUP_GAP));
    let mut ascii_row = div()
        .flex()
        .items_center()
        .flex_none()
        .pl_3()
        .border_l_1()
        .border_color(rgb(palette.border));
    for group in 0..columns.div_ceil(4) {
        let mut cells = div().flex().gap(px(CELL_GAP));
        for column in group * 4..(group * 4 + 4).min(columns) {
            let index = start + column;
            let selected = selection.as_ref().is_some_and(|r| r.contains(&index));
            let byte = doc.bytes().get(index).copied();
            let pending = (cursor.byte_index == index)
                .then(|| doc.pending_nibble())
                .flatten();
            let mut cell = div()
                .flex()
                .w(px(CELL_WIDTH))
                .flex_none()
                .rounded(px(2.))
                .when(selected, |cell| cell.bg(rgb(palette.terminal_selection)));
            for nibble in [HexNibble::High, HexNibble::Low] {
                let active = focused && cursor.byte_index == index && cursor.nibble == nibble;
                let text = match (nibble, pending, byte) {
                    (HexNibble::High, Some(high), _) => format!("{high:X}"),
                    (HexNibble::High, _, Some(b)) => format!("{:X}", b >> 4),
                    (HexNibble::Low, _, Some(b)) => format!("{:X}", b & 15),
                    _ => " ".to_string(),
                };
                let click_state = state.clone();
                let drag_state = state.clone();
                cell = cell.child(
                    div()
                        .id(("nibble", index * 2 + usize::from(nibble == HexNibble::Low)))
                        .w(px(CELL_WIDTH / 2.))
                        .h(px(22.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_b_2()
                        .border_color(if active {
                            rgb(palette.accent).into()
                        } else {
                            gpui::transparent_black()
                        })
                        .child(text)
                        .when(index <= length, |c| {
                            c.cursor_text()
                                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                    click_state.update(cx, |state, cx| {
                                        let anchor = if event.modifiers.shift {
                                            state.document.selection_anchor()
                                        } else {
                                            index
                                        };
                                        state.move_cursor(
                                            HexCursor {
                                                byte_index: index,
                                                nibble,
                                            },
                                            event.modifiers.shift,
                                            cx,
                                        );
                                        state.dragging = Some(anchor);
                                        window.focus(&state.focus, cx);
                                    });
                                    cx.stop_propagation();
                                })
                                .on_mouse_move(move |event, _, cx| {
                                    if !event.dragging() {
                                        return;
                                    }
                                    drag_state.update(cx, |state, cx| state.drag_to(index, cx));
                                })
                        }),
                );
            }
            cells = cells.child(cell);
            let ascii_state = state.clone();
            let at_cursor = focused && cursor.byte_index == index && !selected;
            ascii_row = ascii_row.child(
                div()
                    .id(("ascii", index))
                    .w(px(ASCII_CELL_WIDTH))
                    .h(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |c| c.bg(rgb(palette.terminal_selection)))
                    .border_b_1()
                    .border_color(if at_cursor && byte.is_some() {
                        rgb(palette.accent).into()
                    } else {
                        gpui::transparent_black()
                    })
                    .child(
                        byte.map(|b| {
                            if (0x20..=0x7e).contains(&b) {
                                char::from(b)
                            } else {
                                '·'
                            }
                        })
                        .unwrap_or(' ')
                        .to_string(),
                    )
                    .when(index < length, |c| {
                        c.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            ascii_state.update(cx, |state, cx| {
                                state.move_cursor(
                                    HexCursor {
                                        byte_index: index,
                                        nibble: HexNibble::High,
                                    },
                                    event.modifiers.shift,
                                    cx,
                                );
                                window.focus(&state.focus, cx);
                            });
                            cx.stop_propagation();
                        })
                    }),
            );
        }
        hex = hex.child(cells);
    }
    div()
        .h(px(24.))
        .w_full()
        .px_2()
        .flex()
        .items_center()
        .gap_3()
        .when(offset, |r| {
            r.child(
                div()
                    .w(px(72.))
                    .flex_none()
                    .text_color(rgb(palette.text_dimmed))
                    .child(format!("{start:08X}")),
            )
        })
        .map(|r| match placeholder {
            Some(hint) => r.child(hex).child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_color(rgb(palette.text_dimmed))
                    .child(hint.clone()),
            ),
            None => r
                .child(
                    div()
                        .w(px(hex_column_width(per_row)))
                        .flex_none()
                        .child(hex),
                )
                .when(ascii, |r| r.child(ascii_row)),
        })
        .into_any_element()
}
