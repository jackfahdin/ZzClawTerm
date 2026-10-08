//! Byte-oriented editing. Formatting and row widths never affect the payload.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HexNibble {
    #[default]
    High,
    Low,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HexCursor {
    pub byte_index: usize,
    pub nibble: HexNibble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HexSelection {
    pub anchor: usize,
    pub end: usize,
}

impl HexSelection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.end)..self.anchor.max(self.end)
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct HexDocument {
    bytes: Vec<u8>,
    cursor: HexCursor,
    selection: Option<HexSelection>,
    pending_nibble: Option<u8>,
}

impl HexDocument {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn cursor(&self) -> HexCursor {
        self.cursor
    }

    pub fn selection(&self) -> Option<Range<usize>> {
        self.selection
            .map(HexSelection::range)
            .filter(|r| !r.is_empty())
    }

    pub fn pending_nibble(&self) -> Option<u8> {
        self.pending_nibble
    }

    pub fn selection_anchor(&self) -> usize {
        self.selection
            .map_or(self.cursor.byte_index, |selection| selection.anchor)
    }

    pub fn set_bytes(&mut self, bytes: Vec<u8>) {
        *self = Self {
            bytes,
            ..Self::default()
        };
    }

    pub fn move_cursor(&mut self, cursor: HexCursor, extend: bool) {
        let index = cursor.byte_index.min(self.bytes.len());
        if extend {
            let anchor = self.selection.map_or(self.cursor.byte_index, |s| s.anchor);
            self.selection = Some(HexSelection { anchor, end: index });
        } else {
            self.selection = None;
        }
        self.cursor = HexCursor {
            byte_index: index,
            nibble: if index == self.bytes.len() {
                HexNibble::High
            } else {
                cursor.nibble
            },
        };
        self.pending_nibble = None;
    }

    pub fn select_all(&mut self) {
        self.pending_nibble = None;
        self.selection = Some(HexSelection {
            anchor: 0,
            end: self.bytes.len(),
        });
        self.cursor = HexCursor {
            byte_index: self.bytes.len(),
            nibble: HexNibble::High,
        };
    }

    pub fn cancel_selection(&mut self) {
        self.selection = None;
        self.pending_nibble = None;
        self.cursor.nibble = HexNibble::High;
    }

    fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection() else {
            return false;
        };
        self.bytes.drain(range.clone());
        self.cursor = HexCursor {
            byte_index: range.start,
            nibble: HexNibble::High,
        };
        self.selection = None;
        self.pending_nibble = None;
        true
    }

    pub fn type_nibble(&mut self, nibble: u8) {
        if nibble > 15 {
            return;
        }
        self.delete_selection();
        let index = self.cursor.byte_index;
        match self.cursor.nibble {
            HexNibble::High => {
                // A first nibble is an edit in progress, never a fabricated byte.
                self.pending_nibble = Some(nibble);
                self.cursor.nibble = HexNibble::Low;
            }
            HexNibble::Low => {
                let high = self
                    .pending_nibble
                    .take()
                    .unwrap_or_else(|| self.bytes[index] >> 4);
                let byte = (high << 4) | nibble;
                if index == self.bytes.len() {
                    self.bytes.push(byte);
                } else {
                    self.bytes[index] = byte;
                }
                self.cursor = HexCursor {
                    byte_index: index + 1,
                    nibble: HexNibble::High,
                };
            }
        }
    }

    pub fn insert_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.delete_selection();
        let index = self.cursor.byte_index;
        self.bytes.splice(index..index, bytes.iter().copied());
        self.cursor = HexCursor {
            byte_index: index + bytes.len(),
            nibble: HexNibble::High,
        };
        self.pending_nibble = None;
    }

    pub fn delete(&mut self, backwards: bool) {
        if self.pending_nibble.take().is_some() {
            self.cursor.nibble = HexNibble::High;
            return;
        }
        if self.delete_selection() {
            return;
        }
        let index = self.cursor.byte_index;
        if backwards && index > 0 {
            self.bytes.remove(index - 1);
            self.cursor.byte_index -= 1;
        } else if !backwards && index < self.bytes.len() {
            self.bytes.remove(index);
        }
        self.cursor.nibble = HexNibble::High;
    }

    pub fn selected_bytes(&self) -> &[u8] {
        self.selection()
            .map_or(&self.bytes, |range| &self.bytes[range])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexPasteError {
    InvalidFormat,
    IncompleteByte,
}

/// Accept plain pairs, comma/whitespace-separated pairs, 0x pairs and escaped
/// pairs. Validate the entire input before an editor mutates its document.
pub fn parse_hex_paste(text: &str) -> Result<Vec<u8>, HexPasteError> {
    let input = text.as_bytes();
    let mut digits = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let ch = input[i];
        if ch.is_ascii_whitespace() || ch == b',' {
            if !digits.len().is_multiple_of(2) {
                return Err(HexPasteError::IncompleteByte);
            }
            i += 1;
            continue;
        }
        let prefixed = input[i..].starts_with(b"0x")
            || input[i..].starts_with(b"0X")
            || input[i..].starts_with(b"\\x")
            || input[i..].starts_with(b"\\X");
        if prefixed {
            if !digits.len().is_multiple_of(2) {
                return Err(HexPasteError::IncompleteByte);
            }
            i += 2;
            let pair = input.get(i..i + 2).ok_or(HexPasteError::IncompleteByte)?;
            for &digit in pair {
                digits.push(hex_digit(digit).ok_or(HexPasteError::InvalidFormat)?);
            }
            i += 2;
            if i < input.len()
                && input[i].is_ascii_hexdigit()
                && !input[i..].starts_with(b"0x")
                && !input[i..].starts_with(b"0X")
            {
                return Err(HexPasteError::InvalidFormat);
            }
        } else {
            digits.push(hex_digit(ch).ok_or(HexPasteError::InvalidFormat)?);
            i += 1;
        }
    }
    if !digits.len().is_multiple_of(2) {
        return Err(HexPasteError::IncompleteByte);
    }
    Ok(digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| pair[0] << 4 | pair[1])
        .collect())
}

