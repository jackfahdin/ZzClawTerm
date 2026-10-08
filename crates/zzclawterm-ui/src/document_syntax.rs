//! Background-only syntax parsing behind the gpui-kit integration boundary.
use gpui_kit::component::highlighter::SyntaxHighlighter;
use std::ops::Range;

use gpui::{HighlightStyle, rgb};
use gpui_base::input::HighlightStyleResolver;

use crate::theme::ThemePalette;

struct CommandColors(ThemePalette);

impl HighlightStyleResolver for CommandColors {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        // Keep shell previews to three accents from the current theme rather
        // than importing a separate editor color scheme.
        let color = match name.split('.').next()? {
            "function" | "keyword" => self.0.link,
            "string" => self.0.success,
            "variable" | "constant" | "number" => self.0.accent,
            "comment" => self.0.text_muted,
            _ => return None,
        };
        Some(HighlightStyle {
            color: Some(rgb(color).into()),
            ..Default::default()
        })
    }
}

/// Parse once off the UI thread. The returned byte ranges are safe to cache
/// and render without a parser, theme singleton, or GPUI context.
pub fn command_highlights(
    content: &str,
    palette: ThemePalette,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if content.is_empty() || content.len() > 64 * 1024 {
        return Vec::new();
    }
    let text = ropey::Rope::from(content);
    let mut highlighter = SyntaxHighlighter::new("bash");
    highlighter.update(None, &text, Some(std::time::Duration::from_millis(50)));
    highlighter.styles(&(0..content.len()), &CommandColors(palette))
}

pub fn document_fold_ranges(content: &str, language: &str) -> Vec<Range<usize>> {
    let text = ropey::Rope::from(content);
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &text, Some(std::time::Duration::from_millis(100)));
    let mut ranges = Vec::new();
    if let Some(tree) = highlighter.tree() {
        let mut cursor = tree.walk();
        loop {
            let node = cursor.node();
            if node.is_named() && node.start_position().row + 1 < node.end_position().row {
                let start = content[node.start_byte()..]
                    .find('\n')
                    .map(|offset| node.start_byte() + offset + 1);
                let end = content[..node.end_byte()]
                    .rfind('\n')
                    .map(|offset| offset + 1);
                if let (Some(start), Some(end)) = (start, end)
                    && start < end
                {
                    ranges.push(start..end);
                }
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    ranges.sort_by_key(|range| (range.start, range.end));
                    ranges.dedup_by_key(|range| range.start);
                    return ranges;
                }
            }
        }
    }
    // Unknown language: indentation is the only syntax-independent fold invariant.
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in &lines {
        starts.push(offset);
        offset += line.len();
    }
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .count();
        let mut end = index + 1;
        while end < lines.len() {
            let next = lines[end];
            if !next.trim().is_empty()
                && next
                    .chars()
                    .take_while(|ch| matches!(ch, ' ' | '\t'))
                    .count()
                    <= indent
            {
                break;
            }
            end += 1;
        }
        if end > index + 2 {
            ranges.push(starts[index + 1]..starts.get(end).copied().unwrap_or(content.len()));
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::{command_highlights, document_fold_ranges};

    #[test]
    fn shell_highlights_use_theme_colors_and_preserve_utf8_ranges() {
        let command = "printf '%s' \"你好 $HOME\" | head -n 3 # 注释";
        for theme in ["github-dark", "solarized-light"] {
            let palette = crate::theme::theme_palette(theme);
            let highlights = command_highlights(command, palette);
            for (range, _) in &highlights {
                assert!(range.end <= command.len());
                assert!(command.is_char_boundary(range.start));
                assert!(command.is_char_boundary(range.end));
            }
            for color in [palette.link, palette.success, palette.text_muted] {
                assert!(
                    highlights
                        .iter()
                        .any(|(_, style)| style.color == Some(gpui::rgb(color).into()))
                );
            }
        }
    }
    #[test]
    fn syntax_and_indentation_folds_keep_the_header_and_following_line() {
        let json = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
        assert!(
            document_fold_ranges(json, "json")
                .iter()
                .any(|range| &json[range.clone()] == "  \"a\": 1,\n  \"b\": 2\n")
        );
        let plain = "header\n  first\n  second\nnext\n";
        assert_eq!(document_fold_ranges(plain, "plain"), vec![7..24]);
    }
}
