//! Session character encoding for the terminal I/O path.
//!
//! Graphics protocol segments are handled on raw session bytes. Only the
//! remaining terminal stream is decoded to UTF-8 for Alacritty. Outgoing
//! text (paste / typed input) is re-encoded to the session charset.

use std::borrow::Cow;

use encoding_rs::{Decoder, Encoding, GBK, UTF_8};

/// Stateful charset converter owned by [`crate::TerminalCore`].
pub struct SessionEncoding {
    label: String,
    encoding: &'static Encoding,
    decoder: Decoder,
    /// Trailing bytes of a multi-byte sequence that the UTF-8 fast path held
    /// back for the next chunk (at most 3). Non-UTF-8 sessions keep that state
    /// inside `decoder` instead and leave this empty.
    utf8_tail: Vec<u8>,
}

impl std::fmt::Debug for SessionEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionEncoding")
            .field("label", &self.label)
            .field("encoding", &self.encoding.name())
            .finish()
    }
}

impl Default for SessionEncoding {
    fn default() -> Self {
        Self::from_label("UTF-8")
    }
}

impl Clone for SessionEncoding {
    fn clone(&self) -> Self {
        Self::from_label(&self.label)
    }
}

impl SessionEncoding {
    /// Resolve a user/settings label (`UTF-8`, `GBK`, …) to a streaming converter.
    pub fn from_label(label: &str) -> Self {
        let (label, encoding) = resolve_encoding(label);
        Self {
            label: label.to_string(),
            encoding,
            decoder: encoding.new_decoder_without_bom_handling(),
            utf8_tail: Vec::new(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_utf8(&self) -> bool {
        self.encoding == UTF_8
    }

    /// Decode one output chunk to UTF-8 bytes for the ANSI parser.
    ///
    /// UTF-8 sessions — the default, and the overwhelming majority — take a
    /// validate-and-borrow path: valid input is handed straight back with no
    /// decoder pass and no allocation. Only a trailing incomplete sequence
    /// (held for the next chunk) or a genuinely malformed byte costs a copy.
    ///
    /// Incomplete multi-byte sequences are retained across calls either way.
    pub fn decode_output_bytes<'a>(&mut self, input: &'a [u8]) -> Cow<'a, [u8]> {
        if input.is_empty() {
            return Cow::Borrowed(input);
        }
        if !self.is_utf8() {
            return Cow::Owned(self.decode_via_decoder(input).into_bytes());
        }
        if self.utf8_tail.is_empty() {
            let (emit, hold) = utf8_emit_and_hold(input);
            self.utf8_tail.extend_from_slice(hold);
            return emit;
        }
        // A prior chunk ended mid-character: splice before deciding.
        let mut spliced = std::mem::take(&mut self.utf8_tail);
        spliced.extend_from_slice(input);
        let (emit, hold) = utf8_emit_and_hold(&spliced);
        let emit = emit.into_owned();
        self.utf8_tail.extend_from_slice(hold);
        Cow::Owned(emit)
    }

    /// Decode one output chunk to text.
    ///
    /// Incomplete multi-byte sequences are retained across calls, including
    /// UTF-8 sessions where output consumers may receive arbitrary byte chunks.
    pub fn decode_output_text(&mut self, input: &[u8]) -> String {
        if input.is_empty() {
            return String::new();
        }
        if !self.is_utf8() {
            return self.decode_via_decoder(input);
        }
        // `decode_output_bytes` only ever yields well-formed UTF-8; the lossy
        // fallbacks below exist so a bug could never drop output on the floor.
        match self.decode_output_bytes(input) {
            Cow::Borrowed(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            Cow::Owned(bytes) => String::from_utf8(bytes)
                .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()),
        }
    }

    /// Decode one output chunk to owned UTF-8 bytes.
    pub fn decode_output_chunk(&mut self, input: &[u8]) -> Vec<u8> {
        self.decode_output_bytes(input).into_owned()
    }

    /// The streaming `encoding_rs` path, used by every non-UTF-8 charset.
    fn decode_via_decoder(&mut self, input: &[u8]) -> String {
        let mut dst = String::with_capacity(input.len());
        let mut src = input;
        loop {
            let (result, read, _replacements) = self.decoder.decode_to_string(src, &mut dst, false);
            src = &src[read..];
            match result {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    dst.reserve(dst.capacity().max(16));
                }
            }
        }
        dst
    }

