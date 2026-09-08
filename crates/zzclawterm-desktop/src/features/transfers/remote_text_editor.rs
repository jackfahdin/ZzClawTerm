use std::ops::Range;
use zzclawterm_ui::ZzClawScrollable;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, HighlightStyle,
    InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render, ScrollDelta, ScrollHandle,
    ScrollWheelEvent, ShapedLine, SharedString, StyledText, TextLayout, TextRun, UTF16Selection,
    UnderlineStyle, Window, div, fill, point, prelude::*, px, relative, rgb, rgba, size,
};

use crate::features::{ZzClawTermApp, shell::gpui_code_font_family};
use crate::models::{TransferEditorField, TransferEditorState};

const UNDO_LIMIT: usize = 64;

#[derive(Clone)]
struct EditSnapshot {
    content: String,
    anchor: usize,
    head: usize,
}

pub(in crate::features) struct RemoteTextEditor {
    app: Entity<ZzClawTermApp>,
    tab_id: String,
    focus_handle: FocusHandle,
    content: String,
    anchor: usize,
    head: usize,
    marked_range: Option<Range<usize>>,
    last_layout: Option<TextLayout>,
    selecting: bool,
    read_only: bool,
    scroll: ScrollHandle,
    scroll_cursor_pending: bool,
    search_query: String,
    active_match: usize,
    undo_stack: Vec<EditSnapshot>,
    redo_stack: Vec<EditSnapshot>,
}

