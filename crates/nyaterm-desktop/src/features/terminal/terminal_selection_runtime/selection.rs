use std::time::Duration;

use gpui::{ClipboardItem, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use nyaterm_terminal::TerminalSnapshot;

use crate::features::NyaTermApp;
use crate::features::terminal::LostTerminalSelectionRecovery;
use crate::features::terminal::terminal_runtime::TerminalMouseReportRequest;
use crate::features::terminal::terminal_surface::{
    terminal_absolute_line_for_snapshot_row, terminal_snapshot_absolute_range,
};
use crate::models::{
    TerminalBufferCellPos, TerminalFrameSearchKey, TerminalFrameSearchPurpose, TerminalSelection,
    TerminalViewState,
};
use crate::terminal::{
    TerminalTextCell, terminal_is_zero_width_mark, terminal_text_cell_slice, terminal_text_cells,
};

use super::metrics::{
    TerminalHitTestGeometry, terminal_cell_for_visual_geometry,
    terminal_snapshot_row_for_visual_geometry,
};

const TERMINAL_SELECTION_DRAG_NOTIFY_DELAY: Duration = Duration::from_millis(8);
const TERMINAL_SELECTED_OCCURRENCE_DEBOUNCE: Duration = Duration::from_millis(100);
const TERMINAL_SELECTED_OCCURRENCE_LIMIT: usize = 2000;
const TERMINAL_SELECTED_OCCURRENCE_MAX_CHARS: usize = 256;

impl NyaTermApp {
    pub(in crate::features) fn clear_terminal_selection_state_for_session(
        &mut self,
        session_id: &str,
    ) {
        // Occurrence results can outlive the visible selection while a session
        // is being closed; clear that owner independently of selection state.
        self.clear_terminal_selected_occurrence_for_session(session_id);
        let selection_session_id = self
            .terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id());
        if selection_session_id != Some(session_id) {
            return;
        }
        self.terminal.selection.selection = None;
        self.terminal.selection.session_id = None;
        self.terminal.selection.dragging = false;
        self.clear_terminal_selected_occurrence_for_session(session_id);
    }

    fn notify_terminal_selection_owner_surface(&mut self, cx: &mut Context<Self>) {
        let session_id = self
            .terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id())
            .map(str::to_string);
        if let Some(session_id) = session_id.filter(|session_id| !session_id.is_empty()) {
            self.notify_terminal_selection_visual_only(session_id.as_str(), cx);
        }
    }

    pub(in crate::features) fn clear_terminal_selection(&mut self, cx: &mut Context<Self>) {
        let previous_session_id = self
            .terminal
            .selection
            .session_id
            .clone()
            .or_else(|| self.session.active_id_owned());
        if self.terminal.selection.selection.is_some() || self.terminal.selection.dragging {
            self.terminal.selection.selection = None;
            self.terminal.selection.session_id = None;
            self.terminal.selection.dragging = false;
            self.clear_terminal_selected_occurrence(cx);
            if let Some(previous_session_id) =
                previous_session_id.filter(|session_id| !session_id.is_empty())
            {
                self.notify_terminal_selection_visual_only(previous_session_id.as_str(), cx);
            }
        }
    }

    pub(in crate::features) fn select_all_terminal(&mut self, cx: &mut Context<Self>) {
        let (_, cols) = self.active_terminal_grid_size();
        if cols == 0 {
            self.terminal.selection.selection = None;
            self.terminal.selection.session_id = None;
            self.clear_terminal_selected_occurrence(cx);
            self.notify_terminal_selection_owner_surface(cx);
            return;
        }
        self.clear_terminal_selected_occurrence(cx);
        self.terminal.selection.selection = Some(TerminalSelection::all_buffer(cols));
        self.terminal.selection.session_id = self.session.active_id_owned();
        self.terminal.selection.dragging = false;
        self.shell
            .set_status("selected all terminal text".to_string());
        self.notify_terminal_selection_owner_surface(cx);
        cx.notify();
    }

    pub(in crate::features) fn selected_terminal_text(&self) -> Option<String> {
        let selection = self.terminal.selection.selection.as_ref()?;
        let session_id = self
            .terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id());
        if selection.is_empty() {
            return None;
        }
        if let Some(view) =
            session_id.and_then(|session_id| self.terminal.view.views.get(session_id))
        {
            return terminal_selected_text_for_view(view, *selection);
        }
        terminal_selected_text_from_lines(*selection, &self.terminal.view.screen.all_lines(), 0)
    }

    pub(in crate::features) fn copy_terminal_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.selected_terminal_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.shell
            .set_status("copied terminal selection".to_string());
        self.notify_terminal_selection_owner_surface(cx);
        self.terminal.menus.actions_open = false;
        cx.notify();
        true
    }

    pub(in crate::features) fn copy_terminal_selection_or_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.copy_terminal_selection(cx) {
            return;
        }
        self.copy_terminal_visible_text(cx);
    }

    pub(in crate::features) fn start_terminal_selection_for_session(
        &mut self,
        session_id: Option<&str>,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let selection_session_id = session_id
            .filter(|session_id| !session_id.is_empty())
            .map(str::to_string)
            .or_else(|| {
                self.session
                    .active_id_owned()
                    .filter(|session_id| !session_id.is_empty())
            });
        let Some(geometry) =
            self.terminal_hit_test_geometry_for_session(selection_session_id.as_deref(), cx)
        else {
            return;
        };
        let previous_selection_session_id = self.terminal.selection.session_id.clone();
        if self.terminal.selection.selection.is_some()
            && previous_selection_session_id.as_deref() != selection_session_id.as_deref()
        {
            self.terminal.selection.selection = None;
            self.terminal.selection.session_id = None;
            self.terminal.selection.dragging = false;
            if let Some(previous_selection_session_id) =
                previous_selection_session_id.filter(|session_id| !session_id.is_empty())
            {
                self.notify_terminal_selection_visual_only(
                    previous_selection_session_id.as_str(),
                    cx,
                );
            }
        }
        // A new selection invalidates the previous occurrence query immediately,
        // including while a double/triple-click selection is being formed.
        self.clear_terminal_selected_occurrence(cx);
        let cell = terminal_cell_for_visual_geometry(event.position, &geometry);
        let Some(buffer_cell) =
            Self::terminal_buffer_cell_for_visual_geometry(event.position, &geometry)
        else {
            return;
        };
        // Applications with mouse tracking (vim/less/tmux) consume left presses.
        if let Some(session_id) = selection_session_id.as_deref()
            && self.maybe_send_mouse_report_for_session(
                TerminalMouseReportRequest {
                    session_id,
                    button: 0,
                    col: cell.col as u16,
                    row: cell.row as u16,
                    press: true,
                    motion: false,
                    modifiers: event.modifiers,
                },
                cx,
            )
        {
            self.clear_terminal_selection(cx);
            return;
        }
        if !event.modifiers.modified()
            && let Some(session_id) = selection_session_id.as_deref()
        {
            let snapshot_row = terminal_snapshot_row_for_visual_geometry(event.position, &geometry);
            let target_line = geometry
                .snapshot
                .row(snapshot_row)
                .and_then(|row| row.line_id);
            if let Some(target_line) = target_line
                && let Some(view) = self.terminal.view.views.get_mut(session_id)
                && view.target_line != Some(target_line)
            {
                view.target_line = Some(target_line);
                self.notify_terminal_surface_only(Some(session_id), cx);
            }
        }
        let cols = geometry.cols;
        // Shift+click extends the existing selection from its anchor (xterm-style).
        if event.modifiers.shift
            && event.click_count <= 1
            && let Some(selection) = self.terminal.selection.selection.as_mut()
        {
            selection.head = buffer_cell;
            if self.terminal.selection.session_id.is_none() {
                self.terminal.selection.session_id = selection_session_id;
            }
            self.terminal.selection.dragging = true;
            // Defer status-bar shell notify until selection finishes.
            self.notify_terminal_selection_owner_surface(cx);
            return;
        }
        if event.click_count >= 3 {
            self.terminal.selection.selection = Some(TerminalSelection::from_range(
                TerminalBufferCellPos::new(buffer_cell.line, 0),
                TerminalBufferCellPos::new(buffer_cell.line, cols.saturating_sub(1)),
            ));
            self.terminal.selection.session_id = selection_session_id;
            self.terminal.selection.dragging = false;
            self.shell
                .set_status(format!("selected line {}", cell.row + 1));
            self.notify_terminal_selection_owner_surface(cx);
            // Discrete click: status bar update is fine (not a high-frequency path).
            cx.notify();
            return;
        }
        if event.click_count == 2 {
            let word = self.word_bounds_at_for_visual_geometry(event.position, &geometry);
            self.terminal.selection.selection = Some(TerminalSelection::from_range(
                TerminalBufferCellPos::new(buffer_cell.line, word.0),
                TerminalBufferCellPos::new(buffer_cell.line, word.1.saturating_sub(1).max(word.0)),
            ));
            self.terminal.selection.session_id = selection_session_id;
            self.terminal.selection.dragging = false;
            self.shell.set_status("selected word".to_string());
            self.notify_terminal_selection_owner_surface(cx);
            cx.notify();
            return;
        }
        self.terminal.selection.selection = Some(TerminalSelection::with_anchor(buffer_cell));
        self.terminal.selection.session_id = selection_session_id;
        self.terminal.selection.dragging = true;
        self.notify_terminal_selection_owner_surface(cx);
    }

    fn clear_terminal_selected_occurrence_for_session(&mut self, session_id: &str) {
        if self
            .terminal
            .selection
            .selected_occurrence
            .session_id
            .as_deref()
            != Some(session_id)
        {
            return;
        }
        self.terminal.selection.selected_occurrence.session_id = None;
        self.terminal.selection.selected_occurrence.query = None;
        self.terminal.selection.selected_occurrence.generation = self
            .terminal
            .selection
            .selected_occurrence
            .generation
            .saturating_add(1);
        if let Some(view) = self.terminal.view.views.get_mut(session_id) {
            view.selected_occurrence_result = None;
            view.pending_selected_occurrence_key = None;
            view.selected_occurrence_visible_result = None;
            view.pending_selected_occurrence_visible_key = None;
        }
    }

    fn clear_terminal_selected_occurrence(&mut self, cx: &mut Context<Self>) {
        let session_id = self
            .terminal
            .selection
            .selected_occurrence
            .session_id
            .clone();
        self.terminal.selection.selected_occurrence.session_id = None;
        self.terminal.selection.selected_occurrence.query = None;
        self.terminal.selection.selected_occurrence.generation = self
            .terminal
            .selection
            .selected_occurrence
            .generation
            .saturating_add(1);
        if let Some(session_id) = session_id {
            if let Some(view) = self.terminal.view.views.get_mut(&session_id) {
                view.selected_occurrence_result = None;
                view.pending_selected_occurrence_key = None;
                view.selected_occurrence_visible_result = None;
                view.pending_selected_occurrence_visible_key = None;
            }
            self.notify_terminal_surface_only(Some(session_id.as_str()), cx);
        }
    }

    pub(in crate::features) fn update_terminal_selection_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(button) = self.terminal.selection.mouse_report_button {
            let captured_session_id = self
                .terminal
                .selection
                .mouse_report_session_id
                .clone()
                .or_else(|| self.session.active_id_owned());
            if let Some(session_id) = captured_session_id
                && let Some(cell) = self.point_to_terminal_cell_for_session(
                    Some(session_id.as_str()),
                    event.position,
                    cx,
                )
                && self.maybe_send_mouse_report_for_session(
                    TerminalMouseReportRequest {
                        session_id: &session_id,
                        button,
                        col: cell.col as u16,
                        row: cell.row as u16,
                        press: true,
                        motion: true,
                        modifiers: event.modifiers,
                    },
                    cx,
                )
            {
                return;
            }
        }
        if !self.terminal.selection.dragging {
            return;
        }
        let selection_session_id = self
            .terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id())
            .filter(|session_id| !session_id.is_empty());
        let Some(geometry) = self.terminal_hit_test_geometry_for_session(selection_session_id, cx)
        else {
            return;
        };
        let Some(buffer_cell) =
            Self::terminal_buffer_cell_for_visual_geometry(event.position, &geometry)
        else {
            return;
        };
        if let Some(selection) = self.terminal.selection.selection.as_mut()
            && selection.head != buffer_cell
        {
            selection.head = buffer_cell;
            if self.terminal.selection.session_id.is_none() {
                self.terminal.selection.session_id = selection_session_id.map(str::to_string);
            }
            self.queue_terminal_selection_drag_visual_notify(cx);
        }
    }

    pub(in crate::features) fn finish_terminal_selection(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        if self.finish_terminal_mouse_report(event, cx) {
            self.clear_terminal_selection(cx);
            return;
        }
        if !self.terminal.selection.dragging {
            if self
                .terminal
                .selection
                .selection
                .as_ref()
                .is_some_and(|selection| !selection.is_empty())
            {
                if self.settings.summary().interaction_copy_on_select {
                    let _ = self.copy_terminal_selection(cx);
                }
                // Double/triple-click selections are committed on MouseDown,
                // but occurrence search remains a MouseUp-only operation.
                self.schedule_terminal_selected_occurrence_search(cx);
                self.notify_terminal_selection_owner_surface(cx);
            }
            // Stationary click without an active drag can still reposition the
            // tracked input cursor (Tauri handleTerminalMouseUp smart cursor).
            if self.terminal.selection.selection.is_none() {
                self.handle_smart_input_click(event, cx);
            }
            return;
        }
        let selection_session_id = self
            .terminal
            .selection
            .session_id
            .as_deref()
            .or(self.session.active_id())
            .filter(|session_id| !session_id.is_empty());
        if let Some(geometry) =
            self.terminal_hit_test_geometry_for_session(selection_session_id, cx)
        {
            let buffer_cell =
                Self::terminal_buffer_cell_for_visual_geometry(event.position, &geometry);
            if let Some(selection) = self.terminal.selection.selection.as_mut()
                && let Some(buffer_cell) = buffer_cell
            {
                selection.head = buffer_cell;
            }
        }
        self.terminal.selection.dragging = false;
        if self
            .terminal
            .selection
            .selection
            .as_ref()
            .is_some_and(|selection| selection.is_empty())
        {
            self.terminal.selection.selection = None;
            self.clear_terminal_selected_occurrence(cx);
            // Empty selection after click: try smart input cursor move.
            self.handle_smart_input_click(event, cx);
        } else if let Some(selected) = self.smart_cursor_selected_input_range() {
            // Collapse caret toward click/edge, then clear selection (Tauri path).
            let target = if event.click_count >= 2 {
                selected.end
            } else if let Some(index) = self.input_index_at_mouse(event.position, cx) {
                index.clamp(selected.start, selected.end)
            } else {
                selected.end
            };
            if self.settings.summary().interaction_copy_on_select {
                let _ = self.copy_terminal_selection(cx);
            }
            let _ = self.move_smart_input_cursor(target, cx);
            self.clear_terminal_selection(cx);
        } else if self.settings.summary().interaction_copy_on_select {
            let _ = self.copy_terminal_selection(cx);
        } else if self.terminal.selection.selection.is_some() {
            // One shell notify for status after drag ends (not per mouse move).
            self.shell.set_status("selection ready".to_string());
            cx.notify();
        }
        self.schedule_terminal_selected_occurrence_search(cx);
        self.notify_terminal_selection_owner_surface(cx);
    }

    pub(in crate::features) fn recover_terminal_selection_after_lost_mouse_up(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let previous_session_id = self
            .terminal
            .selection
            .session_id
            .clone()
            .or_else(|| self.session.active_id_owned());
        match self.terminal.recover_lost_selection_mouse_up() {
            LostTerminalSelectionRecovery::None => {}
            LostTerminalSelectionRecovery::ClearedEmpty => {
                self.clear_terminal_selected_occurrence(cx);
                if let Some(session_id) =
                    previous_session_id.filter(|session_id| !session_id.is_empty())
                {
                    self.notify_terminal_selection_visual_only(session_id.as_str(), cx);
                }
            }
            LostTerminalSelectionRecovery::Committed => {
                if self.settings.summary().interaction_copy_on_select {
                    let _ = self.copy_terminal_selection(cx);
                } else {
                    self.shell.set_status("selection ready".to_string());
                    cx.notify();
                }
                self.schedule_terminal_selected_occurrence_search(cx);
                self.notify_terminal_selection_owner_surface(cx);
            }
        }
    }

    fn schedule_terminal_selected_occurrence_search(&mut self, cx: &mut Context<Self>) {
        let session_id = self
            .terminal
            .selection
            .session_id
            .clone()
            .or_else(|| self.session.active_id_owned());
        let Some(session_id) = session_id.filter(|id| !id.is_empty()) else {
            self.clear_terminal_selected_occurrence(cx);
            return;
        };
        let query = self
            .selected_terminal_text()
            .and_then(|text| terminal_selected_occurrence_query(&text));
        let Some(query) = query else {
            self.clear_terminal_selected_occurrence(cx);
            return;
        };
        self.terminal.selection.selected_occurrence.session_id = Some(session_id.clone());
        self.terminal.selection.selected_occurrence.query = Some(query.clone());
        self.terminal.selection.selected_occurrence.generation = self
            .terminal
            .selection
            .selected_occurrence
            .generation
            .saturating_add(1);
        let generation = self.terminal.selection.selected_occurrence.generation;
        let selection = self.terminal.selection.selection;
        let (absolute_start, absolute_end) = self
            .terminal
            .view
            .views
            .get(&session_id)
            .and_then(|view| {
                selection
                    .and_then(|selection| terminal_snapshot_covering_selection(view, selection))
                    .map(terminal_snapshot_absolute_range)
            })
            .or_else(|| {
                self.terminal
                    .view
                    .views
                    .get(&session_id)
                    .map(|view| view.screen.viewport_absolute_range(view.scroll_offset))
            })
            .unwrap_or((0, 0));
        let visible_key = TerminalFrameSearchKey {
            query: query.clone(),
            case_sensitive: true,
            regex: false,
            whole_word: false,
            limit: TERMINAL_SELECTED_OCCURRENCE_LIMIT,
            request_generation: generation,
        };
        let _ = self.request_terminal_frame_search(
            &session_id,
            TerminalFrameSearchPurpose::SelectedOccurrenceVisible {
                absolute_start,
                absolute_end,
            },
            visible_key,
        );
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TERMINAL_SELECTED_OCCURRENCE_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, _cx| {
                if this.terminal.selection.selected_occurrence.generation != generation
                    || this
                        .terminal
                        .selection
                        .selected_occurrence
                        .session_id
                        .as_deref()
                        != Some(session_id.as_str())
                    || this.terminal.selection.selected_occurrence.query.as_deref()
                        != Some(query.as_str())
                {
                    return;
                }
                let key = TerminalFrameSearchKey {
                    query: query.clone(),
                    case_sensitive: true,
                    regex: false,
                    whole_word: false,
                    limit: TERMINAL_SELECTED_OCCURRENCE_LIMIT,
                    request_generation: generation,
                };
                let _ = this.request_terminal_frame_search(
                    &session_id,
                    TerminalFrameSearchPurpose::SelectedOccurrence,
                    key,
                );
            });
        })
        .detach();
    }

    fn queue_terminal_selection_drag_visual_notify(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self
            .terminal
            .selection
            .session_id
            .clone()
            .or_else(|| self.session.active_id_owned())
        else {
            return;
        };
        if session_id.is_empty() {
            return;
        }
        if !self.shell.queue_terminal_selection_drag(session_id) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TERMINAL_SELECTION_DRAG_NOTIFY_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.flush_terminal_selection_drag_visual_notify(cx);
            });
        })
        .detach();
    }

    fn flush_terminal_selection_drag_visual_notify(&mut self, cx: &mut Context<Self>) {
        let session_ids = self.shell.drain_terminal_selection_drag_sessions();
        for session_id in session_ids {
            self.notify_terminal_selection_visual_only(session_id.as_str(), cx);
        }
    }

    fn word_bounds_at_for_visual_geometry(
        &self,
        position: gpui::Point<gpui::Pixels>,
        geometry: &TerminalHitTestGeometry,
    ) -> (usize, usize) {
        terminal_word_bounds_for_visual_geometry(
            position,
            geometry,
            self.settings.summary().interaction_word_separators.as_str(),
        )
    }

    fn terminal_buffer_cell_for_visual_geometry(
        position: gpui::Point<gpui::Pixels>,
        geometry: &TerminalHitTestGeometry,
    ) -> Option<TerminalBufferCellPos> {
        let snapshot_row = terminal_snapshot_row_for_visual_geometry(position, geometry)
            .min(geometry.snapshot_rows.saturating_sub(1));
        let absolute_line =
            terminal_absolute_line_for_snapshot_row(geometry.snapshot.as_ref(), snapshot_row)?;
        let viewport_cell = terminal_cell_for_visual_geometry(position, geometry);
        Some(TerminalBufferCellPos::new(absolute_line, viewport_cell.col))
    }
}

