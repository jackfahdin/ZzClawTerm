//! Cursor-aware editing for a single-line text field.
//!
//! Kept UI-independent so the rules that are easy to get subtly wrong — caret
//! motion across grapheme-unfriendly byte offsets, what a word boundary is,
//! which end of a selection moves when shift is held — are testable without a
//! window. The GPUI widget in `nyaterm-ui` owns the pixels; this owns the text.

use std::ops::Range;

/// Where a caret motion should land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMotion {
    Left,
    Right,
    /// Previous word boundary, the usual Ctrl+Left.
    WordLeft,
    WordRight,
    Start,
    End,
}

/// A single-line editing buffer: content, caret, and an optional selection.
///
/// Offsets are byte offsets into [`TextEdit::content`] and are always kept on
/// `char` boundaries, so callers can slice with them without checking.
#[derive(Debug, Clone, Default)]
pub struct TextEdit {
    content: String,
    cursor: usize,
    /// The fixed end of a selection. `None` means the caret is collapsed.
    anchor: Option<usize>,
}

impl TextEdit {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let cursor = content.len();
        Self {
            content,
            cursor,
            anchor: None,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Replace the whole buffer, keeping the caret inside it.
    ///
    /// Used when the owner refreshes the field from its own state; the caret is
    /// clamped rather than reset so an unrelated redraw does not jump it.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.floor_boundary(self.cursor);
        self.anchor = self.anchor.map(|anchor| self.floor_boundary(anchor));
        self.collapse_empty_selection();
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected span, empty when the caret is collapsed.
    pub fn selection(&self) -> Range<usize> {
        match self.anchor {
            Some(anchor) if anchor <= self.cursor => anchor..self.cursor,
            Some(anchor) => self.cursor..anchor,
            None => self.cursor..self.cursor,
        }
    }

    pub fn has_selection(&self) -> bool {
        self.anchor.is_some()
    }

    /// Whether the caret sits at the *start* of the selection, which platform
    /// IME APIs need in order to place a candidate window correctly.
    pub fn selection_is_reversed(&self) -> bool {
        self.anchor.is_some_and(|anchor| anchor > self.cursor)
    }

    pub fn set_selection(&mut self, range: Range<usize>, reversed: bool) {
        let start = self.floor_boundary(range.start);
        let end = self.floor_boundary(range.end.max(range.start));
        if reversed {
            self.cursor = start;
            self.anchor = Some(end);
        } else {
            self.cursor = end;
            self.anchor = Some(start);
        }
        self.collapse_empty_selection();
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.cursor = self.floor_boundary(offset);
        self.anchor = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.content.len();
        self.collapse_empty_selection();
    }

    /// Replace `range` with `text` and leave the caret after the insertion.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = self.floor_boundary(range.start);
        let end = self.floor_boundary(range.end).max(start);
        self.content.replace_range(start..end, text);
        self.cursor = start + text.len();
        self.anchor = None;
    }

    /// Insert at the caret, replacing the selection if there is one.
    pub fn insert(&mut self, text: &str) {
        let range = self.selection();
        self.replace(range, text);
    }

    /// Backspace: delete the selection, or the character before the caret.
    pub fn delete_backward(&mut self) -> bool {
        let range = self.selection();
        let range = if range.is_empty() {
            self.previous_boundary(self.cursor)..self.cursor
        } else {
            range
        };
        if range.is_empty() {
            return false;
        }
        self.replace(range, "");
        true
    }

    /// Delete: the selection, or the character after the caret.
    pub fn delete_forward(&mut self) -> bool {
        let range = self.selection();
        let range = if range.is_empty() {
            self.cursor..self.next_boundary(self.cursor)
        } else {
            range
        };
        if range.is_empty() {
            return false;
        }
        self.replace(range, "");
        true
    }

    /// Ctrl+Backspace: delete the selection, or back to the previous word.
    pub fn delete_word_backward(&mut self) -> bool {
        let range = self.selection();
        let range = if range.is_empty() {
            self.word_boundary_before(self.cursor)..self.cursor
        } else {
            range
        };
        if range.is_empty() {
            return false;
        }
        self.replace(range, "");
        true
    }

