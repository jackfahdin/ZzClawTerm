use std::{ops::Range, sync::Arc};

use gpui::{
    AnyElement, App, Bounds, IntoElement, MouseButton, Pixels, Point, UniformListDecoration,
    WeakEntity, Window, div,
    prelude::{InteractiveElement, ParentElement, Styled},
    px, rgb,
};
use rust_i18n::t;

use crate::features::pages::connections::{
    list::{ConnectionListRow, icon_action_button},
    panel::ConnectionPanel,
};
use crate::theme::ThemePalette;

use super::CONNECTION_ACTION_CLEARANCE_PX;

const CONNECTION_ACTION_RIGHT_INSET_PX: f32 = 8.;
const CONNECTION_ACTION_BUTTON_SIZE_PX: f32 = 24.;

/// The row action strip is a uniform-list decoration rather than a row child.
/// Rows are translated by the horizontal scroll offset, while this decoration
/// cancels that translation and therefore stays at the viewport's right edge.
pub(super) struct ConnectionRowActionsDecoration {
    rows: Arc<[ConnectionListRow]>,
    panel: WeakEntity<ConnectionPanel>,
    palette: ThemePalette,
}

impl ConnectionRowActionsDecoration {
    pub(super) fn new(
        rows: Arc<[ConnectionListRow]>,
        panel: WeakEntity<ConnectionPanel>,
        palette: ThemePalette,
    ) -> Self {
        Self {
            rows,
            panel,
            palette,
        }
    }
}

impl UniformListDecoration for ConnectionRowActionsDecoration {
    fn compute(
        &self,
        visible_range: Range<usize>,
        bounds: Bounds<Pixels>,
        scroll_offset: Point<Pixels>,
        item_height: Pixels,
        _item_count: usize,
        window: &mut Window,
        _cx: &mut App,
    ) -> AnyElement {
        let Some((row_index, connection_id)) = hovered_connection(
            &self.rows,
            visible_range,
            bounds,
            scroll_offset,
            item_height,
            window.mouse_position(),
        ) else {
            return div().into_any_element();
        };

        let connection_id = connection_id.to_string();
        let connect_id = connection_id.clone();
        let edit_id = connection_id.clone();
        let connect_panel = self.panel.clone();
        let edit_panel = self.panel.clone();
        let action_left = action_left_in_decoration(bounds.size.width, scroll_offset.x);
        let action_top =
            item_height * row_index + (item_height - px(CONNECTION_ACTION_BUTTON_SIZE_PX)) / 2.;
        let action_selector = format!("connection-actions-{connection_id}");
        let palette = self.palette;

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id(action_selector.clone())
                    .debug_selector(move || action_selector.clone())
                    .absolute()
                    .left(action_left)
                    .top(action_top)
                    .h(px(CONNECTION_ACTION_BUTTON_SIZE_PX))
                    .w(px(CONNECTION_ACTION_CLEARANCE_PX))
                    .px_1()
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .bg(rgb(palette.hover))
                    // A mouse-down reaches the list before a click. Stop both so
                    // pressing an action never clears the current multi-selection.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(icon_action_button(
                        palette,
                        format!("connection-connect-{connection_id}"),
                        "icons/conn/connect.svg",
                        t!("savedConnections.connect"),
                        move |_, window, cx| {
                            cx.stop_propagation();
                            let Some(panel) = connect_panel.upgrade() else {
                                return;
                            };
                            panel.update(cx, |panel, cx| {
                                panel.with_app(cx, |app, cx| {
                                    let Some(connection) = app
                                        .connection_state
                                        .connections()
                                        .iter()
                                        .find(|connection| connection.id == connect_id)
                                        .cloned()
                                    else {
                                        return;
                                    };
                                    app.start_saved_connection(connection, window, cx);
                                });
                            });
                        },
                    ))
                    .child(icon_action_button(
                        palette,
                        format!("connection-edit-{connection_id}"),
                        "icons/net/edit.svg",
                        t!("savedConnections.edit"),
                        move |_, window, cx| {
                            cx.stop_propagation();
                            let Some(panel) = edit_panel.upgrade() else {
                                return;
                            };
                            panel.update(cx, |panel, cx| {
                                panel.with_app(cx, |app, cx| {
                                    app.open_connection_editor(
                                        Some(edit_id.clone()),
                                        None,
                                        false,
                                        window,
                                        cx,
                                    );
                                });
                            });
                        },
                    )),
            )
            .into_any_element()
    }
}

fn hovered_connection(
    rows: &[ConnectionListRow],
    visible_range: Range<usize>,
    content_bounds: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
    item_height: Pixels,
    pointer: Point<Pixels>,
) -> Option<(usize, &str)> {
    let row_index = hovered_row_index(
        visible_range,
        content_bounds,
        scroll_offset,
        item_height,
        pointer,
    )?;
    match rows.get(row_index)? {
        ConnectionListRow::Connection { connection_id, .. } => {
            Some((row_index, connection_id.as_str()))
        }
        _ => None,
    }
}