fn terminal_snapshot_covering_selection(
    view: &TerminalViewState,
    selection: TerminalSelection,
) -> Option<&TerminalSnapshot> {
    if selection.all_buffer || selection.is_empty() {
        return None;
    }
    let (selection_start, selection_end) = selection.ordered();
    view.frame_snapshot
        .iter()
        .chain(view.scrollback_snapshots.values())
        .map(AsRef::as_ref)
        .filter(|snapshot| {
            let (snapshot_start, snapshot_end) = terminal_snapshot_absolute_range(snapshot);
            snapshot_start <= selection_start.line && selection_end.line < snapshot_end
        })
        .min_by_key(|snapshot| snapshot.display_offset.abs_diff(view.scroll_offset))
}

fn terminal_selected_text_for_view(
    view: &TerminalViewState,
    selection: TerminalSelection,
) -> Option<String> {
    if !selection.all_buffer
        && let Some(snapshot) = terminal_snapshot_covering_selection(view, selection)
    {
        let (absolute_start, _) = terminal_snapshot_absolute_range(snapshot);
        return terminal_selected_text_from_line_source(
            selection,
            snapshot.row_count(),
            absolute_start,
            |index| snapshot.line(index),
        );
    }
    terminal_selected_text_from_lines(selection, &view.screen.all_lines(), 0)
}

