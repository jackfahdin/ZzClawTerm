use gpui::{KeyDownEvent, KeyUpEvent};
use nyaterm_terminal::alternate_scroll_key_bytes;

use crate::{
    TerminalKeyMode, TerminalSearchFlags, terminal_buffer_matches, terminal_font_features,
    terminal_key_bytes, terminal_key_bytes_with_mode, terminal_key_release_bytes_with_mode,
};

#[test]
fn terminal_font_features_disable_all_ligature_tags() {
    assert_eq!(
        terminal_font_features().tag_value_list(),
        &[
            ("calt".to_string(), 0),
            ("clig".to_string(), 0),
            ("liga".to_string(), 0),
        ]
    );
}

#[test]
fn buffer_matches_report_column_ranges() {
    let output = "hello world\nfoo hello bar";
    let matches = terminal_buffer_matches(
        output,
        "hello",
        &TerminalSearchFlags {
            case_sensitive: false,
            regex: false,
            whole_word: false,
        },
        10,
    )
    .expect("matches");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].line_index, 0);
    assert_eq!(matches[0].start_col, 0);
    assert_eq!(matches[0].end_col, 5);
    assert_eq!(matches[1].line_index, 1);
    assert_eq!(matches[1].start_col, 4);
    assert_eq!(matches[1].end_col, 9);
}

#[test]
fn terminal_key_bytes_include_common_xterm_keys() {
    assert_eq!(key_bytes("space", None, mods(false, false, false)), b" ");
    assert_eq!(
        key_bytes("space", Some(" "), mods(false, false, false)),
        b" "
    );
    assert_eq!(key_bytes("tab", None, mods(true, false, false)), b"\x1b[Z");
    assert_eq!(
        key_bytes("insert", None, mods(false, false, false)),
        b"\x1b[2~"
    );
    assert_eq!(key_bytes("f1", None, mods(false, false, false)), b"\x1bOP");
    assert_eq!(
        key_bytes("f12", None, mods(false, false, false)),
        b"\x1b[24~"
    );
}

#[test]
fn terminal_key_bytes_encode_modified_xterm_keys() {
    assert_eq!(
        key_bytes("right", None, mods(false, false, true)),
        b"\x1b[1;5C"
    );
    assert_eq!(
        key_bytes("right", None, mods(true, false, true)),
        b"\x1b[1;6C"
    );
    assert_eq!(
        key_bytes("delete", None, mods(false, true, false)),
        b"\x1b[3;3~"
    );
    assert_eq!(
        key_bytes("f1", None, mods(false, false, true)),
        b"\x1b[1;5P"
    );
    assert_eq!(
        key_bytes("f5", None, mods(true, false, false)),
        b"\x1b[15;2~"
    );
}

#[test]
fn terminal_key_bytes_send_control_c_and_control_v() {
    assert_eq!(key_bytes("c", Some("c"), mods(false, false, true)), b"\x03");
    assert_eq!(key_bytes("v", Some("v"), mods(false, false, true)), b"\x16");
}

#[test]
fn terminal_key_bytes_reserve_platform_only_shortcuts() {
    let event = key_event(
        "c",
        Some("c"),
        gpui::Modifiers {
            platform: true,
            ..gpui::Modifiers::default()
        },
    );
    assert!(terminal_key_bytes(&event).is_none());
}