fn hovered_row_index(
    visible_range: Range<usize>,
    content_bounds: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
    item_height: Pixels,
    pointer: Point<Pixels>,
) -> Option<usize> {
    if item_height <= px(0.) {
        return None;
    }

    // UniformList passes decoration bounds whose origin already includes the
    // scroll offset. Undo it only for viewport hit-testing; row positions remain
    // in content coordinates rooted at `content_bounds.origin`.
    let viewport_bounds = Bounds::new(content_bounds.origin - scroll_offset, content_bounds.size);
    if !viewport_bounds.contains(&pointer) {
        return None;
    }

    let row_index = ((pointer.y - content_bounds.origin.y) / item_height).floor() as usize;
    visible_range.contains(&row_index).then_some(row_index)
}

fn action_left_in_decoration(viewport_width: Pixels, horizontal_scroll: Pixels) -> Pixels {
    viewport_width
        - horizontal_scroll
        - px(CONNECTION_ACTION_RIGHT_INSET_PX + CONNECTION_ACTION_CLEARANCE_PX)
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use gpui::{Bounds, point, px, size};

    use crate::features::pages::connections::list::ConnectionListRow;

    use super::{action_left_in_decoration, hovered_connection, hovered_row_index};

    #[test]
    fn action_screen_position_stays_at_the_viewport_right_edge() {
        let viewport_width = px(280.);
        let content_origin_x = px(12.);

        for horizontal_scroll in [px(0.), px(-40.), px(-240.)] {
            let decoration_left = action_left_in_decoration(viewport_width, horizontal_scroll);
            let scrolled_content_origin = content_origin_x + horizontal_scroll;
            assert_eq!(
                scrolled_content_origin + decoration_left,
                px(228.),
                "the 56px action strip must remain 8px from the 292px viewport right edge"
            );
        }
    }

    #[test]
    fn hovered_row_tracks_vertical_scroll_and_visible_range() {
        let viewport_origin = point(px(10.), px(20.));
        let viewport_size = size(px(280.), px(102.));
        let item_height = px(34.);
        let visible_range = Range { start: 5, end: 8 };
        let scroll_offset = point(px(-120.), px(-170.));
        let content_bounds = Bounds::new(viewport_origin + scroll_offset, viewport_size);

        assert_eq!(
            hovered_row_index(
                visible_range.clone(),
                content_bounds,
                scroll_offset,
                item_height,
                point(px(100.), px(37.)),
            ),
            Some(5)
        );
        assert_eq!(
            hovered_row_index(
                visible_range.clone(),
                content_bounds,
                scroll_offset,
                item_height,
                point(px(100.), px(88.)),
            ),
            Some(7)
        );
        assert_eq!(
            hovered_row_index(
                visible_range,
                content_bounds,
                scroll_offset,
                item_height,
                point(px(100.), px(122.)),
            ),
            None,
            "the pointer is outside the viewport"
        );
    }

    #[test]
    fn fractional_dpi_item_height_maps_hover_without_exact_pixel_assumptions() {
        let viewport_origin = point(px(10.), px(20.));
        let item_height = px(33.6);
        let viewport_size = size(px(160.), item_height * 3);
        let visible_range = 5..8;
        let scroll_offset = point(px(-40.), -(item_height * 5));
        let content_bounds = Bounds::new(viewport_origin + scroll_offset, viewport_size);

        assert_eq!(
            hovered_row_index(
                visible_range.clone(),
                content_bounds,
                scroll_offset,
                item_height,
                point(px(40.), viewport_origin.y + px(16.)),
            ),
            Some(5)
        );
        assert_eq!(
            hovered_row_index(
                visible_range,
                content_bounds,
                scroll_offset,
                item_height,
                point(px(40.), viewport_origin.y + item_height + px(16.)),
            ),
            Some(6)
        );
    }

    #[test]
    fn actions_are_only_created_for_connection_rows() {
        let rows = vec![
            ConnectionListRow::Separator,
            ConnectionListRow::EmptyGroup { depth: 0 },
            ConnectionListRow::Connection {
                connection_id: "connection".to_string(),
                depth: 0,
            },
        ];
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(160.), px(102.)));
        let offset = point(px(0.), px(0.));

        assert_eq!(
            hovered_connection(
                &rows,
                0..3,
                bounds,
                offset,
                px(34.),
                point(px(20.), px(37.)),
            ),
            None
        );
        assert_eq!(
            hovered_connection(
                &rows,
                0..3,
                bounds,
                offset,
                px(34.),
                point(px(20.), px(71.)),
            ),
            None
        );
        assert_eq!(
            hovered_connection(
                &rows,
                0..3,
                bounds,
                offset,
                px(34.),
                point(px(20.), px(105.)),
            ),
            Some((2, "connection"))
        );
    }

    #[test]
    fn hovered_row_rejects_invalid_height_and_pointer_outside_viewport() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(34.)));
        let range = 0..1;

        assert_eq!(
            hovered_row_index(
                range.clone(),
                bounds,
                point(px(0.), px(0.)),
                px(0.),
                point(px(20.), px(25.)),
            ),
            None
        );
        assert_eq!(
            hovered_row_index(
                range,
                bounds,
                point(px(0.), px(0.)),
                px(34.),
                point(px(9.), px(25.)),
            ),
            None
        );
    }
}
