use std::collections::HashSet;

use gpui::{FontWeight, IntoElement, div, prelude::*, rgb};
use nyaterm_core::truncate_preview;

use crate::theme::ThemePalette;

pub(super) fn command_suggestion_highlight_parts(
    text: &str,
    indices: &[u32],
    palette: ThemePalette,
    selected: bool,
) -> Vec<gpui::AnyElement> {
    let _selected = selected;
    let clipped = truncate_preview(text, 48);
    let index_set: HashSet<u32> = indices.iter().copied().collect();
    let chars: Vec<char> = clipped.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let highlighted = index_set.contains(&(i as u32));
        let start = i;
        i += 1;
        while i < chars.len() && index_set.contains(&(i as u32)) == highlighted {
            i += 1;
        }
        let chunk: String = chars[start..i].iter().collect();
        let color = if highlighted {
            palette.link
        } else {
            palette.text
        };
        parts.push(
            div()
                .text_color(rgb(color))
                .when(highlighted, |this| this.font_weight(FontWeight::SEMIBOLD))
                .child(chunk)
                .into_any_element(),
        );
    }
    if parts.is_empty() {
        parts.push(
            div()
                .text_color(rgb(palette.text))
                .child(clipped)
                .into_any_element(),
        );
    }
    parts
}