    /// Encode UTF-8 text for the session wire format (paste / typed text).
    pub fn encode_str(&self, text: &str) -> Vec<u8> {
        if self.is_utf8() {
            return text.as_bytes().to_vec();
        }
        let (cow, _, _) = self.encoding.encode(text);
        cow.into_owned()
    }

    /// Encode a UTF-8 byte buffer. Non-UTF-8 / pure-ASCII control payloads pass through.
    pub fn encode_outgoing(&self, utf8_or_ascii: &[u8]) -> Vec<u8> {
        if self.is_utf8() || utf8_or_ascii.is_empty() {
            return utf8_or_ascii.to_vec();
        }
        if utf8_or_ascii.iter().all(|b| b.is_ascii()) {
            return utf8_or_ascii.to_vec();
        }
        match std::str::from_utf8(utf8_or_ascii) {
            Ok(text) => self.encode_str(text),
            Err(_) => utf8_or_ascii.to_vec(),
        }
    }

    /// Reset decoder state (e.g. after a hard screen clear/reconnect).
    pub fn reset_decoder(&mut self) {
        self.decoder = self.encoding.new_decoder_without_bom_handling();
        self.utf8_tail.clear();
    }
}

/// Split `buf` into the bytes that can be emitted now and the trailing bytes of
/// an unfinished character to hold for the next chunk. Malformed bytes in the
/// emitted part become U+FFFD.
fn utf8_emit_and_hold(buf: &[u8]) -> (Cow<'_, [u8]>, &[u8]) {
    let (head, hold) = buf.split_at(buf.len() - incomplete_utf8_tail_len(buf));
    let emit = match std::str::from_utf8(head) {
        Ok(_) => Cow::Borrowed(head),
        Err(_) => Cow::Owned(String::from_utf8_lossy(head).into_owned().into_bytes()),
    };
    (emit, hold)
}

/// How many trailing bytes begin a multi-byte sequence that has not finished
/// yet. Zero when the buffer ends on a complete character, on ASCII, or on a
/// byte that can never lead a valid sequence (those get replaced right away
/// rather than held for a continuation that would not rescue them).
fn incomplete_utf8_tail_len(buf: &[u8]) -> usize {
    // A UTF-8 character is at most 4 bytes, so only the last 3 can be pending.
    for back in 1..=3.min(buf.len()) {
        let byte = buf[buf.len() - back];
        if byte < 0x80 {
            return 0;
        }
        if byte >= 0xc0 {
            let needed = if byte >= 0xf8 {
                return 0;
            } else if byte >= 0xf0 {
                4
            } else if byte >= 0xe0 {
                3
            } else {
                2
            };
            return if needed > back { back } else { 0 };
        }
        // Continuation byte: walk back to the lead byte that owns it.
    }
    0
}

