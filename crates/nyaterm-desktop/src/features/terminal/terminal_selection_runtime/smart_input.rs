use gpui::{App, Context, KeyDownEvent, MouseUpEvent, Pixels, Point};
use nyaterm_core::{
    InputSelectionRange, TerminalInputState, apply_terminal_input_data,
    build_move_input_cursor_data, can_suggest_from_tracker, delete_terminal_input_range,
};

use crate::features::NyaTermApp;
use crate::terminal::{TerminalTextCell, terminal_text_cells};

use super::helpers::SmartSelectionEdge;

impl NyaTermApp {
    pub(in crate::features) fn handle_smart_input_click(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) {
        if event.modifiers.control
            || event.modifiers.platform
            || event.modifiers.alt
            || event.modifiers.shift
        {
            return;
        }
        if !self.can_use_smart_cursor_selection() {
            return;
        }
        let Some(target) = self.input_index_at_mouse(event.position, cx) else {
            return;
        };
        let _ = self.move_smart_input_cursor(target, cx);
    }

    pub(in crate::features) fn input_index_at_mouse(
        &self,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<usize> {
        if !self.can_use_smart_cursor_selection() {
            return None;
        }
        let state = &self.terminal.assist.command_input_tracker;
        if state.value.is_empty() {
            return None;
        }
        let cell = self.point_to_terminal_cell(position, cx)?;
        let offset = self.active_terminal_display_offset();
        if offset != 0 {
            return None;
        }
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), 0);
        let snapshot_row = self.terminal_snapshot_row_for_session_viewport_row(
            self.session.active_id(),
            snapshot.as_ref(),
            0,
            cell.row,
        )?;
        if snapshot_row != snapshot.cursor.row {
            return None;
        }
        let line = snapshot.line(snapshot_row).unwrap_or("");
        let line_cells = smart_input_cells(line);
        let value_cells = smart_input_cells(&state.value);
        if value_cells.is_empty() || value_cells.len() > line_cells.len() {
            return None;
        }
        let input_start_col =
            find_smart_input_start_col(&line_cells, &value_cells, snapshot.cursor.col)?;
        let input_end_col = input_start_col + value_cells.len();
        if cell.col < input_start_col {
            return None;
        }
        let cell_index = if cell.col >= input_end_col {
            value_cells.len()
        } else {
            cell.col - input_start_col
        };
        Some(byte_for_smart_input_cell_index(&value_cells, cell_index))
    }

    pub(in crate::features) fn move_smart_input_cursor(
        &mut self,
        target_cursor: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_use_smart_cursor_selection() {
            return false;
        }
        let mut state = self.terminal.assist.command_input_tracker.clone();
        let next_cursor = target_cursor.min(state.value.len());
        let payload = build_move_input_cursor_data(&state.value, state.cursor, next_cursor);
        if payload.is_empty() && next_cursor == state.cursor {
            return false;
        }
        state.cursor = next_cursor;
        self.terminal.assist.command_input_tracker = state;
        if !payload.is_empty() {
            self.send_terminal_input_without_suggestion_track(payload.into_bytes(), cx);
        } else {
            cx.notify();
        }
        true
    }

    pub(in crate::features) fn can_use_smart_cursor_selection(&self) -> bool {
        if self.session.active_id().is_none() {
            return false;
        }
        if self.is_credential_prompt_input_mode() {
            return false;
        }
        let state = &self.terminal.assist.command_input_tracker;
        if state.desynced || state.line_rewrite_required || state.paste_mode || state.multiline {
            return false;
        }
        if let Some(session_id) = self.session.active_id()
            && !self.sync_peer_session_ids(session_id).is_empty()
        {
            return false;
        }
        true
    }

    /// Map the painted terminal selection onto the tracked input line when it is fully contained.
    pub(in crate::features) fn smart_cursor_selected_input_range(
        &self,
    ) -> Option<InputSelectionRange> {
        if !self.can_use_smart_cursor_selection() {
            return None;
        }
        let state = &self.terminal.assist.command_input_tracker;
        if state.value.is_empty() {
            return None;
        }
        let selection = self.terminal.selection.selection.as_ref()?;
        if selection.is_empty() || selection.all_buffer {
            return None;
        }
        // Only single-row selections can map to a single-line tracked input.
        let (start, end) = selection.ordered();
        if start.line != end.line {
            return None;
        }

        let offset = self.active_terminal_display_offset();
        if offset != 0 {
            // Selection in history is not the live input line.
            return None;
        }
        let snapshot = self.terminal_snapshot_for_session(self.session.active_id(), 0);

        let snapshot_row =
            crate::features::terminal::terminal_surface::terminal_absolute_line_for_snapshot_row(
                snapshot.as_ref(),
                snapshot.cursor.row,
            )
            .filter(|line| *line == start.line)
            .map(|_| snapshot.cursor.row)?;
        if snapshot_row != snapshot.cursor.row {
            return None;
        }

        let line = snapshot.line(snapshot_row).unwrap_or("");
        let line_cells = smart_input_cells(line);
        let (col_start, col_end_excl) = selection.cols_for_absolute_line(start.line)?;
        let col_end = col_end_excl.min(line_cells.len().max(col_start));
        let col_start = col_start.min(col_end);
        if col_end <= col_start {
            return None;
        }

        // Find tracked value as a suffix of the cursor line (prompt + input).
        let value = &state.value;
        if value.is_empty() {
            return None;
        }
        let value_cells = smart_input_cells(value);
        if value_cells.len() > line_cells.len() {
            return None;
        }
        // Prefer alignment ending at cursor_col (input ends at cursor when typing at end).
        // Fall back to last occurrence of value as a contiguous span on the line.
        let input_start_col =
            find_smart_input_start_col(&line_cells, &value_cells, snapshot.cursor.col)?;
        let input_end_col = input_start_col + value_cells.len();

        if col_start < input_start_col || col_end > input_end_col {
            return None;
        }

        let sel_start_char = col_start - input_start_col;
        let sel_end_char = col_end - input_start_col;
        if sel_end_char <= sel_start_char || sel_end_char > value_cells.len() {
            return None;
        }

        let byte_start = byte_for_smart_input_cell_index(&value_cells, sel_start_char);
        let byte_end = byte_end_for_smart_input_cell_index(&value_cells, sel_end_char);
        InputSelectionRange::new(byte_start, byte_end)
    }

    pub(in crate::features) fn send_smart_selection_payload(
        &mut self,
        next_state: TerminalInputState,
        payload: String,
        cx: &mut Context<Self>,
    ) {
        self.terminal.assist.command_input_tracker = next_state;
        self.clear_terminal_selection(cx);
        // Don't double-track via note_command_suggestion_input; state is already updated.
        self.send_terminal_input_without_suggestion_track(payload.into_bytes(), cx);
        if can_suggest_from_tracker(&self.terminal.assist.command_input_tracker) {
            self.schedule_command_suggestion_refresh(cx);
        } else if self.terminal.assist.command_suggestions.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::features) fn delete_smart_input_selection(
        &mut self,
        selected: InputSelectionRange,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_use_smart_cursor_selection() {
            return false;
        }
        let state = self.terminal.assist.command_input_tracker.clone();
        let move_to_end = build_move_input_cursor_data(&state.value, state.cursor, selected.end);
        let delete_count = selected.len_chars(&state.value);
        if delete_count == 0 {
            return false;
        }
        let delete_bytes = "\u{007f}".repeat(delete_count);
        let next = delete_terminal_input_range(&state, selected.start, selected.end);
        let payload = format!("{move_to_end}{delete_bytes}");
        self.send_smart_selection_payload(next, payload, cx);
        true
    }

    pub(in crate::features) fn replace_smart_input_selection(
        &mut self,
        selected: InputSelectionRange,
        data: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_use_smart_cursor_selection() || data.is_empty() {
            return false;
        }
        let state = self.terminal.assist.command_input_tracker.clone();
        let move_to_end = build_move_input_cursor_data(&state.value, state.cursor, selected.end);
        let delete_count = selected.len_chars(&state.value);
        let delete_bytes = "\u{007f}".repeat(delete_count);
        let after_delete = delete_terminal_input_range(&state, selected.start, selected.end);
        let next = apply_terminal_input_data(&after_delete, data);
        let payload = format!("{move_to_end}{delete_bytes}{data}");
        self.send_smart_selection_payload(next, payload, cx);
        true
    }

    pub(in crate::features) fn collapse_smart_input_selection(
        &mut self,
        selected: InputSelectionRange,
        edge: SmartSelectionEdge,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_use_smart_cursor_selection() {
            return false;
        }
        let state = self.terminal.assist.command_input_tracker.clone();
        let target = match edge {
            SmartSelectionEdge::Start => selected.start,
            SmartSelectionEdge::End => selected.end,
        };
        let payload = build_move_input_cursor_data(&state.value, state.cursor, target);
        let mut next = state;
        next.cursor = target.min(next.value.len());
        self.terminal.assist.command_input_tracker = next;
        self.clear_terminal_selection(cx);
        if !payload.is_empty() {
            self.send_terminal_input_without_suggestion_track(payload.into_bytes(), cx);
        } else {
            cx.notify();
        }
        true
    }

    /// Handle Backspace/Delete/arrows/plain text against a smart input selection.
    /// Returns true when the key was consumed.
    pub(in crate::features) fn handle_smart_input_selection_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selected) = self.smart_cursor_selected_input_range() else {
            return false;
        };
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control
            || keystroke.modifiers.platform
            || keystroke.modifiers.alt
            || keystroke.modifiers.function
        {
            return false;
        }

        match keystroke.key.as_str() {
            "left" if !keystroke.modifiers.shift => {
                return self.collapse_smart_input_selection(
                    selected,
                    SmartSelectionEdge::Start,
                    cx,
                );
            }
            "right" if !keystroke.modifiers.shift => {
                return self.collapse_smart_input_selection(selected, SmartSelectionEdge::End, cx);
            }
            "backspace" | "delete" => {
                return self.delete_smart_input_selection(selected, cx);
            }
            _ => {}
        }

        if let Some(ch) = keystroke.key_char.as_deref()
            && ch.chars().count() == 1
            && !ch.chars().any(|c| c.is_control())
        {
            return self.replace_smart_input_selection(selected, ch, cx);
        }
        false
    }
}