fn hex_digit(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'a'..=b'f' => Some(ch - b'a' + 10),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub enum HexCopyFormat {
    Hex,
    Compact,
    CArray,
    Escaped,
    Ascii,
}

pub fn format_hex(bytes: &[u8], format: HexCopyFormat) -> String {
    if matches!(format, HexCopyFormat::Ascii) {
        return bytes
            .iter()
            .map(|&b| {
                if (0x20..=0x7e).contains(&b) {
                    char::from(b)
                } else {
                    '·'
                }
            })
            .collect();
    }
    let (prefix, separator) = match format {
        HexCopyFormat::Hex => ("", " "),
        HexCopyFormat::Compact => ("", ""),
        HexCopyFormat::CArray => ("0x", ", "),
        HexCopyFormat::Escaped => ("\\x", ""),
        HexCopyFormat::Ascii => unreachable!(),
    };
    bytes
        .iter()
        .map(|b| format!("{prefix}{b:02X}"))
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests {
    use super::{HexCopyFormat, HexCursor, HexDocument, HexNibble, format_hex, parse_hex_paste};

    #[test]
    fn paste_formats_decode_identically_and_reject_invalid_input() {
        for value in [
            "48656C6C6F",
            "48 65 6C 6C 6F",
            "48,65,6C,6C,6F",
            "0x48 0x65 0x6c 0x6c 0x6f",
            "\\x48\\x65\\x6c\\x6c\\x6f",
        ] {
            assert_eq!(parse_hex_paste(value).unwrap(), b"Hello");
        }
        for value in ["Hello", "4", "4 8", "0x", "0x1234", "48 ZZ", "48 中"] {
            assert!(parse_hex_paste(value).is_err(), "{value}");
        }
    }

    #[test]
    fn nibble_edits_commit_only_complete_bytes_and_overwrite_at_cursor() {
        let mut doc = HexDocument::default();
        doc.type_nibble(4);
        assert!(doc.bytes().is_empty());
        assert_eq!(doc.pending_nibble(), Some(4));
        doc.type_nibble(8);
        assert_eq!(doc.bytes(), b"H");
        doc.move_cursor(
            HexCursor {
                byte_index: 0,
                nibble: HexNibble::Low,
            },
            false,
        );
        doc.type_nibble(9);
        assert_eq!(doc.bytes(), b"I");
        doc.type_nibble(15);
        doc.delete(true);
        assert_eq!(doc.bytes(), b"I");
        assert_eq!(doc.pending_nibble(), None);
    }

    #[test]
    fn byte_selection_replaces_deletes_and_copies_the_same_range() {
        let mut doc = HexDocument::default();
        doc.set_bytes(b"Hello".to_vec());
        doc.move_cursor(
            HexCursor {
                byte_index: 1,
                nibble: HexNibble::High,
            },
            false,
        );
        doc.move_cursor(
            HexCursor {
                byte_index: 4,
                nibble: HexNibble::High,
            },
            true,
        );
        assert_eq!(doc.selected_bytes(), b"ell");
        assert_eq!(
            format_hex(doc.selected_bytes(), HexCopyFormat::Hex),
            "65 6C 6C"
        );
        doc.insert_bytes(b"i");
        assert_eq!(doc.bytes(), b"Hio");
        doc.select_all();
        doc.delete(false);
        assert!(doc.bytes().is_empty());
    }
}