fn key_bytes(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> Vec<u8> {
    terminal_key_bytes(&key_event(key, key_char, modifiers)).expect("terminal key bytes")
}

fn key_event(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn key_up_event(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyUpEvent {
    KeyUpEvent {
        keystroke: gpui::Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        },
    }
}

fn mods(shift: bool, alt: bool, control: bool) -> gpui::Modifiers {
    gpui::Modifiers {
        shift,
        alt,
        control,
        ..gpui::Modifiers::default()
    }
}

#[test]
fn application_cursor_arrows_use_ss3() {
    let mode = TerminalKeyMode {
        application_cursor: true,
        application_keypad: false,
        kitty_keyboard_disambiguate: false,
        kitty_keyboard_report_event_types: false,
        kitty_keyboard_report_alternate_keys: false,
        kitty_keyboard_report_all_keys_as_esc: false,
        kitty_keyboard_report_associated_text: false,
    };
    let event = key_event("up", None, mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1bOA".to_vec()
    );
    let event = key_event("home", None, mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1bOH".to_vec()
    );
    // Modified arrows still use CSI with parameters.
    let event = key_event("up", None, mods(false, false, true));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[1;5A".to_vec()
    );
}

#[test]
fn alternate_scroll_key_bytes_respect_cursor_mode() {
    assert_eq!(alternate_scroll_key_bytes(true, false), b"\x1b[A".to_vec());
    assert_eq!(alternate_scroll_key_bytes(false, true), b"\x1bOB".to_vec());
}

#[test]
fn terminal_key_bytes_encode_modified_backspace() {
    // Plain Backspace remains DEL.
    assert_eq!(
        key_bytes("backspace", None, mods(false, false, false)),
        b"\x7f"
    );
    // Ctrl+Backspace -> BS (0x08), same family as Ctrl+H.
    assert_eq!(
        key_bytes("backspace", None, mods(false, false, true)),
        b"\x08"
    );
    // Alt+Backspace -> ESC DEL for delete-word-backward.
    assert_eq!(
        key_bytes("backspace", None, mods(false, true, false)),
        b"\x1b\x7f"
    );
}

#[test]
fn application_keypad_numpad_uses_ss3() {
    let mode = TerminalKeyMode {
        application_cursor: false,
        application_keypad: true,
        kitty_keyboard_disambiguate: false,
        kitty_keyboard_report_event_types: false,
        kitty_keyboard_report_alternate_keys: false,
        kitty_keyboard_report_all_keys_as_esc: false,
        kitty_keyboard_report_associated_text: false,
    };
    let event = key_event("numpad5", Some("5"), mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1bOu".to_vec()
    );
    let event = key_event("numpad_enter", None, mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1bOM".to_vec()
    );
    // Normal mode still falls through to key_char digits.
    let event = key_event("numpad5", Some("5"), mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, TerminalKeyMode::default()).unwrap(),
        b"5".to_vec()
    );
}

#[test]
fn kitty_keyboard_disambiguates_text_and_ambiguous_keys() {
    let mode = TerminalKeyMode {
        application_cursor: false,
        application_keypad: false,
        kitty_keyboard_disambiguate: true,
        kitty_keyboard_report_event_types: false,
        kitty_keyboard_report_alternate_keys: false,
        kitty_keyboard_report_all_keys_as_esc: false,
        kitty_keyboard_report_associated_text: false,
    };

    let event = key_event("a", Some("a"), mods(false, false, true));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[97;5u".to_vec()
    );

    let event = key_event("a", Some("a"), mods(false, true, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[97;3u".to_vec()
    );

    let event = key_event("escape", None, mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[27u".to_vec()
    );

    let event = key_event("enter", None, mods(true, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[13;2u".to_vec()
    );

    let event = key_event("x", Some("x"), mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"x".to_vec()
    );
}

#[test]
fn kitty_keyboard_reports_event_types_and_all_keys() {
    let mode = TerminalKeyMode {
        application_cursor: false,
        application_keypad: false,
        kitty_keyboard_disambiguate: true,
        kitty_keyboard_report_event_types: true,
        kitty_keyboard_report_alternate_keys: false,
        kitty_keyboard_report_all_keys_as_esc: true,
        kitty_keyboard_report_associated_text: false,
    };

    let event = key_event("x", Some("x"), mods(false, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[120;1:1u".to_vec()
    );

    let mut repeat = key_event("x", Some("x"), mods(false, false, false));
    repeat.is_held = true;
    assert_eq!(
        terminal_key_bytes_with_mode(&repeat, mode).unwrap(),
        b"\x1b[120;1:2u".to_vec()
    );

    let event = key_event("a", Some("a"), mods(false, true, true));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[97;7:1u".to_vec()
    );

    let release = key_up_event("x", Some("x"), mods(false, false, false));
    assert_eq!(
        terminal_key_release_bytes_with_mode(&release, mode).unwrap(),
        b"\x1b[120;1:3u".to_vec()
    );

    let plain_mode = TerminalKeyMode {
        kitty_keyboard_report_event_types: false,
        ..mode
    };
    assert!(terminal_key_release_bytes_with_mode(&release, plain_mode).is_none());
}

#[test]
fn kitty_keyboard_reports_alternate_keys_and_associated_text() {
    let mode = TerminalKeyMode {
        application_cursor: false,
        application_keypad: false,
        kitty_keyboard_disambiguate: true,
        kitty_keyboard_report_event_types: true,
        kitty_keyboard_report_alternate_keys: true,
        kitty_keyboard_report_all_keys_as_esc: true,
        kitty_keyboard_report_associated_text: true,
    };

    let event = key_event("a", Some("A"), mods(true, false, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[97:65:97;2:1;65u".to_vec()
    );

    let event = key_event("s", Some("ß"), mods(false, true, false));
    assert_eq!(
        terminal_key_bytes_with_mode(&event, mode).unwrap(),
        b"\x1b[115;3:1;223u".to_vec()
    );

    let release = key_up_event("a", Some("A"), mods(true, false, false));
    assert_eq!(
        terminal_key_release_bytes_with_mode(&release, mode).unwrap(),
        b"\x1b[97:65:97;2:3u".to_vec()
    );
}
