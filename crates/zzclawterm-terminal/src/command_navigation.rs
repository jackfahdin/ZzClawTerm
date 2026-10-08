use crate::TerminalCore;
use alacritty_terminal::{grid::Dimensions, index::Column, term::cell::Flags};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandNavigationAction {
    Previous,
    Next,
    Select,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandNavigationResult {
    pub display_offset: usize,
    pub selection: Option<(usize, usize)>,
    pub cols: usize,
}

#[derive(Debug, Default)]
pub(crate) struct CommandNavigationState {
    position: Option<i64>,
    selection: Option<(i64, i64)>,
}

impl TerminalCore {
    pub fn reset_command_navigation(&mut self) {
        self.active_line_state_mut().command_navigation = Default::default();
    }

    pub fn record_fallback_command(&mut self) {
        if self.alternate_screen() {
            return;
        }
        let mut cursor = self.term.grid().cursor.point.line;
        while cursor > self.term.topmost_line()
            && self.term.grid()[cursor - 1usize][Column(self.cols - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            cursor -= 1usize;
        }
        let logical = self.active_line_state().logical_line(cursor);
        self.active_line_state_mut()
            .metadata
            .entry(logical)
            .or_default()
            .command_anchor = true;
    }

    pub fn navigate_command(
        &mut self,
        action: CommandNavigationAction,
    ) -> Option<CommandNavigationResult> {
        if self.alternate_screen() {
            return None;
        }
        self.retain_active_presentation_range();
        let cursor = self
            .active_line_state()
            .logical_line(self.term.grid().cursor.point.line);
        let first = self
            .active_line_state()
            .logical_line(self.term.topmost_line());
        let origin = self.active_line_state().logical_origin;
        let anchors = self
            .active_line_state()
            .metadata
            .iter()
            .filter_map(|(line, metadata)| metadata.command_anchor.then_some(*line))
            .collect::<Vec<_>>();
        let state = &mut self.active_line_state_mut().command_navigation;
        if state.position.is_some_and(|line| !anchors.contains(&line)) {
            state.position = None;
        }
        if state
            .selection
            .is_some_and(|(start, end)| !anchors.contains(&start) || !anchors.contains(&end))
        {
            state.selection = None;
        }
        let current = state.position.unwrap_or(cursor);
        let (target, selection) = match action {
            CommandNavigationAction::Previous => {
                let target = anchors.iter().copied().rev().find(|line| *line < current)?;
                state.position = Some(target);
                state.selection = None;
                (target, None)
            }
            CommandNavigationAction::Next => {
                let target = anchors
                    .iter()
                    .copied()
                    .find(|line| *line > current && *line < cursor)
                    .unwrap_or(cursor);
                if target <= current {
                    return None;
                }
                state.position = (target != cursor).then_some(target);
                state.selection = None;
                (target, None)
            }
            CommandNavigationAction::Select => {
                let start = state
                    .selection
                    .map(|(start, _)| start)
                    .or_else(|| anchors.iter().copied().rev().find(|line| *line <= current))?;
                let last = match state.selection {
                    Some((_, end)) => anchors.iter().copied().find(|line| *line > end)?,
                    None => start,
                };
                let end = anchors
                    .iter()
                    .copied()
                    .find(|line| *line > last)
                    .map_or(cursor, |line| line - 1)
                    .max(start);
                state.selection = Some((start, last));
                (
                    start,
                    Some((
                        (start - first).max(0) as usize,
                        (end - first).max(0) as usize,
                    )),
                )
            }
        };
        Some(CommandNavigationResult {
            display_offset: origin.saturating_sub(target).max(0) as usize,
            selection,
            cols: self.cols,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CommandNavigationAction;
    use crate::TerminalCore;

    #[test]
    fn shell_commands_navigate_extend_and_clear_with_the_grid() {
        let mut terminal = TerminalCore::new(20, 3);
        for _ in 0..3 {
            terminal.advance(
                b"\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07output\r\n\x1b]133;D;0\x07",
            );
        }
        let previous = terminal
            .navigate_command(CommandNavigationAction::Previous)
            .unwrap();
        let older = terminal
            .navigate_command(CommandNavigationAction::Previous)
            .unwrap();
        assert!(older.display_offset > previous.display_offset);
        let one = terminal
            .navigate_command(CommandNavigationAction::Select)
            .unwrap()
            .selection
            .unwrap();
        let two = terminal
            .navigate_command(CommandNavigationAction::Select)
            .unwrap()
            .selection
            .unwrap();
        assert_eq!(one.0, two.0);
        assert!(two.1 > one.1);
        terminal.clear_except_input();
        assert!(
            terminal
                .navigate_command(CommandNavigationAction::Previous)
                .is_none()
        );
    }
    #[test]
    fn fallback_wrap_eviction_resize_and_alternate_screen_follow_grid_lifetime() {
        let mut terminal = TerminalCore::new(8, 3);
        terminal.set_scrollback_limit(2);
        terminal.advance(b"prompt> wrapped input");
        terminal.record_fallback_command();
        terminal.advance(b"\r\nresult\r\n");
        let selection = terminal
            .navigate_command(CommandNavigationAction::Select)
            .unwrap()
            .selection
            .unwrap();
        assert_eq!(selection.0, 0);
        terminal.advance(b"\x1b[?1049h");
        assert!(
            terminal
                .navigate_command(CommandNavigationAction::Previous)
                .is_none()
        );
        terminal.advance(b"\x1b[?1049l");
        terminal.resize(16, 3);
        assert!(
            terminal
                .navigate_command(CommandNavigationAction::Previous)
                .is_none()
        );
        terminal.record_fallback_command();
        for _ in 0..10 {
            terminal.advance(b"\r\noutput");
        }
        assert!(
            terminal
                .navigate_command(CommandNavigationAction::Previous)
                .is_none()
        );
    }
}