fn terminal_selected_text_from_lines(
    selection: TerminalSelection,
    lines: &[String],
    absolute_start: usize,
) -> Option<String> {
    terminal_selected_text_from_line_source(selection, lines.len(), absolute_start, |index| {
        lines.get(index).map(String::as_str)
    })
}

fn terminal_selected_text_from_line_source<'a>(
    selection: TerminalSelection,
    line_count: usize,
    absolute_start: usize,
    mut line_at: impl FnMut(usize) -> Option<&'a str>,
) -> Option<String> {
    if selection.all_buffer {
        let mut text = String::new();
        for index in 0..line_count {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(line_at(index).unwrap_or_default().trim_end());
        }
        while text.ends_with('\n') {
            text.pop();
        }
        return (!text.is_empty()).then_some(text);
    }
    let (start, end) = selection.ordered();
    let absolute_end = absolute_start.saturating_add(line_count);
    if start.line < absolute_start || end.line >= absolute_end {
        return None;
    }
    let mut text = String::new();
    let mut first_line = true;
    for line_index in start.line..=end.line {
        let line = line_at(line_index - absolute_start)?;
        let cells = terminal_text_cells(line);
        let (col_start, col_end_excl) = selection.cols_for_absolute_line(line_index)?;
        let col_end = col_end_excl.min(cells.len().max(col_start));
        let col_start = col_start.min(col_end);
        let slice = terminal_text_cell_slice(&cells, col_start, col_end);
        if !first_line {
            text.push('\n');
        }
        first_line = false;
        text.push_str(slice.trim_end());
    }
    if text.is_empty() { None } else { Some(text) }
}

