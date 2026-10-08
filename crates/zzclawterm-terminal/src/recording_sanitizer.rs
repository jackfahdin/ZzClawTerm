//! Streaming removal of terminal control strings for plain-text transcripts.

#[derive(Clone, Copy, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    Csi,
    String {
        osc: bool,
        escape: bool,
    },
}

#[derive(Default)]
pub struct RecordingSanitizer {
    state: State,
}

impl RecordingSanitizer {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn feed(&mut self, text: &str) -> String {
        let mut output = String::with_capacity(text.len());
        for ch in text.chars() {
            match self.state {
                State::Ground => match ch {
                    '\x1b' => self.state = State::Escape,
                    '\u{009b}' => self.state = State::Csi,
                    '\u{009d}' => {
                        self.state = State::String {
                            osc: true,
                            escape: false,
                        }
                    }
                    '\u{0090}' | '\u{0098}' | '\u{009e}' | '\u{009f}' => {
                        self.state = State::String {
                            osc: false,
                            escape: false,
                        }
                    }
                    '\r' => {}
                    '\n' => {
                        output.push('\n');
                    }
                    '\t' => output.push('\t'),
                    ch if ch.is_control() => {}
                    ch => {
                        output.push(ch);
                    }
                },
                State::Escape => {
                    self.state = match ch {
                        '[' => State::Csi,
                        ']' => State::String {
                            osc: true,
                            escape: false,
                        },
                        'P' | 'X' | '^' | '_' => State::String {
                            osc: false,
                            escape: false,
                        },
                        '\x1b' => State::Escape,
                        '\x20'..='\x2f' => State::EscapeIntermediate,
                        _ => State::Ground,
                    };
                }
                State::EscapeIntermediate => {
                    if ('\x30'..='\x7e').contains(&ch) {
                        self.state = State::Ground;
                    } else if ch == '\x1b' {
                        self.state = State::Escape;
                    }
                }
                State::Csi => {
                    if ('\x40'..='\x7e').contains(&ch) {
                        self.state = State::Ground;
                    } else if ch == '\x1b' {
                        self.state = State::Escape;
                    }
                }
                State::String { osc, escape } => {
                    self.state =
                        if (osc && ch == '\x07') || ch == '\u{009c}' || (escape && ch == '\\') {
                            State::Ground
                        } else {
                            State::String {
                                osc,
                                escape: ch == '\x1b',
                            }
                        };
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use crate::recording_sanitizer::RecordingSanitizer;

    #[test]
    fn control_strings_survive_every_utf8_chunk_boundary() {
        let input = "\x1b[31m红\x1b[0m\x1b]0;hidden\x1b\\\x1bPpayload\x1b\\\x1b(0\u{009b}31m\u{009d}hidden\u{009c}\r\nvisible";
        for boundary in 0..=input.len() {
            if !input.is_char_boundary(boundary) {
                continue;
            }
            let mut sanitizer = RecordingSanitizer::default();
            let mut output = sanitizer.feed(&input[..boundary]);
            output.push_str(&sanitizer.feed(&input[boundary..]));
            assert_eq!(output, "红\nvisible", "boundary {boundary}");
        }
    }
}
