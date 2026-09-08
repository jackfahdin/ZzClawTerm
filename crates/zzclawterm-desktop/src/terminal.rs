pub(crate) use zzclawterm_terminal::{
    TerminalTextCell, terminal_byte_index_for_cell_col, terminal_is_zero_width_mark,
    terminal_text_cell_slice, terminal_text_cells,
};
pub(crate) use zzclawterm_terminal_gpui::{
    TerminalBufferMatch, TerminalGridSelection, TerminalKeyMode, TerminalKeywordHighlightSnapshot,
    TerminalKeywordHighlighter, TerminalLineDecorations, ZzClawTerminalElement,
    ZzClawTerminalLayoutCache, compile_terminal_keyword_highlighter,
    precompute_terminal_keyword_highlights_for_rows_with_stats_and_cancel, terminal_font_features,
    terminal_key_bytes_with_mode, terminal_key_release_bytes_with_mode,
    terminal_keyword_highlight_expanded_rows, terminal_keyword_rules_key,
    terminal_screen_from_output,
};

pub(crate) const INITIAL_TERMINAL_BANNER: &str = "$ zzclawterm --native\nGPUI shell initialized.\nStart a local terminal or open a saved connection.\n";

pub(crate) fn initial_terminal_screen() -> zzclawterm_terminal::TerminalScreen {
    zzclawterm_terminal_gpui::initial_terminal_screen(INITIAL_TERMINAL_BANNER)
}