fn resolve_encoding(label: &str) -> (&'static str, &'static Encoding) {
    let trimmed = label.trim();
    if trimmed.eq_ignore_ascii_case("gbk")
        || trimmed.eq_ignore_ascii_case("gb2312")
        || trimmed.eq_ignore_ascii_case("cp936")
    {
        ("GBK", GBK)
    } else if trimmed.eq_ignore_ascii_case("gb18030") {
        ("GB18030", encoding_rs::GB18030)
    } else if trimmed.eq_ignore_ascii_case("big5") {
        ("Big5", encoding_rs::BIG5)
    } else if trimmed.eq_ignore_ascii_case("shift_jis")
        || trimmed.eq_ignore_ascii_case("shift-jis")
        || trimmed.eq_ignore_ascii_case("sjis")
    {
        ("Shift_JIS", encoding_rs::SHIFT_JIS)
    } else if trimmed.eq_ignore_ascii_case("euc-kr") || trimmed.eq_ignore_ascii_case("euckr") {
        ("EUC-KR", encoding_rs::EUC_KR)
    } else {
        ("UTF-8", UTF_8)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{SessionEncoding, incomplete_utf8_tail_len};

    #[test]
    fn utf8_is_passthrough() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        assert!(enc.is_utf8());
        assert_eq!(enc.decode_output_chunk(b"hi"), b"hi");
        assert_eq!(enc.encode_str("hi"), b"hi");
    }

    #[test]
    fn utf8_split_multibyte_across_chunks() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        let bytes = "测".as_bytes();

        let part1 = enc.decode_output_text(&bytes[..1]);
        assert!(
            part1.is_empty(),
            "incomplete UTF-8 byte should be held, got {part1:?}"
        );
        let part2 = enc.decode_output_text(&bytes[1..]);
        assert_eq!(part2, "测");
    }

    /// The whole point of the fast path: valid UTF-8 is handed back untouched,
    /// with no decoder pass and no allocation.
    #[test]
    fn utf8_valid_input_is_borrowed_not_copied() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        let input = "hello 世界".as_bytes();
        assert!(matches!(
            enc.decode_output_bytes(input),
            Cow::Borrowed(borrowed) if std::ptr::eq(borrowed, input)
        ));
    }

    #[test]
    fn utf8_malformed_byte_becomes_replacement_char() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        // 0xff can never appear in valid UTF-8.
        assert_eq!(enc.decode_output_text(b"a\xffb"), "a\u{fffd}b");
    }

    /// A bad byte must not desync the stream: whatever follows still decodes,
    /// including a character split across the next chunk boundary.
    #[test]
    fn utf8_stream_recovers_after_a_malformed_byte() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        assert_eq!(enc.decode_output_text(b"\xff"), "\u{fffd}");

        let bytes = "测".as_bytes();
        assert!(enc.decode_output_text(&bytes[..2]).is_empty());
        assert_eq!(enc.decode_output_text(&bytes[2..]), "测");
    }

    #[test]
    fn utf8_reset_decoder_drops_the_held_tail() {
        let mut enc = SessionEncoding::from_label("UTF-8");
        let bytes = "测".as_bytes();
        assert!(enc.decode_output_text(&bytes[..1]).is_empty());

        enc.reset_decoder();

        // Without the reset the stale lead byte would pair with these two and
        // resurrect the dropped character.
        assert_eq!(enc.decode_output_text(&bytes[1..]), "\u{fffd}\u{fffd}");
    }

    #[test]
    fn incomplete_tail_len_holds_only_rescuable_prefixes() {
        assert_eq!(incomplete_utf8_tail_len(b"abc"), 0, "ASCII holds nothing");
        assert_eq!(
            incomplete_utf8_tail_len("测".as_bytes()),
            0,
            "complete char"
        );
        assert_eq!(incomplete_utf8_tail_len(&"测".as_bytes()[..1]), 1);
        assert_eq!(incomplete_utf8_tail_len(&"测".as_bytes()[..2]), 2);
        assert_eq!(incomplete_utf8_tail_len(&"𝄞".as_bytes()[..3]), 3);
        assert_eq!(
            incomplete_utf8_tail_len(b"\xff"),
            0,
            "a byte that can never lead a sequence is replaced, not held"
        );
    }

    #[test]
    fn gbk_roundtrip_chinese() {
        let mut enc = SessionEncoding::from_label("GBK");
        assert!(!enc.is_utf8());
        // "测试" in GBK
        let gbk = [0xb2, 0xe2, 0xca, 0xd4];
        let utf8 = enc.decode_output_chunk(&gbk);
        assert_eq!(String::from_utf8(utf8.clone()).unwrap(), "测试");
        assert_eq!(enc.encode_str("测试"), gbk);
        // Outgoing UTF-8 bytes re-encode.
        assert_eq!(enc.encode_outgoing("测试".as_bytes()), gbk);
        // ASCII CSI stays intact.
        assert_eq!(enc.encode_outgoing(b"\x1b[A"), b"\x1b[A");
    }

    #[test]
    fn gbk_split_multibyte_across_chunks() {
        let mut enc = SessionEncoding::from_label("GBK");
        // First byte of "测" (0xb2 0xe2)
        let part1 = enc.decode_output_chunk(&[0xb2]);
        assert!(
            part1.is_empty(),
            "incomplete GBK byte should be held, got {part1:?}"
        );
        let part2 = enc.decode_output_chunk(&[0xe2, 0xca, 0xd4]);
        let combined = [part1, part2].concat();
        assert_eq!(String::from_utf8(combined).unwrap(), "测试");
    }
}
