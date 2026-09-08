//! Terminal cell coordinates and absolute-buffer selection state.

/// Inclusive cell coordinate inside the visible terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TerminalCellPos {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

impl TerminalCellPos {
    pub(crate) fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

/// Absolute-buffer text selection (start/end are inclusive cell positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TerminalBufferCellPos {
    pub(crate) line: usize,
    pub(crate) col: usize,
}

impl TerminalBufferCellPos {
    pub(crate) fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TerminalSelection {
    pub(crate) anchor: TerminalBufferCellPos,
    pub(crate) head: TerminalBufferCellPos,
    pub(crate) all_buffer: bool,
}

impl TerminalSelection {
    pub(crate) fn with_anchor(anchor: TerminalBufferCellPos) -> Self {
        Self {
            anchor,
            head: anchor,
            all_buffer: false,
        }
    }

    pub(crate) fn from_range(anchor: TerminalBufferCellPos, head: TerminalBufferCellPos) -> Self {
        Self {
            anchor,
            head,
            all_buffer: false,
        }
    }

    pub(crate) fn all_buffer(cols: usize) -> Self {
        Self {
            anchor: TerminalBufferCellPos::new(0, 0),
            head: TerminalBufferCellPos::new(0, cols.saturating_sub(1)),
            all_buffer: true,
        }
    }

    pub(crate) fn ordered(&self) -> (TerminalBufferCellPos, TerminalBufferCellPos) {
        let a = self.anchor;
        let b = self.head;
        if (a.line, a.col) <= (b.line, b.col) {
            (a, b)
        } else {
            (b, a)
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.all_buffer && self.anchor == self.head
    }

    /// Column range [start, end) for a painted line, if any cells are selected.
    /// Endpoints are inclusive cell positions; returned range is half-open for slicing.
    pub(crate) fn cols_for_absolute_line(&self, line: usize) -> Option<(usize, usize)> {
        if self.all_buffer {
            return Some((0, usize::MAX));
        }
        if self.is_empty() {
            return None;
        }
        let (start, end) = self.ordered();
        if line < start.line || line > end.line {
            return None;
        }
        if start.line == end.line {
            return Some((start.col, end.col.saturating_add(1)));
        }
        if line == start.line {
            return Some((start.col, usize::MAX));
        }
        if line == end.line {
            return Some((0, end.col.saturating_add(1)));
        }
        Some((0, usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalBufferCellPos, TerminalSelection};

    #[test]
    fn all_buffer_selection_covers_every_viewport_row() {
        let selection = TerminalSelection::all_buffer(80);

        assert!(!selection.is_empty());
        assert_eq!(selection.cols_for_absolute_line(0), Some((0, usize::MAX)));
        assert_eq!(
            selection.cols_for_absolute_line(10_000),
            Some((0, usize::MAX))
        );
    }

    #[test]
    fn reverse_multiline_selection_keeps_half_open_absolute_line_ranges() {
        let selection = TerminalSelection::from_range(
            TerminalBufferCellPos::new(12, 4),
            TerminalBufferCellPos::new(10, 2),
        );

        assert_eq!(
            selection.ordered(),
            (
                TerminalBufferCellPos::new(10, 2),
                TerminalBufferCellPos::new(12, 4)
            )
        );
        assert_eq!(selection.cols_for_absolute_line(9), None);
        assert_eq!(selection.cols_for_absolute_line(10), Some((2, usize::MAX)));
        assert_eq!(selection.cols_for_absolute_line(11), Some((0, usize::MAX)));
        assert_eq!(selection.cols_for_absolute_line(12), Some((0, 5)));
        assert_eq!(selection.cols_for_absolute_line(13), None);
    }
}