fn terminal_word_bounds_for_visual_geometry(
    position: gpui::Point<gpui::Pixels>,
    geometry: &TerminalHitTestGeometry,
    separators: &str,
) -> (usize, usize) {
    let snapshot_row = terminal_snapshot_row_for_visual_geometry(position, geometry)
        .min(geometry.snapshot_rows.saturating_sub(1));
    let col = terminal_cell_for_visual_geometry(position, geometry).col;
    terminal_word_bounds_at_snapshot_row(col, geometry.snapshot.as_ref(), snapshot_row, separators)
}

fn terminal_word_bounds_at_snapshot_row(
    col: usize,
    snapshot: &TerminalSnapshot,
    snapshot_row: usize,
    separators: &str,
) -> (usize, usize) {
    let line = snapshot.line(snapshot_row).unwrap_or("");
    let cells = terminal_text_cells(line);
    if cells.is_empty() {
        return (col, col.saturating_add(1));
    }
    let idx = col.min(cells.len().saturating_sub(1));
    // xterm wordSeparator semantics: characters listed are separators, not word body.
    let is_word = |cell: &TerminalTextCell| terminal_text_cell_is_word(cell, separators);
    if !is_word(&cells[idx]) {
        return (idx, idx.saturating_add(1));
    }
    let mut start = idx;
    while start > 0 && is_word(&cells[start - 1]) {
        start -= 1;
    }
    let mut end = idx + 1;
    while end < cells.len() && is_word(&cells[end]) {
        end += 1;
    }
    (start, end)
}

