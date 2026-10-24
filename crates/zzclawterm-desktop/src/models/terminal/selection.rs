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

pub(crate) use zzclawterm_terminal::selection::{TerminalBufferCellPos, TerminalSelection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSelectionTextError {
    MissingSession,
    StaleHistory,
    InvalidRange,
    WorkerUnavailable,
}
