pub(super) fn terminal_plain_text_input_event(event: &gpui::KeyDownEvent) -> bool {
    let modifiers = &event.keystroke.modifiers;
    if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
        return false;
    }
    event
        .keystroke
        .key_char
        .as_deref()
        .is_some_and(|input| input.chars().count() == 1 && input.chars().all(|ch| !ch.is_control()))
}

#[cfg(test)]
mod tests {
    use super::terminal_plain_text_input_event;

    fn key_event(
        key: &str,
        key_char: Option<&str>,
        modifiers: gpui::Modifiers,
    ) -> gpui::KeyDownEvent {
        gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers,
                key: key.to_string(),
                key_char: key_char.map(str::to_string),
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn plain_text_input_event_accepts_unmodified_printable_text() {
        let event = key_event("a", Some("a"), gpui::Modifiers::default());

        assert!(terminal_plain_text_input_event(&event));
    }

    #[test]
    fn plain_text_input_event_accepts_shifted_printable_text() {
        let event = key_event(
            "A",
            Some("A"),
            gpui::Modifiers {
                shift: true,
                ..gpui::Modifiers::default()
            },
        );

        assert!(terminal_plain_text_input_event(&event));
    }

    #[test]
    fn plain_text_input_event_rejects_control_modified_keys() {
        let event = key_event(
            "a",
            Some("a"),
            gpui::Modifiers {
                control: true,
                ..gpui::Modifiers::default()
            },
        );

        assert!(!terminal_plain_text_input_event(&event));
    }

    #[test]
    fn plain_text_input_event_rejects_navigation_keys() {
        let event = key_event("enter", None, gpui::Modifiers::default());

        assert!(!terminal_plain_text_input_event(&event));
    }
}