    /// Move the caret, extending the selection when `extend` is set.
    ///
    /// A plain move with a selection active collapses to the edge the motion
    /// points at, which is what every other text field does.
    pub fn move_cursor(&mut self, motion: CursorMotion, extend: bool) {
        if !extend && self.has_selection() {
            let selection = self.selection();
            match motion {
                CursorMotion::Left | CursorMotion::WordLeft => {
                    self.set_cursor(selection.start);
                    return;
                }
                CursorMotion::Right | CursorMotion::WordRight => {
                    self.set_cursor(selection.end);
                    return;
                }
                CursorMotion::Start | CursorMotion::End => {}
            }
        }

        let next = match motion {
            CursorMotion::Left => self.previous_boundary(self.cursor),
            CursorMotion::Right => self.next_boundary(self.cursor),
            CursorMotion::WordLeft => self.word_boundary_before(self.cursor),
            CursorMotion::WordRight => self.word_boundary_after(self.cursor),
            CursorMotion::Start => 0,
            CursorMotion::End => self.content.len(),
        };

        if extend {
            let anchor = self.anchor.unwrap_or(self.cursor);
            self.anchor = Some(anchor);
            self.cursor = next;
            self.collapse_empty_selection();
        } else {
            self.set_cursor(next);
        }
    }

    /// The word surrounding `offset`, for double-click selection.
    ///
    /// Between two words — on a run of separators — it returns that run, so a
    /// double click always selects something contiguous under the pointer.
    pub fn word_range_at(&self, offset: usize) -> Range<usize> {
        let offset = self.floor_boundary(offset);
        if self.content.is_empty() {
            return 0..0;
        }
        let probe = if offset == self.content.len() {
            self.previous_boundary(offset)
        } else {
            offset
        };
        let Some(kind) = self.content[probe..].chars().next().map(char_class) else {
            return offset..offset;
        };
        let mut start = probe;
        while start > 0 {
            let previous = self.previous_boundary(start);
            match self.content[previous..].chars().next() {
                Some(c) if char_class(c) == kind => start = previous,
                _ => break,
            }
        }
        let mut end = probe;
        while end < self.content.len() {
            match self.content[end..].chars().next() {
                Some(c) if char_class(c) == kind => end += c.len_utf8(),
                _ => break,
            }
        }
        start..end
    }