impl RemoteTextEditor {
    pub(in crate::features) fn new(
        app: Entity<ZzClawTermApp>,
        tab: &TransferEditorState,
        cx: &mut Context<Self>,
    ) -> Self {
        let cursor = tab.content.len();
        Self {
            app,
            tab_id: tab.id.clone(),
            focus_handle: cx.focus_handle(),
            content: tab.content.clone(),
            anchor: cursor,
            head: cursor,
            marked_range: None,
            last_layout: None,
            selecting: false,
            read_only: tab.loading || tab.saving,
            scroll: ScrollHandle::new(),
            scroll_cursor_pending: true,
            search_query: tab.search_query.clone(),
            active_match: tab.active_match,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Construct a read-only surface for previewing `content`.
    ///
    /// Reuses the editor's selection, copy, IME `text_for_range`, and painting,
    /// but is permanently `read_only`, so every mutating path early-returns and
    /// it never writes back to any editor tab. `id` only namespaces the scroll
    /// and element ids; it does not have to correspond to an open editor tab.
    pub(in crate::features) fn new_read_only(
        app: Entity<ZzClawTermApp>,
        id: String,
        content: String,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            app,
            tab_id: id,
            focus_handle: cx.focus_handle(),
            content,
            anchor: 0,
            head: 0,
            marked_range: None,
            last_layout: None,
            selecting: false,
            read_only: true,
            scroll: ScrollHandle::new(),
            scroll_cursor_pending: false,
            search_query: String::new(),
            active_match: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Replace the read-only surface's content, resetting the selection. Used
    /// when the previewed tab or its content changes under a stable surface.
    pub(in crate::features) fn sync_read_only(
        &mut self,
        id: &str,
        content: &str,
        cx: &mut Context<Self>,
    ) {
        let changed = self.tab_id != id || self.content != content;
        if !changed {
            return;
        }
        self.tab_id = id.to_string();
        self.content = content.to_string();
        self.anchor = 0;
        self.head = 0;
        self.marked_range = None;
        self.last_layout = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        cx.notify();
    }

    pub(in crate::features) fn sync_from_tab(
        &mut self,
        tab: &TransferEditorState,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        if self.content != tab.content {
            self.content = tab.content.clone();
            let cursor = nearest_char_boundary(&self.content, self.head.min(self.content.len()));
            self.anchor = cursor;
            self.head = cursor;
            self.marked_range = None;
            self.last_layout = None;
            self.undo_stack.clear();
            self.redo_stack.clear();
            self.scroll_cursor_pending = true;
            changed = true;
        }
        let read_only = tab.loading || tab.saving;
        if self.read_only != read_only {
            self.read_only = read_only;
            changed = true;
        }
        if self.search_query != tab.search_query || self.active_match != tab.active_match {
            self.search_query = tab.search_query.clone();
            self.active_match = tab.active_match;
            if self.search_query.is_empty() {
                self.anchor = self.head;
            } else if let Some((start, matched)) = self
                .content
                .match_indices(&self.search_query)
                .nth(self.active_match)
            {
                self.anchor = start;
                self.head = start + matched.len();
                self.scroll_cursor_pending = true;
            }
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::features) fn cursor_position(&self) -> (usize, usize) {
        let cursor = self.head.min(self.content.len());
        let before = &self.content[..cursor];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
        let column = self.content[line_start..cursor].chars().count() + 1;
        (line, column)
    }

    pub(in crate::features) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn selected_range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    fn selection_reversed(&self) -> bool {
        self.head < self.anchor
    }

    fn snapshot(&self) -> EditSnapshot {
        EditSnapshot {
            content: self.content.clone(),
            anchor: self.anchor,
            head: self.head,
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn restore_snapshot(&mut self, snapshot: EditSnapshot, cx: &mut Context<Self>) {
        self.content = snapshot.content;
        self.anchor = snapshot.anchor.min(self.content.len());
        self.head = snapshot.head.min(self.content.len());
        self.marked_range = None;
        self.last_layout = None;
        self.scroll_cursor_pending = true;
        self.sync_content_to_app(cx);
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(snapshot) = self.undo_stack.pop() else {
            return;
        };
        self.redo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot, cx);
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(snapshot) = self.redo_stack.pop() else {
            return;
        };
        self.undo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot, cx);
    }

    fn insert_newline(&mut self, cx: &mut Context<Self>) {
        let range = self.selected_range();
        let start = range.start;
        let current_line_start = line_start(&self.content, start);
        let indentation = self.content[current_line_start..start]
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect::<String>();
        let previous = self.content[..start]
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace());
        let next = self.content[range.end..]
            .chars()
            .find(|ch| !ch.is_whitespace());
        let closes_block = matches!(
            (previous, next),
            (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}'))
        );
        let insertion = if closes_block {
            format!("\n{indentation}    \n{indentation}")
        } else {
            format!("\n{indentation}")
        };
        self.replace_byte_range(range, &insertion, true, cx);
        if closes_block {
            self.anchor = start + 1 + indentation.len() + 4;
            self.head = self.anchor;
            self.scroll_cursor_pending = true;
            cx.notify();
        }
    }

    fn indent_selection(&mut self, outdent: bool, cx: &mut Context<Self>) {
        let selection = self.selected_range();
        if selection.is_empty() && !outdent {
            self.replace_byte_range(selection, "    ", true, cx);
            return;
        }
        let start = line_start(&self.content, selection.start);
        let selection_end = if selection.end > selection.start
            && selection.end == line_start(&self.content, selection.end)
        {
            previous_char_boundary(&self.content, selection.end)
        } else {
            selection.end
        };
        let end = line_end(&self.content, selection_end);
        let source = &self.content[start..end];
        let mut replacement = String::with_capacity(source.len() + 16);
        let mut changed = 0usize;
        for (index, line) in source.split('\n').enumerate() {
            if index > 0 {
                replacement.push('\n');
            }
            if outdent {
                let remove = if line.starts_with('\t') {
                    1
                } else {
                    line.chars().take_while(|ch| *ch == ' ').take(4).count()
                };
                changed += remove;
                replacement.push_str(&line[remove..]);
            } else {
                replacement.push_str("    ");
                replacement.push_str(line);
                changed += 4;
            }
        }
        if changed == 0 {
            return;
        }
        self.replace_byte_range(start..end, &replacement, true, cx);
        self.anchor = start;
        self.head = start + replacement.len();
        self.scroll_cursor_pending = true;
        cx.notify();
    }

    fn sync_content_to_app(&self, cx: &mut Context<Self>) {
        let tab_id = self.tab_id.clone();
        let content = self.content.clone();
        self.app.update(cx, move |app, cx| {
            app.transfer.sync_editor_content(&tab_id, content);
            app.mark_user_activity();
            cx.notify();
        });
    }

    fn notify_cursor_changed(&self, cx: &mut Context<Self>) {
        self.app.update(cx, |app, cx| {
            app.mark_user_activity();
            cx.notify();
        });
    }

    fn replace_byte_range(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        record_undo: bool,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let start = nearest_char_boundary(&self.content, range.start.min(self.content.len()));
        let end =
            nearest_char_boundary(&self.content, range.end.min(self.content.len())).max(start);
        if record_undo {
            self.push_undo();
        }
        self.content.replace_range(start..end, new_text);
        let cursor = start + new_text.len();
        self.anchor = cursor;
        self.head = cursor;
        self.marked_range = None;
        self.last_layout = None;
        self.scroll_cursor_pending = true;
        self.sync_content_to_app(cx);
        cx.notify();
    }

    fn move_cursor(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        let offset = nearest_char_boundary(&self.content, offset.min(self.content.len()));
        if extend {
            self.head = offset;
        } else {
            self.anchor = offset;
            self.head = offset;
        }
        self.marked_range = None;
        self.scroll_cursor_pending = true;
        self.notify_cursor_changed(cx);
        cx.notify();
    }

    fn move_left(&mut self, extend: bool, by_word: bool, cx: &mut Context<Self>) {
        if !extend && self.anchor != self.head {
            self.move_cursor(self.selected_range().start, false, cx);
            return;
        }
        let target = if by_word {
            previous_word_boundary(&self.content, self.head)
        } else {
            previous_char_boundary(&self.content, self.head)
        };
        self.move_cursor(target, extend, cx);
    }

    fn move_right(&mut self, extend: bool, by_word: bool, cx: &mut Context<Self>) {
        if !extend && self.anchor != self.head {
            self.move_cursor(self.selected_range().end, false, cx);
            return;
        }
        let target = if by_word {
            next_word_boundary(&self.content, self.head)
        } else {
            next_char_boundary(&self.content, self.head)
        };
        self.move_cursor(target, extend, cx);
    }

    fn move_vertical(&mut self, delta: isize, extend: bool, cx: &mut Context<Self>) {
        let cursor = self.head.min(self.content.len());
        let current_start = line_start(&self.content, cursor);
        let column = self.content[current_start..cursor].chars().count();
        let target_start = if delta < 0 {
            if current_start == 0 {
                0
            } else {
                line_start(&self.content, current_start - 1)
            }
        } else {
            let current_end = line_end(&self.content, cursor);
            if current_end >= self.content.len() {
                current_start
            } else {
                current_end + 1
            }
        };
        let target_end = line_end(&self.content, target_start);
        let target = byte_offset_for_char_column(&self.content, target_start, target_end, column);
        self.move_cursor(target, extend, cx);
    }

    fn select_word_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let (start, end) = word_bounds(&self.content, offset);
        self.anchor = start;
        self.head = end;
        self.scroll_cursor_pending = true;
        self.notify_cursor_changed(cx);
        cx.notify();
    }

    fn select_line_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let start = line_start(&self.content, offset);
        let mut end = line_end(&self.content, offset);
        if end < self.content.len() {
            end += 1;
        }
        self.anchor = start;
        self.head = end;
        self.scroll_cursor_pending = true;
        self.notify_cursor_changed(cx);
        cx.notify();
    }

    fn index_for_point(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.last_layout.as_ref() else {
            return self.content.len();
        };
        let index = layout
            .index_for_position(position)
            .unwrap_or_else(|index| index)
            .min(self.content.len());
        nearest_char_boundary(&self.content, index)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        self.app.update(cx, |app, cx| {
            if let Some(tab) = app.transfer.active_editor_tab_mut() {
                tab.focused_field = TransferEditorField::Content;
            }
            cx.notify();
        });
        let index = self.index_for_point(event.position);
        if event.click_count >= 3 {
            self.select_line_at(index, cx);
            self.selecting = false;
        } else if event.click_count == 2 {
            self.select_word_at(index, cx);
            self.selecting = false;
        } else {
            if event.modifiers.shift {
                self.head = index;
            } else {
                self.anchor = index;
                self.head = index;
            }
            self.selecting = true;
            self.scroll_cursor_pending = true;
            self.notify_cursor_changed(cx);
            cx.notify();
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !remote_text_selection_drag_active(self.selecting, event.pressed_button) {
            self.selecting = false;
            return;
        }
        self.head = self.index_for_point(event.position);
        self.scroll_cursor_pending = true;
        self.notify_cursor_changed(cx);
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        cx.stop_propagation();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let primary = modifiers.platform || modifiers.control;
        let extend = modifiers.shift;

        if primary && !modifiers.alt {
            let handled = match key {
                "a" => {
                    self.anchor = 0;
                    self.head = self.content.len();
                    self.scroll_cursor_pending = true;
                    self.notify_cursor_changed(cx);
                    cx.notify();
                    true
                }
                "c" => {
                    self.copy_selection(cx);
                    true
                }
                "x" if !self.read_only => {
                    self.cut_selection(cx);
                    true
                }
                "v" if !self.read_only => {
                    self.paste(window, cx);
                    true
                }
                "z" if modifiers.shift => {
                    self.redo(cx);
                    true
                }
                "z" => {
                    self.undo(cx);
                    true
                }
                "y" => {
                    self.redo(cx);
                    true
                }
                "s" => {
                    self.app.update(cx, |app, cx| {
                        app.save_transfer_editor(false, window, cx);
                    });
                    true
                }
                "f" => {
                    self.app.update(cx, |app, cx| {
                        if let Some(tab) = app.transfer.active_editor_tab_mut() {
                            tab.focused_field = TransferEditorField::Search;
                        }
                        window.focus(app.transfer.editor_focus(), cx);
                        cx.notify();
                    });
                    true
                }
                "left" => {
                    self.move_left(extend, true, cx);
                    true
                }
                "right" => {
                    self.move_right(extend, true, cx);
                    true
                }
                _ => false,
            };
            if handled {
                cx.stop_propagation();
            }
            return;
        }

        let handled = match key {
            "left" => {
                self.move_left(extend, false, cx);
                true
            }
            "right" => {
                self.move_right(extend, false, cx);
                true
            }
            "up" => {
                self.move_vertical(-1, extend, cx);
                true
            }
            "down" => {
                self.move_vertical(1, extend, cx);
                true
            }
            "home" => {
                self.move_cursor(line_start(&self.content, self.head), extend, cx);
                true
            }
            "end" => {
                self.move_cursor(line_end(&self.content, self.head), extend, cx);
                true
            }
            "backspace" if !self.read_only => {
                let range = self.selected_range();
                let range = if range.is_empty() {
                    previous_char_boundary(&self.content, self.head)..self.head
                } else {
                    range
                };
                if !range.is_empty() {
                    self.replace_byte_range(range, "", true, cx);
                }
                true
            }
            "delete" if !self.read_only => {
                let range = self.selected_range();
                let range = if range.is_empty() {
                    self.head..next_char_boundary(&self.content, self.head)
                } else {
                    range
                };
                if !range.is_empty() {
                    self.replace_byte_range(range, "", true, cx);
                }
                true
            }
            "enter" if !self.read_only => {
                self.insert_newline(cx);
                true
            }
            "tab" if !self.read_only => {
                self.indent_selection(extend, cx);
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[range].to_string()));
        }
    }

    fn cut_selection(&mut self, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if range.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[range.clone()].to_string(),
        ));
        self.replace_byte_range(range, "", true, cx);
    }

    fn paste(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.replace_byte_range(self.selected_range(), &text.replace("\r\n", "\n"), true, cx);
    }

    fn ensure_cursor_visible(&mut self) {
        if !self.scroll_cursor_pending {
            return;
        }
        self.scroll_cursor_pending = false;
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };
        let Some(cursor) = layout.position_for_index(self.head.min(self.content.len())) else {
            return;
        };
        let viewport = self.scroll.bounds();
        if viewport.size.width <= px(0.) || viewport.size.height <= px(0.) {
            return;
        }
        let line_height = layout.line_height();
        let mut offset = self.scroll.offset();
        if cursor.y < viewport.top() {
            offset.y += viewport.top() - cursor.y;
        } else if cursor.y + line_height > viewport.bottom() {
            offset.y -= cursor.y + line_height - viewport.bottom();
        }
        let max = self.scroll.max_offset();
        offset.x = offset.x.clamp(-max.x, px(0.));
        offset.y = offset.y.clamp(-max.y, px(0.));
        self.scroll.set_offset(offset);
    }
}

