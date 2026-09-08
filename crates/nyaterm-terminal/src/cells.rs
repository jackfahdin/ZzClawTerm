use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTextCell {
    pub text: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

pub fn terminal_is_zero_width_mark(ch: char) -> bool {
    if UnicodeWidthChar::width(ch) == Some(0) {
        return true;
    }
    matches!(
        ch as u32,
        0x0300..=0x036f
            | 0x1ab0..=0x1aff
            | 0x1dc0..=0x1dff
            | 0x20d0..=0x20ff
            | 0xfe20..=0xfe2f
    )
}

pub fn terminal_char_cell_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0).max(1)
}

pub fn terminal_cell_count(text: &str) -> usize {
    let mut cells = 0usize;
    for ch in text.chars() {
        if terminal_is_zero_width_mark(ch) && cells > 0 {
            continue;
        }
        cells += terminal_char_cell_width(ch);
    }
    cells
}

pub fn terminal_cell_col_for_byte_index(text: &str, byte_index: usize) -> usize {
    let byte_index = byte_index.min(text.len());
    let mut cells = 0usize;
    for (idx, ch) in text.char_indices() {
        if idx >= byte_index {
            break;
        }
        if terminal_is_zero_width_mark(ch) && cells > 0 {
            continue;
        }
        cells += terminal_char_cell_width(ch);
    }
    cells
}

pub fn terminal_byte_index_for_cell_col(text: &str, cell_col: usize) -> usize {
    if cell_col == 0 {
        return 0;
    }
    let mut cells = 0usize;
    for (idx, ch) in text.char_indices() {
        if !terminal_is_zero_width_mark(ch) || cells == 0 {
            let width = terminal_char_cell_width(ch);
            if cell_col < cells.saturating_add(width) {
                return idx;
            }
            cells += width;
        }
    }
    text.len()
}

pub fn terminal_text_cells(text: &str) -> Vec<TerminalTextCell> {
    let mut cells: Vec<TerminalTextCell> = Vec::new();
    for (byte_start, ch) in text.char_indices() {
        let byte_end = byte_start + ch.len_utf8();
        if terminal_is_zero_width_mark(ch)
            && let Some(previous) = cells.last_mut()
        {
            let previous_start = previous.byte_start;
            for cell in cells
                .iter_mut()
                .rev()
                .take_while(|cell| cell.byte_start == previous_start)
            {
                cell.text.push(ch);
                cell.byte_end = byte_end;
            }
            continue;
        }
        let cell = TerminalTextCell {
            text: ch.to_string(),
            byte_start,
            byte_end,
        };
        for _ in 0..terminal_cell_count(&cell.text).max(1) {
            cells.push(cell.clone());
        }
    }
    cells
}

pub fn terminal_text_cell_slice(cells: &[TerminalTextCell], start: usize, end: usize) -> String {
    let Some(slice) = cells.get(start..end) else {
        return String::new();
    };
    let mut out = String::new();
    let mut last_range = None;
    for cell in slice {
        let range = (cell.byte_start, cell.byte_end);
        if last_range == Some(range) {
            continue;
        }
        out.push_str(&cell.text);
        last_range = Some(range);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        terminal_byte_index_for_cell_col, terminal_cell_col_for_byte_index, terminal_cell_count,
        terminal_is_zero_width_mark, terminal_text_cell_slice, terminal_text_cells,
    };

    #[test]
    fn terminal_cell_count_keeps_combining_mark_with_previous_cell() {
        assert_eq!(terminal_cell_count("e\u{301}x"), 2);
        assert_eq!(terminal_cell_count("\u{301}x"), 2);
        assert_eq!(terminal_cell_count(""), 0);
    }

    #[test]
    fn terminal_cell_count_treats_wide_char_as_two_cells() {
        assert_eq!(terminal_cell_count("界x"), 3);
        assert_eq!(terminal_cell_count("界\u{301}x"), 3);
    }

    #[test]
    fn terminal_cell_count_keeps_variation_selector_with_previous_cell() {
        assert!(terminal_is_zero_width_mark('\u{fe0f}'));
        assert_eq!(terminal_cell_count("a\u{fe0f}x"), 2);
    }

    #[test]
    fn terminal_cell_col_for_byte_index_keeps_combining_mark_with_previous_cell() {
        let text = "e\u{301}x";

        assert_eq!(terminal_cell_col_for_byte_index(text, 0), 0);
        assert_eq!(terminal_cell_col_for_byte_index(text, "e".len()), 1);
        assert_eq!(terminal_cell_col_for_byte_index(text, "e\u{301}".len()), 1);
        assert_eq!(terminal_cell_col_for_byte_index(text, text.len()), 2);
    }

    #[test]
    fn terminal_cell_col_for_byte_index_counts_wide_char_columns() {
        let text = "界x";

        assert_eq!(terminal_cell_col_for_byte_index(text, 0), 0);
        assert_eq!(terminal_cell_col_for_byte_index(text, "界".len()), 2);
        assert_eq!(terminal_cell_col_for_byte_index(text, text.len()), 3);
    }

    #[test]
    fn terminal_cell_col_for_byte_index_keeps_variation_selector_with_previous_cell() {
        let text = "a\u{fe0f}x";

        assert_eq!(terminal_cell_col_for_byte_index(text, 0), 0);
        assert_eq!(terminal_cell_col_for_byte_index(text, "a".len()), 1);
        assert_eq!(terminal_cell_col_for_byte_index(text, "a\u{fe0f}".len()), 1);
        assert_eq!(terminal_cell_col_for_byte_index(text, text.len()), 2);
    }

    #[test]
    fn terminal_byte_index_for_cell_col_skips_attached_combining_marks() {
        let text = "e\u{301}x";

        assert_eq!(terminal_byte_index_for_cell_col(text, 0), 0);
        assert_eq!(terminal_byte_index_for_cell_col(text, 1), "e\u{301}".len());
        assert_eq!(terminal_byte_index_for_cell_col(text, 2), text.len());
        assert_eq!(terminal_byte_index_for_cell_col(text, 99), text.len());
    }

    #[test]
    fn terminal_byte_index_for_cell_col_maps_wide_char_spacer_to_base() {
        let text = "界x";

        assert_eq!(terminal_byte_index_for_cell_col(text, 0), 0);
        assert_eq!(terminal_byte_index_for_cell_col(text, 1), 0);
        assert_eq!(terminal_byte_index_for_cell_col(text, 2), "界".len());
        assert_eq!(terminal_byte_index_for_cell_col(text, 3), text.len());
    }

    #[test]
    fn terminal_byte_index_for_cell_col_skips_attached_variation_selector() {
        let text = "a\u{fe0f}x";

        assert_eq!(terminal_byte_index_for_cell_col(text, 0), 0);
        assert_eq!(terminal_byte_index_for_cell_col(text, 1), "a\u{fe0f}".len());
        assert_eq!(terminal_byte_index_for_cell_col(text, 2), text.len());
    }

    #[test]
    fn terminal_text_cell_slice_deduplicates_wide_and_combining_cells() {
        let cells = terminal_text_cells("a界e\u{301}z");

        assert_eq!(terminal_text_cell_slice(&cells, 0, 1), "a");
        assert_eq!(terminal_text_cell_slice(&cells, 1, 3), "界");
        assert_eq!(terminal_text_cell_slice(&cells, 3, 4), "e\u{301}");
        assert_eq!(terminal_text_cell_slice(&cells, 4, 5), "z");
    }
}