fn terminal_text_cell_is_word(cell: &TerminalTextCell, separators: &str) -> bool {
    cell.text
        .chars()
        .find(|ch| !terminal_is_zero_width_mark(*ch))
        .is_some_and(|ch| !separators.contains(ch))
}

#[cfg(test)]
fn terminal_all_lines_text(lines: Vec<String>) -> Option<String> {
    let text = lines
        .into_iter()
        .map(|line| line.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim_end_matches('\n').to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn terminal_selected_occurrence_query(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains('\n') {
        return None;
    }
    let char_count = text.chars().count();
    if !(2..=TERMINAL_SELECTED_OCCURRENCE_MAX_CHARS).contains(&char_count) {
        return None;
    }
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use gpui::{Point, Size, px};
    use nyaterm_terminal::TerminalScreen;

    use super::super::metrics::TerminalHitTestGeometry;
    use crate::features::terminal::terminal_surface::terminal_snapshot_absolute_range;
    use crate::models::{TerminalBufferCellPos, TerminalSelection, TerminalViewState};
    use crate::terminal::{TerminalTextCell, terminal_text_cell_slice, terminal_text_cells};

    use super::{
        TERMINAL_SELECTED_OCCURRENCE_MAX_CHARS, TERMINAL_SELECTION_DRAG_NOTIFY_DELAY,
        terminal_all_lines_text, terminal_selected_occurrence_query,
        terminal_selected_text_for_view, terminal_snapshot_covering_selection,
        terminal_text_cell_is_word, terminal_word_bounds_for_visual_geometry,
    };

    #[test]
    fn terminal_selection_drag_notify_delay_is_frame_coalesced() {
        assert_eq!(
            TERMINAL_SELECTION_DRAG_NOTIFY_DELAY,
            Duration::from_millis(8)
        );
    }

    #[test]
    fn selected_text_uses_worker_snapshot_when_legacy_screen_has_no_output() {
        let mut worker_screen = TerminalScreen::new(40, 3);
        worker_screen
            .advance(b"IPv4 address for br0\r\nIPv6 address for br0\r\nIPv6 address for br1");
        let snapshot = Arc::new(worker_screen.viewport_snapshot(0));
        let (absolute_start, absolute_end) = terminal_snapshot_absolute_range(&snapshot);
        let selection = TerminalSelection::from_range(
            TerminalBufferCellPos::new(absolute_start + 2, 5),
            TerminalBufferCellPos::new(absolute_start + 2, 11),
        );
        let mut view = TerminalViewState::new();
        view.frame_snapshot = Some(snapshot);

        assert!(
            !view
                .screen
                .all_lines()
                .iter()
                .any(|line| line.contains("address"))
        );
        assert_eq!(
            terminal_selected_text_for_view(&view, selection).as_deref(),
            Some("address")
        );
        assert_eq!(
            terminal_snapshot_covering_selection(&view, selection)
                .map(terminal_snapshot_absolute_range),
            Some((absolute_start, absolute_end))
        );
    }

    #[test]
    fn terminal_text_cells_keep_combining_mark_with_previous_cell() {
        let cells = terminal_text_cells("e\u{301}x");

        assert_eq!(
            cells,
            vec![
                TerminalTextCell {
                    text: "e\u{301}".to_string(),
                    byte_start: 0,
                    byte_end: "e\u{301}".len(),
                },
                TerminalTextCell {
                    text: "x".to_string(),
                    byte_start: "e\u{301}".len(),
                    byte_end: "e\u{301}x".len(),
                },
            ]
        );
        assert_eq!(terminal_text_cell_slice(&cells, 0, 1), "e\u{301}");
        assert_eq!(terminal_text_cell_slice(&cells, 1, 2), "x");
    }

    #[test]
    fn terminal_text_word_cells_use_base_character_for_separators() {
        let cells = terminal_text_cells("e\u{301}/x");

        assert!(terminal_text_cell_is_word(&cells[0], "/"));
        assert!(!terminal_text_cell_is_word(&cells[1], "/"));
        assert!(terminal_text_cell_is_word(&cells[2], "/"));
    }

    #[test]
    fn double_click_word_bounds_use_the_fractionally_painted_snapshot_row() {
        let mut screen = TerminalScreen::new(20, 3);
        screen.advance(b"wrong\r\ntarget word");
        let snapshot = Arc::new(screen.viewport_snapshot(0));
        let geometry = TerminalHitTestGeometry {
            bounds: gpui::bounds(
                Point {
                    x: px(0.0),
                    y: px(0.0),
                },
                Size {
                    width: px(160.0),
                    height: px(48.0),
                },
            ),
            snapshot: snapshot.clone(),
            cell_w: 8.0,
            cell_h: 16.0,
            padding_left: 0.0,
            padding_top: 0.0,
            gutter: 0.0,
            rows: 3,
            cols: snapshot.cols,
            display_offset: 0,
            viewport_anchor_row: 0,
            snapshot_rows: snapshot.row_count(),
            viewport_rows: 3,
            visual_y_offset: -8.0,
        };
        let position = Point {
            x: px(2.5 * geometry.cell_w),
            y: px(geometry.visual_y_offset + geometry.cell_h * 1.5),
        };

        assert_eq!(
            terminal_word_bounds_for_visual_geometry(position, &geometry, " /"),
            (0, 6)
        );
    }

    #[test]
    fn terminal_text_cells_count_wide_char_as_two_terminal_cells() {
        let cells = terminal_text_cells("界x");

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].text, "界");
        assert_eq!(cells[1].text, "界");
        assert_eq!(cells[0].byte_start, cells[1].byte_start);
        assert_eq!(cells[0].byte_end, cells[1].byte_end);
        assert_eq!(terminal_text_cell_slice(&cells, 0, 1), "界");
        assert_eq!(terminal_text_cell_slice(&cells, 1, 2), "界");
        assert_eq!(terminal_text_cell_slice(&cells, 0, 2), "界");
        assert_eq!(terminal_text_cell_slice(&cells, 2, 3), "x");
    }

    #[test]
    fn terminal_text_cells_attach_combining_mark_to_all_wide_halves() {
        let text = "界\u{301}x";
        let cells = terminal_text_cells(text);

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].text, "界\u{301}");
        assert_eq!(cells[1].text, "界\u{301}");
        assert_eq!(cells[0].byte_end, "界\u{301}".len());
        assert_eq!(cells[1].byte_end, "界\u{301}".len());
        assert_eq!(terminal_text_cell_slice(&cells, 0, 2), "界\u{301}");
        assert_eq!(terminal_text_cell_slice(&cells, 1, 2), "界\u{301}");
    }

    #[test]
    fn terminal_text_cells_attach_variation_selector_to_previous_cell() {
        let text = "a\u{fe0f}x";
        let cells = terminal_text_cells(text);

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "a\u{fe0f}");
        assert_eq!(cells[0].byte_end, "a\u{fe0f}".len());
        assert_eq!(terminal_text_cell_slice(&cells, 0, 1), "a\u{fe0f}");
        assert_eq!(terminal_text_cell_slice(&cells, 1, 2), "x");
    }

    #[test]
    fn terminal_all_lines_text_preserves_internal_blank_lines() {
        assert_eq!(
            terminal_all_lines_text(vec![
                "first  ".to_string(),
                String::new(),
                "last".to_string(),
                String::new(),
            ]),
            Some("first\n\nlast".to_string())
        );
    }

    #[test]
    fn terminal_selected_occurrence_query_filters_short_multiline_and_long_text() {
        assert_eq!(
            terminal_selected_occurrence_query(" ab "),
            Some("ab".to_string())
        );
        assert_eq!(terminal_selected_occurrence_query("a"), None);
        assert_eq!(terminal_selected_occurrence_query("a\nb"), None);
        assert_eq!(terminal_selected_occurrence_query("   "), None);
        assert_eq!(
            terminal_selected_occurrence_query(
                &"x".repeat(TERMINAL_SELECTED_OCCURRENCE_MAX_CHARS + 1)
            ),
            None
        );
    }
}