fn remote_text_selection_drag_active(selecting: bool, pressed_button: Option<MouseButton>) -> bool {
    selecting && pressed_button == Some(MouseButton::Left)
}

impl EntityInputHandler for RemoteTextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range()),
            reversed: self.selection_reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range());
        if self.marked_range.is_none() && range.is_empty() {
            if is_close_bracket(new_text) && self.content[range.start..].starts_with(new_text) {
                self.move_cursor(range.start + new_text.len(), false, cx);
                return;
            }
            if should_auto_close(&self.content, range.start, new_text)
                && let Some(close) = matching_close_bracket(new_text)
            {
                let start = range.start;
                let pair = format!("{new_text}{close}");
                self.replace_byte_range(range, &pair, true, cx);
                self.anchor = start + new_text.len();
                self.head = self.anchor;
                self.scroll_cursor_pending = true;
                cx.notify();
                return;
            }
        }
        let record_undo = self.marked_range.is_none();
        self.replace_byte_range(range, new_text, record_undo, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range());
        let record_undo = self.marked_range.is_none();
        let start = range.start;
        self.replace_byte_range(range, new_text, record_undo, cx);
        if new_text.is_empty() {
            self.marked_range = None;
            return;
        }
        self.marked_range = Some(start..start + new_text.len());
        if let Some(selected) = new_selected_range_utf16 {
            let relative_start = utf16_to_utf8(new_text, selected.start);
            let relative_end = utf16_to_utf8(new_text, selected.end);
            self.anchor = start + relative_start;
            self.head = start + relative_end;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let position = layout.position_for_index(range.end)?;
        Some(Bounds::new(position, size(px(2.), layout.line_height())))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_point(position)))
    }
}