type SmartInputCell = TerminalTextCell;

fn smart_input_cells(text: &str) -> Vec<SmartInputCell> {
    terminal_text_cells(text)
}

fn find_smart_input_start_col(
    line_cells: &[SmartInputCell],
    value_cells: &[SmartInputCell],
    cursor_col: usize,
) -> Option<usize> {
    if value_cells.is_empty() || value_cells.len() > line_cells.len() {
        return None;
    }
    if cursor_col >= value_cells.len() {
        let candidate = cursor_col - value_cells.len();
        if smart_input_cells_match(line_cells, value_cells, candidate) {
            return Some(candidate);
        }
    }
    let max_start = line_cells.len().saturating_sub(value_cells.len());
    (0..=max_start)
        .rev()
        .find(|start_col| smart_input_cells_match(line_cells, value_cells, *start_col))
}

fn smart_input_cells_match(
    line_cells: &[SmartInputCell],
    value_cells: &[SmartInputCell],
    start_col: usize,
) -> bool {
    line_cells
        .get(start_col..start_col + value_cells.len())
        .map(|slice| {
            slice
                .iter()
                .zip(value_cells)
                .all(|(line, value)| line.text == value.text)
        })
        .unwrap_or(false)
}

fn byte_for_smart_input_cell_index(cells: &[SmartInputCell], cell_index: usize) -> usize {
    if cell_index == 0 {
        0
    } else {
        cells
            .get(cell_index)
            .map(|cell| cell.byte_start)
            .or_else(|| cells.last().map(|cell| cell.byte_end))
            .unwrap_or(0)
    }
}

