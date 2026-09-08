use std::ops::Range;

use gpui::{
    Bounds, Context, ElementInputHandler, EntityInputHandler, IntoElement, Pixels, Point, Size,
    UTF16Selection, Window, prelude::*, px,
};

use crate::features::NyaTermApp;

fn terminal_input_selection() -> UTF16Selection {
    UTF16Selection {
        range: 0..0,
        reversed: false,
    }
}

fn terminal_visible_surface_bounds(
    bounds: Bounds<Pixels>,
    content_mask: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let visible = content_mask.intersect(&bounds);
    (visible.size.width > px(0.) && visible.size.height > px(0.)).then_some(visible)
}

/// Invisible canvas child that records the terminal output bounds for selection hit-testing.
pub(in crate::features) fn terminal_bounds_tracker(
    entity: gpui::Entity<NyaTermApp>,
    session_id: Option<String>,
    active: bool,
) -> impl IntoElement {
    let input_entity = entity.clone();
    let bounds_entity = input_entity.clone();
    let tracked_session_id = session_id.clone();
    gpui::canvas(
        move |bounds, window, cx| {
            let Some(bounds) =
                terminal_visible_surface_bounds(bounds, window.content_mask().bounds)
            else {
                return;
            };
            let scale_factor = window.scale_factor();
            if bounds_entity
                .read(cx)
                .terminal_surface_bounds_tracking_is_current(
                    session_id.as_deref(),
                    bounds,
                    scale_factor,
                )
            {
                return;
            }
            // Defer mutation so we never re-enter the entity while layout/prepaint is running.
            let entity = entity.clone();
            let session_id = tracked_session_id.clone();
            cx.defer(move |cx| {
                entity.update(cx, |this, cx| {
                    this.remember_terminal_surface_bounds_for_session_and_sync(
                        session_id.as_deref(),
                        bounds,
                        scale_factor,
                        cx,
                    );
                });
            });
        },
        move |bounds, _state, window, cx| {
            if !active {
                return;
            }
            let focus = input_entity.read(cx).terminal.input.focus.clone();
            if input_entity
                .read(cx)
                .settings
                .summary()
                .interaction_mac_ime_compatibility
            {
                window.handle_input(&focus, ElementInputHandler::new(bounds, input_entity), cx);
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

impl EntityInputHandler for NyaTermApp {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            let text = self.terminal.paste.text();
            let byte_range = byte_range_from_utf16(text, &range);
            *adjusted_range = Some(utf16_range_from_bytes(text, &byte_range));
            return Some(text[byte_range].to_string());
        }
        if self.terminal.input.ime_marked_text.is_empty() {
            return None;
        }
        let len = self.terminal.input.ime_marked_text.encode_utf16().count();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        *adjusted_range = Some(start..end);
        Some(self.terminal.input.ime_marked_text.clone())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.terminal.paste.focus.is_focused(window) {
            let text = self.terminal.paste.text();
            let range = self.terminal.paste.selected_byte_range();
            let reversed = self
                .terminal
                .paste
                .anchor
                .is_some_and(|anchor| anchor > self.terminal.paste.cursor);
            return Some(UTF16Selection {
                range: utf16_range_from_bytes(text, &range),
                reversed,
            });
        }
        // GPUI's IME contract needs a valid insertion range even when there is
        // no marked text. This is also what lets CJK candidate windows anchor to
        // the terminal cursor instead of treating the surface as non-editable.
        Some(terminal_input_selection())
    }

    fn marked_text_range(
        &self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            return self
                .terminal
                .paste
                .marked_range
                .as_ref()
                .map(|range| utf16_range_from_bytes(self.terminal.paste.text(), range));
        }
        let len = self.terminal.input.ime_marked_text.encode_utf16().count();
        (len > 0).then_some(0..len)
    }

    fn unmark_text(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            self.terminal.paste.marked_text.clear();
            self.terminal.paste.marked_range = None;
            return;
        }
        self.terminal.input.ime_marked_text.clear();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            let range = range
                .as_ref()
                .map(|range| byte_range_from_utf16(self.terminal.paste.text(), range))
                .or_else(|| self.terminal.paste.marked_range.clone())
                .unwrap_or_else(|| self.terminal.paste.selected_byte_range());
            if self.terminal.paste.replace_range(range, text) {
                cx.notify();
            }
            return;
        }
        self.terminal.input.ime_marked_text.clear();
        if !text.is_empty() {
            if let Some(selected) = self.smart_cursor_selected_input_range()
                && self.replace_smart_input_selection(selected, text, cx)
            {
                return;
            }
            let bytes = text.as_bytes().to_vec();
            let has_buffer_selection = self.terminal.selection.selection.is_some()
                && self.smart_cursor_selected_input_range().is_none();
            if has_buffer_selection {
                self.send_terminal_input_without_suggestion_track(bytes, cx);
            } else {
                self.send_terminal_input(bytes, cx);
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            let range = range
                .as_ref()
                .map(|range| byte_range_from_utf16(self.terminal.paste.text(), range))
                .or_else(|| self.terminal.paste.marked_range.clone())
                .unwrap_or_else(|| self.terminal.paste.selected_byte_range());
            let start = range.start;
            if !self.terminal.paste.replace_range(range, new_text) {
                return;
            }
            self.terminal.paste.marked_text = new_text.to_string();
            self.terminal.paste.marked_range =
                (!new_text.is_empty()).then_some(start..start + new_text.len());
            if let Some(selected) = new_selected_range {
                let selected = byte_range_from_utf16(new_text, &selected);
                self.terminal.paste.anchor =
                    (selected.start != selected.end).then_some(start + selected.start);
                self.terminal.paste.cursor = start + selected.end;
            }
            cx.notify();
            return;
        }
        self.terminal.input.ime_marked_text = new_text.to_string();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            return Some(element_bounds);
        }
        let (cell_w, cell_h) = self.terminal_cell_size();
        let insets = self.terminal_content_insets();
        let gutter = self.terminal_gutter_width_px();
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), 0);
        let row = if snapshot.cursor.row == usize::MAX {
            snapshot.row_count().saturating_sub(1)
        } else {
            snapshot
                .cursor
                .row
                .min(snapshot.row_count().saturating_sub(1))
        };
        let col = snapshot.cursor.col.min(snapshot.cols.saturating_sub(1));
        let origin_x = f32::from(element_bounds.origin.x);
        let origin_y = f32::from(element_bounds.origin.y);
        let max_x = origin_x + f32::from(element_bounds.size.width) - cell_w.max(1.);
        let max_y = origin_y + f32::from(element_bounds.size.height) - cell_h.max(1.);
        let x = (origin_x + insets.left + gutter + col as f32 * cell_w)
            .min(max_x)
            .max(origin_x);
        let y = (origin_y + insets.top + row as f32 * cell_h)
            .min(max_y)
            .max(origin_y);
        Some(gpui::bounds(
            Point { x: px(x), y: px(y) },
            Size {
                width: px(cell_w.max(1.)),
                height: px(cell_h.max(1.)),
            },
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.terminal.paste.draft.is_some() && self.terminal.paste.focus.is_focused(window) {
            return Some(utf16_offset_for_byte(
                self.terminal.paste.text(),
                self.terminal.paste.cursor,
            ));
        }
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, px};

    use super::{terminal_input_selection, terminal_visible_surface_bounds};

    #[test]
    fn terminal_input_selection_keeps_a_valid_insertion_point() {
        let selection = terminal_input_selection();

        assert_eq!(selection.range, 0..0);
        assert!(!selection.reversed);
    }

    #[test]
    fn terminal_surface_bounds_follow_the_visible_content_mask() {
        let bounds = Bounds::new(
            gpui::point(px(10.), px(20.)),
            gpui::size(px(600.), px(4_000.)),
        );
        let content_mask = Bounds::new(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(1_280.), px(800.)),
        );

        assert_eq!(
            terminal_visible_surface_bounds(bounds, content_mask),
            Some(Bounds::new(
                gpui::point(px(10.), px(20.)),
                gpui::size(px(600.), px(780.))
            ))
        );
    }

    #[test]
    fn terminal_surface_bounds_exclude_scrollbar_column() {
        let outer_width = 610.;
        let text_width = outer_width
            - crate::features::terminal::terminal_surface::TERMINAL_SCROLLBAR_COLUMN_WIDTH;
        let bounds = Bounds::new(
            gpui::point(px(10.), px(20.)),
            gpui::size(px(text_width), px(400.)),
        );
        let content_mask = Bounds::new(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(1_280.), px(800.)),
        );

        assert_eq!(
            terminal_visible_surface_bounds(bounds, content_mask),
            Some(Bounds::new(
                gpui::point(px(10.), px(20.)),
                gpui::size(px(text_width), px(400.))
            ))
        );
    }

    #[test]
    fn terminal_surface_bounds_skip_fully_clipped_surfaces() {
        let bounds = Bounds::new(
            gpui::point(px(10.), px(900.)),
            gpui::size(px(600.), px(400.)),
        );
        let content_mask = Bounds::new(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(1_280.), px(800.)),
        );

        assert_eq!(terminal_visible_surface_bounds(bounds, content_mask), None);
    }
}

fn byte_offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf16_offset = 0usize;
    for (byte_offset, ch) in text.char_indices() {
        if utf16_offset >= offset {
            return byte_offset;
        }
        utf16_offset += ch.len_utf16();
        if utf16_offset >= offset {
            return byte_offset + ch.len_utf8();
        }
    }
    text.len()
}

fn byte_range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = byte_offset_from_utf16(text, range.start);
    let end = byte_offset_from_utf16(text, range.end).max(start);
    start..end
}

fn utf16_offset_for_byte(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[..offset].encode_utf16().count()
}

fn utf16_range_from_bytes(text: &str, range: &Range<usize>) -> Range<usize> {
    utf16_offset_for_byte(text, range.start)..utf16_offset_for_byte(text, range.end)
}

pub(super) fn open_external_url_for_action(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("empty url".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to open url: {error}"))
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to open url: {error}"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to open url: {error}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum SmartSelectionEdge {
    Start,
    End,
}