impl RemoteTextEditor {
    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_to_utf8(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..nearest_char_boundary(&self.content, offset.min(self.content.len()))]
            .encode_utf16()
            .count()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

impl Focusable for RemoteTextEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RemoteTextEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (palette, font_size) = self.app.read_with(cx, |app, _| {
            (
                app.theme_palette(),
                app.settings
                    .summary()
                    .transfer_internal_editor_font_size
                    .clamp(8, 72) as f32,
            )
        });
        let selection = self.selected_range();
        let display_text = if self.content.is_empty() {
            SharedString::from(" ")
        } else {
            SharedString::from(self.content.clone())
        };
        let mut highlights = Vec::new();
        if !self.search_query.is_empty() {
            for (index, (start, matched)) in
                self.content.match_indices(&self.search_query).enumerate()
            {
                if index == self.active_match {
                    continue;
                }
                highlights.push((
                    start..start + matched.len(),
                    HighlightStyle {
                        background_color: Some(rgba((palette.warning << 8) | 0x38).into()),
                        ..Default::default()
                    },
                ));
            }
        }
        if let Some((left, right)) = matching_bracket_ranges(&self.content, self.head) {
            for range in [left, right] {
                highlights.push((
                    range,
                    HighlightStyle {
                        background_color: Some(rgba((palette.primary << 8) | 0x38).into()),
                        underline: Some(UnderlineStyle {
                            color: Some(rgb(palette.primary).into()),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                ));
            }
        }
        if !selection.is_empty() {
            highlights.push((
                selection,
                HighlightStyle {
                    background_color: Some(rgba(0x2f81f750).into()),
                    ..Default::default()
                },
            ));
        } else if let Some(marked_range) = self.marked_range.clone() {
            highlights.push((
                marked_range,
                HighlightStyle {
                    underline: Some(UnderlineStyle {
                        color: Some(rgb(palette.text).into()),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            ));
        }
        let text = StyledText::new(display_text).with_highlights(highlights);

        let editor_surface = div()
            .id(SharedString::from(format!(
                "remote-text-editor-{}",
                self.tab_id
            )))
            .size_full()
            .key_context("RemoteTextEditor")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .bg(rgb(palette.input))
            .font_family(gpui_code_font_family())
            .text_size(px(font_size))
            .line_height(px((font_size + 7.).max(14.)))
            .whitespace_normal()
            .overflow_y_scroll()
            .overflow_x_hidden()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if !event.modifiers.control && !event.modifiers.platform {
                    return;
                }
                let delta_y = match event.delta {
                    ScrollDelta::Pixels(delta) => f32::from(delta.y),
                    ScrollDelta::Lines(delta) => delta.y,
                };
                if delta_y == 0.0 {
                    return;
                }
                let step = if delta_y < 0.0 { 1 } else { -1 };
                this.app.update(cx, |app, cx| {
                    app.adjust_transfer_internal_editor_font_size(step, cx);
                });
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                div()
                    .w_full()
                    .min_h(relative(1.))
                    .relative()
                    .py_3()
                    .pr_3()
                    .pl(px(56.))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px(46.))
                            .border_r_1()
                            .border_color(rgb(palette.border))
                            .bg(rgb(palette.surface)),
                    )
                    .child(RemoteTextElement {
                        editor: cx.entity(),
                        text,
                    }),
            );

        // The editing surface is itself the scroller, so the bar has to hang off a
        // non-scrolling parent or it would scroll away with the text.
        div()
            .size_full()
            .relative()
            .child(editor_surface)
            .vertical_scrollbar(&self.scroll)
    }
}

struct RemoteTextElement {
    editor: Entity<RemoteTextEditor>,
    text: StyledText,
}

struct RemoteTextPrepaint {
    cursor: Option<PaintQuad>,
    active_line: Option<PaintQuad>,
    line_numbers: Vec<(ShapedLine, Point<Pixels>)>,
}

impl IntoElement for RemoteTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RemoteTextElement {
    type RequestLayoutState = ();
    type PrepaintState = RemoteTextPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(id, inspector_id, bounds, state, window, cx);
        let editor = self.editor.read(cx);
        let layout = self.text.layout();
        let palette = editor.app.read(cx).theme_palette();
        let line_height = layout.line_height();
        let cursor_position = layout.position_for_index(editor.head.min(editor.content.len()));
        let cursor = if !editor.selected_range().is_empty() {
            None
        } else {
            cursor_position.map(|position| {
                fill(
                    Bounds::new(position, size(px(1.5), line_height)),
                    rgb(palette.focus_ring),
                )
            })
        };
        let active_start = line_start(&editor.content, editor.head.min(editor.content.len()));
        let active_number = editor.content[..active_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let visual_rows = line_number_visual_rows(&editor.content, &layout.wrapped_text());
        let active_y = visual_rows
            .get(active_number.saturating_sub(1))
            .map(|row| bounds.top() + line_height * *row as f32);
        let active_line = active_y.map(|y| {
            fill(
                Bounds::new(
                    point(bounds.left() - px(56.), y),
                    size(bounds.size.width + px(56.), line_height),
                ),
                rgba((palette.hover << 8) | 0x4d),
            )
        });
        let mut line_numbers = Vec::new();
        let viewport = editor.scroll.bounds();
        for (index, visual_row) in visual_rows.into_iter().enumerate() {
            let y = bounds.top() + line_height * visual_row as f32;
            if y + line_height < viewport.top() || y > viewport.bottom() {
                continue;
            }
            let number = index + 1;
            let label = SharedString::from(number.to_string());
            let color = if number == active_number {
                rgb(palette.text)
            } else {
                rgb(palette.text_dimmed)
            };
            let run = TextRun {
                len: label.len(),
                font: window.text_style().font(),
                color: color.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(label, px(11.), &[run], None);
            let origin = point(bounds.left() - px(10.) - shaped.width, y);
            line_numbers.push((shaped, origin));
        }
        RemoteTextPrepaint {
            cursor,
            active_line,
            line_numbers,
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        if let Some(active_line) = prepaint.active_line.take() {
            window.paint_quad(active_line);
        }
        self.text
            .paint(id, inspector_id, bounds, state, &mut (), window, cx);
        for (line, origin) in prepaint.line_numbers.drain(..) {
            let _ = line.paint(
                origin,
                self.text.layout().line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        let layout = self.text.layout().clone();
        self.editor.update(cx, |editor, cx| {
            editor.last_layout = Some(layout);
            if editor.scroll_cursor_pending {
                editor.ensure_cursor_visible();
                cx.notify();
            }
        });
    }
}

fn matching_close_bracket(value: &str) -> Option<char> {
    match value {
        "(" => Some(')'),
        "[" => Some(']'),
        "{" => Some('}'),
        "\"" => Some('"'),
        "'" => Some('\''),
        _ => None,
    }
}

fn is_close_bracket(value: &str) -> bool {
    matches!(value, ")" | "]" | "}" | "\"" | "'")
}

fn should_auto_close(content: &str, offset: usize, value: &str) -> bool {
    if !matches!(value, "\"" | "'") {
        return true;
    }
    let previous = content[..offset].chars().next_back();
    let next = content[offset..].chars().next();
    !previous.is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        && next.is_none_or(|ch| ch.is_whitespace() || matches!(ch, ')' | ']' | '}' | ',' | ';'))
}

fn matching_bracket_ranges(content: &str, cursor: usize) -> Option<(Range<usize>, Range<usize>)> {
    let cursor = nearest_char_boundary(content, cursor.min(content.len()));
    let candidate = if cursor < content.len() {
        let end = next_char_boundary(content, cursor);
        bracket_kind(content[cursor..end].chars().next()?).map(|kind| (cursor, end, kind))
    } else {
        None
    }
    .or_else(|| {
        if cursor == 0 {
            return None;
        }
        let start = previous_char_boundary(content, cursor);
        bracket_kind(content[start..cursor].chars().next()?).map(|kind| (start, cursor, kind))
    })?;
    let (start, end, (open, close, forward)) = candidate;

    if forward {
        let mut depth = 0usize;
        for (offset, ch) in content[end..].char_indices() {
            if ch == open {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    let match_start = end + offset;
                    return Some((start..end, match_start..match_start + ch.len_utf8()));
                }
                depth -= 1;
            }
        }
    } else {
        let mut depth = 0usize;
        for (offset, ch) in content[..start].char_indices().rev() {
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return Some((offset..offset + ch.len_utf8(), start..end));
                }
                depth -= 1;
            }
        }
    }
    None
}

fn bracket_kind(ch: char) -> Option<(char, char, bool)> {
    match ch {
        '(' => Some(('(', ')', true)),
        '[' => Some(('[', ']', true)),
        '{' => Some(('{', '}', true)),
        ')' => Some(('(', ')', false)),
        ']' => Some(('[', ']', false)),
        '}' => Some(('{', '}', false)),
        _ => None,
    }
}

fn line_number_visual_rows(content: &str, wrapped_text: &str) -> Vec<usize> {
    let visual_lines = wrapped_text.split('\n').collect::<Vec<_>>();
    let mut visual_index = 0usize;
    let mut rows = Vec::with_capacity(content.bytes().filter(|byte| *byte == b'\n').count() + 1);

    for logical_line in content.split('\n') {
        rows.push(visual_index);
        if logical_line.is_empty() {
            visual_index = visual_index.saturating_add(1);
            continue;
        }
        let mut consumed = 0usize;
        while consumed < logical_line.len() && visual_index < visual_lines.len() {
            consumed = consumed.saturating_add(visual_lines[visual_index].len());
            visual_index += 1;
            if visual_lines[visual_index - 1].is_empty() {
                break;
            }
        }
        if consumed < logical_line.len() {
            visual_index = visual_index.saturating_add(1);
        }
    }
    rows
}

fn nearest_char_boundary(content: &str, mut offset: usize) -> usize {
    offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_char_boundary(content: &str, offset: usize) -> usize {
    let offset = nearest_char_boundary(content, offset);
    content[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(content: &str, offset: usize) -> usize {
    let offset = nearest_char_boundary(content, offset);
    content[offset..]
        .chars()
        .next()
        .map(|ch| offset + ch.len_utf8())
        .unwrap_or(content.len())
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn previous_word_boundary(content: &str, offset: usize) -> usize {
    let mut cursor = nearest_char_boundary(content, offset);
    while cursor > 0 {
        let previous = previous_char_boundary(content, cursor);
        let ch = content[previous..cursor].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        cursor = previous;
    }
    let word_kind = cursor
        .checked_sub(1)
        .and_then(|_| {
            let previous = previous_char_boundary(content, cursor);
            content[previous..cursor].chars().next()
        })
        .map(is_word_char);
    while cursor > 0 {
        let previous = previous_char_boundary(content, cursor);
        let ch = content[previous..cursor].chars().next().unwrap_or(' ');
        if Some(is_word_char(ch)) != word_kind || ch.is_whitespace() {
            break;
        }
        cursor = previous;
    }
    cursor
}

fn next_word_boundary(content: &str, offset: usize) -> usize {
    let mut cursor = nearest_char_boundary(content, offset);
    let word_kind = content[cursor..].chars().next().map(is_word_char);
    while cursor < content.len() {
        let next = next_char_boundary(content, cursor);
        let ch = content[cursor..next].chars().next().unwrap_or(' ');
        if Some(is_word_char(ch)) != word_kind || ch.is_whitespace() {
            break;
        }
        cursor = next;
    }
    while cursor < content.len() {
        let next = next_char_boundary(content, cursor);
        let ch = content[cursor..next].chars().next().unwrap_or(' ');
        if !ch.is_whitespace() {
            break;
        }
        cursor = next;
    }
    cursor
}

fn word_bounds(content: &str, offset: usize) -> (usize, usize) {
    if content.is_empty() {
        return (0, 0);
    }
    let mut offset = nearest_char_boundary(content, offset.min(content.len()));
    if offset == content.len() {
        offset = previous_char_boundary(content, offset);
    }
    let next = next_char_boundary(content, offset);
    let target = content[offset..next].chars().next().unwrap_or(' ');
    if target == '\n' {
        return (offset, next);
    }
    let target_word = is_word_char(target);
    let target_whitespace = target.is_whitespace();
    let same_kind = |ch: char| {
        if target_whitespace {
            ch.is_whitespace() && ch != '\n'
        } else if target_word {
            is_word_char(ch)
        } else {
            !is_word_char(ch) && !ch.is_whitespace()
        }
    };
    let mut start = offset;
    while start > 0 {
        let previous = previous_char_boundary(content, start);
        let ch = content[previous..start].chars().next().unwrap_or(' ');
        if !same_kind(ch) {
            break;
        }
        start = previous;
    }
    let mut end = next;
    while end < content.len() {
        let next = next_char_boundary(content, end);
        let ch = content[end..next].chars().next().unwrap_or(' ');
        if !same_kind(ch) {
            break;
        }
        end = next;
    }
    (start, end)
}

fn line_start(content: &str, offset: usize) -> usize {
    let offset = nearest_char_boundary(content, offset);
    content[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end(content: &str, offset: usize) -> usize {
    let offset = nearest_char_boundary(content, offset);
    content[offset..]
        .find('\n')
        .map(|index| offset + index)
        .unwrap_or(content.len())
}

fn byte_offset_for_char_column(content: &str, start: usize, end: usize, column: usize) -> usize {
    content[start..end]
        .char_indices()
        .nth(column)
        .map(|(offset, _)| start + offset)
        .unwrap_or(end)
}

fn utf16_to_utf8(content: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_offset = 0;
    for ch in content.chars() {
        if utf16_offset >= offset {
            break;
        }
        utf16_offset += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

#[cfg(test)]
mod tests {
    use super::{
        byte_offset_for_char_column, is_close_bracket, line_end, line_number_visual_rows,
        line_start, matching_bracket_ranges, matching_close_bracket, nearest_char_boundary,
        next_char_boundary, previous_char_boundary, remote_text_selection_drag_active,
        should_auto_close, utf16_to_utf8, word_bounds,
    };
    use gpui::MouseButton;

    #[test]
    fn editor_selection_drag_stops_when_left_button_is_no_longer_pressed() {
        assert!(remote_text_selection_drag_active(
            true,
            Some(MouseButton::Left)
        ));
        assert!(!remote_text_selection_drag_active(true, None));
        assert!(!remote_text_selection_drag_active(
            true,
            Some(MouseButton::Right)
        ));
        assert!(!remote_text_selection_drag_active(
            false,
            Some(MouseButton::Left)
        ));
    }

    #[test]
    fn editor_boundaries_preserve_utf8_characters() {
        let content = "a你🙂b";
        assert_eq!(next_char_boundary(content, 1), 4);
        assert_eq!(next_char_boundary(content, 4), 8);
        assert_eq!(previous_char_boundary(content, 8), 4);
        assert_eq!(nearest_char_boundary(content, 7), 4);
    }

    #[test]
    fn editor_line_navigation_clamps_to_target_line() {
        let content = "abcdef\nxy\n1234";
        assert_eq!(line_start(content, 5), 0);
        assert_eq!(line_start(content, 9), 7);
        assert_eq!(line_end(content, 7), 9);
        assert_eq!(byte_offset_for_char_column(content, 7, 9, 5), 9);
    }

    #[test]
    fn editor_word_bounds_group_words_and_punctuation() {
        assert_eq!(word_bounds("hello, world", 2), (0, 5));
        assert_eq!(word_bounds("hello, world", 5), (5, 6));
        assert_eq!(word_bounds("hello, world", 8), (7, 12));
    }

    #[test]
    fn editor_utf16_offsets_round_trip_surrogate_pairs() {
        let content = "a🙂你";
        assert_eq!(utf16_to_utf8(content, 1), 1);
        assert_eq!(utf16_to_utf8(content, 3), 5);
        assert_eq!(utf16_to_utf8(content, 4), content.len());
    }

    #[test]
    fn editor_matches_nested_brackets_from_either_side() {
        let content = "fn main() { call([1, 2]); }";
        assert_eq!(matching_bracket_ranges(content, 10), Some((10..11, 26..27)));
        assert_eq!(matching_bracket_ranges(content, 28), Some((10..11, 26..27)));
        assert_eq!(matching_bracket_ranges(content, 18), Some((17..18, 22..23)));
    }

    #[test]
    fn editor_bracket_pair_helpers_cover_supported_pairs() {
        assert_eq!(matching_close_bracket("("), Some(')'));
        assert_eq!(matching_close_bracket("\""), Some('"'));
        assert!(is_close_bracket("}"));
        assert!(!is_close_bracket("("));
        assert!(should_auto_close("value = ", 8, "\""));
        assert!(!should_auto_close("isn't", 3, "'"));
    }

    #[test]
    fn editor_line_numbers_follow_soft_wrapped_rows() {
        assert_eq!(
            line_number_visual_rows("abcdef\nxy\n", "abc\ndef\nxy\n"),
            vec![0, 2, 3]
        );
        assert_eq!(line_number_visual_rows("a\n\nb", "a\n\nb"), vec![0, 1, 2]);
    }
}