fn byte_end_for_smart_input_cell_index(cells: &[SmartInputCell], cell_index: usize) -> usize {
    if cell_index == 0 {
        0
    } else {
        cells
            .get(cell_index.saturating_sub(1))
            .map(|cell| cell.byte_end)
            .or_else(|| cells.last().map(|cell| cell.byte_end))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        byte_end_for_smart_input_cell_index, byte_for_smart_input_cell_index,
        find_smart_input_start_col, smart_input_cells,
    };

    #[test]
    fn smart_input_cells_keep_combining_mark_with_previous_cell() {
        let cells = smart_input_cells("e\u{301}x");

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "e\u{301}");
        assert_eq!(byte_for_smart_input_cell_index(&cells, 1), "e\u{301}".len());
        assert_eq!(
            byte_for_smart_input_cell_index(&cells, 2),
            "e\u{301}x".len()
        );
    }

    #[test]
    fn smart_input_start_prefers_cursor_aligned_combining_cells() {
        let line_cells = smart_input_cells("$ e\u{301}x");
        let value_cells = smart_input_cells("e\u{301}x");

        assert_eq!(
            find_smart_input_start_col(&line_cells, &value_cells, 4),
            Some(2)
        );
    }

    #[test]
    fn smart_input_cells_count_wide_char_as_two_terminal_cells() {
        let cells = smart_input_cells("界x");

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].text, "界");
        assert_eq!(cells[1].text, "界");
        assert_eq!(cells[0].byte_start, cells[1].byte_start);
        assert_eq!(cells[0].byte_end, cells[1].byte_end);
        assert_eq!(byte_for_smart_input_cell_index(&cells, 1), 0);
        assert_eq!(byte_end_for_smart_input_cell_index(&cells, 1), "界".len());
        assert_eq!(byte_for_smart_input_cell_index(&cells, 2), "界".len());
        assert_eq!(byte_for_smart_input_cell_index(&cells, 3), "界x".len());
    }

    #[test]
    fn smart_input_start_prefers_cursor_aligned_wide_cells() {
        let line_cells = smart_input_cells("$ 界x");
        let value_cells = smart_input_cells("界x");

        assert_eq!(
            find_smart_input_start_col(&line_cells, &value_cells, 5),
            Some(2)
        );
    }

    #[test]
    fn smart_input_cells_attach_combining_mark_to_all_wide_halves() {
        let cells = smart_input_cells("界\u{301}x");

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].text, "界\u{301}");
        assert_eq!(cells[1].text, "界\u{301}");
        assert_eq!(cells[0].byte_end, "界\u{301}".len());
        assert_eq!(cells[1].byte_end, "界\u{301}".len());
        assert_eq!(byte_for_smart_input_cell_index(&cells, 1), 0);
        assert_eq!(
            byte_end_for_smart_input_cell_index(&cells, 1),
            "界\u{301}".len()
        );
    }

    #[test]
    fn smart_input_cells_attach_variation_selector_to_previous_cell() {
        let cells = smart_input_cells("a\u{fe0f}x");

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "a\u{fe0f}");
        assert_eq!(cells[0].byte_end, "a\u{fe0f}".len());
        assert_eq!(
            byte_end_for_smart_input_cell_index(&cells, 1),
            "a\u{fe0f}".len()
        );
        assert_eq!(
            byte_for_smart_input_cell_index(&cells, 1),
            "a\u{fe0f}".len()
        );
    }
}