    /// Snap an arbitrary byte offset onto the nearest boundary at or below it.
    ///
    /// Offsets arrive from hit-testing and from platform IME callbacks, neither
    /// of which promises to land on a `char` boundary.
    pub fn floor_boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.content.len());
        while offset > 0 && !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let offset = self.floor_boundary(offset);
        self.content[..offset]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let offset = self.floor_boundary(offset);
        self.content[offset..]
            .chars()
            .next()
            .map(|c| offset + c.len_utf8())
            .unwrap_or(offset)
    }

    /// Skip any whitespace, then one run of a single class.
    ///
    /// Stopping where the class changes is what makes Ctrl+Left walk
    /// `user@host.example` part by part instead of jumping the whole thing.
    fn word_boundary_before(&self, offset: usize) -> usize {
        let mut index = self.floor_boundary(offset);
        while index > 0 {
            let previous = self.previous_boundary(index);
            match self.content[previous..].chars().next() {
                Some(c) if char_class(c) == CharClass::Space => index = previous,
                _ => break,
            }
        }
        let Some(kind) = self.class_before(index) else {
            return index;
        };
        while index > 0 {
            let previous = self.previous_boundary(index);
            match self.content[previous..].chars().next() {
                Some(c) if char_class(c) == kind => index = previous,
                _ => break,
            }
        }
        index
    }

    fn word_boundary_after(&self, offset: usize) -> usize {
        let mut index = self.floor_boundary(offset);
        while index < self.content.len() {
            match self.content[index..].chars().next() {
                Some(c) if char_class(c) == CharClass::Space => index += c.len_utf8(),
                _ => break,
            }
        }
        let Some(kind) = self.class_at(index) else {
            return index;
        };
        while index < self.content.len() {
            match self.content[index..].chars().next() {
                Some(c) if char_class(c) == kind => index += c.len_utf8(),
                _ => break,
            }
        }
        index
    }

    fn class_at(&self, offset: usize) -> Option<CharClass> {
        self.content[offset..].chars().next().map(char_class)
    }

    fn class_before(&self, offset: usize) -> Option<CharClass> {
        if offset == 0 {
            return None;
        }
        self.content[self.previous_boundary(offset)..]
            .chars()
            .next()
            .map(char_class)
    }

    fn collapse_empty_selection(&mut self) {
        if self.anchor == Some(self.cursor) {
            self.anchor = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

/// Word-motion classes.
///
/// Hosts and paths are what this field mostly holds, so `.`, `-`, `/` and `@`
/// count as punctuation: Ctrl+Left in `user@host.example` stops at each part
/// rather than jumping the whole thing.
fn char_class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

#[cfg(test)]
mod tests {
    use super::{CursorMotion, TextEdit};

    #[test]
    fn editing_steps_by_character_not_by_byte() {
        let mut edit = TextEdit::new("南京a");
        assert_eq!(edit.cursor(), edit.content().len());

        assert!(edit.delete_backward());
        assert_eq!(edit.content(), "南京");
        assert!(edit.delete_backward());
        assert_eq!(edit.content(), "南");

        edit.move_cursor(CursorMotion::Start, false);
        edit.insert("x");
        assert_eq!(edit.content(), "x南");
        assert_eq!(edit.cursor(), 1);

        assert!(edit.delete_forward());
        assert_eq!(edit.content(), "x");
    }

    #[test]
    fn shift_extends_and_typing_replaces_the_selection() {
        let mut edit = TextEdit::new("abc");
        edit.move_cursor(CursorMotion::Left, true);
        edit.move_cursor(CursorMotion::Left, true);
        assert_eq!(edit.selection(), 1..3);
        assert!(edit.selection_is_reversed());

        edit.insert("Z");
        assert_eq!(edit.content(), "aZ");
        assert!(!edit.has_selection());
    }

    #[test]
    fn a_plain_move_collapses_to_the_edge_it_points_at() {
        let mut edit = TextEdit::new("abcdef");
        edit.set_selection(1..4, false);

        edit.move_cursor(CursorMotion::Left, false);
        assert_eq!(edit.cursor(), 1);
        assert!(!edit.has_selection());

        edit.set_selection(1..4, false);
        edit.move_cursor(CursorMotion::Right, false);
        assert_eq!(edit.cursor(), 4);
    }

    #[test]
    fn word_motion_stops_at_the_parts_of_a_host() {
        let mut edit = TextEdit::new("user@host.example");
        edit.move_cursor(CursorMotion::Start, false);

        edit.move_cursor(CursorMotion::WordRight, false);
        assert_eq!(&edit.content()[..edit.cursor()], "user");
        edit.move_cursor(CursorMotion::WordRight, false);
        assert_eq!(&edit.content()[..edit.cursor()], "user@");
        edit.move_cursor(CursorMotion::WordRight, false);
        assert_eq!(&edit.content()[..edit.cursor()], "user@host");

        edit.move_cursor(CursorMotion::End, false);
        edit.move_cursor(CursorMotion::WordLeft, false);
        assert_eq!(&edit.content()[..edit.cursor()], "user@host.");
    }

    #[test]
    fn delete_word_backward_takes_one_part_at_a_time() {
        let mut edit = TextEdit::new("user@host");
        assert!(edit.delete_word_backward());
        assert_eq!(edit.content(), "user@");
        assert!(edit.delete_word_backward());
        assert_eq!(edit.content(), "user");
    }

    #[test]
    fn double_click_selects_the_word_under_the_pointer() {
        let edit = TextEdit::new("root@192.168.0.1");
        assert_eq!(edit.word_range_at(2), 0..4);
        // On the separator run itself, not the words either side of it.
        assert_eq!(edit.word_range_at(4), 4..5);
        // Past the end still resolves to the last word.
        assert_eq!(edit.word_range_at(edit.content().len()), 15..16);
    }

    #[test]
    fn offsets_from_hit_testing_are_snapped_onto_boundaries() {
        let mut edit = TextEdit::new("南京");
        // Mid-codepoint, as a click or an IME callback can produce.
        edit.set_cursor(1);
        assert_eq!(edit.cursor(), 0);

        edit.set_content("南");
        assert!(edit.content().is_char_boundary(edit.cursor()));
    }

    #[test]
    fn set_content_clamps_rather_than_resetting_the_caret() {
        let mut edit = TextEdit::new("abcdef");
        edit.set_cursor(5);
        edit.set_content("ab");
        assert_eq!(edit.cursor(), 2);
    }
}
