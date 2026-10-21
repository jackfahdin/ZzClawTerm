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
    use gpui_kit::component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
    use zzclawterm_core::document_language::DOCUMENT_LANGUAGES;

    use super::{command_highlights, document_fold_ranges};

    #[test]
    fn every_document_language_has_a_working_highlighter() {
        let samples = [
            (
                "astro",
                "---\nconst title = 'Hello';\n---\n<h1>{title}</h1>",
            ),
            ("bash", "echo \"hello $HOME\" # comment"),
            ("c", "int main(void) { return 42; }"),
            ("cpp", "class Hello { public: int value = 42; };"),
            ("css", "body { color: red; }"),
            ("diff", "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"),
            ("ejs", "<h1><%= title %></h1>"),
            ("elixir", "defmodule Hello do\n  def value, do: 42\nend"),
            ("erb", "<h1><%= title %></h1>"),
            ("go", "package main\nfunc main() { println(42) }"),
            ("html", "<h1 class=\"title\">Hello</h1>"),
            ("java", "class Hello { int value = 42; }"),
            ("javascript", "const value = 42;"),
            ("jsdoc", "/** @param {string} name */"),
            ("json", "{\"value\": 42}"),
            ("kotlin", "fun main() { val value = 42 }"),
            ("lua", "local value = 42"),
            ("make", "all:\n\techo hello\n"),
            ("markdown", "# Hello\n\n**world**\n"),
            ("php", "<?php echo 'hello'; ?>"),
            ("python", "def hello():\n    return 42\n"),
            ("ruby", "def hello\n  42\nend"),
            ("rust", "fn main() { let value = 42; }"),
            ("scala", "object Hello { val value = 42 }"),
            ("sql", "SELECT name FROM users WHERE id = 42;"),
            (
                "svelte",
                "<script>let title = 'Hello';</script><h1>{title}</h1>",
            ),
            ("toml", "value = 42"),
            (
                "tsx",
                "type Props = { title: string }; const view = <div>Hello</div>;",
            ),
            ("typescript", "const value: number = 42;"),
            ("yaml", "value: 42"),
            ("zig", "const value: u32 = 42;"),
        ];
        assert_eq!(samples.len(), DOCUMENT_LANGUAGES.len());
        for language in DOCUMENT_LANGUAGES {
            let sample = samples
                .iter()
                .find(|(id, _)| *id == language.id)
                .unwrap_or_else(|| panic!("missing sample for {}", language.id))
                .1;
            let grammar = LanguageRegistry::singleton()
                .language(language.id)
                .unwrap_or_else(|| panic!("missing grammar for {}", language.id));
            assert!(grammar.has_grammar(), "{}", language.id);
            assert!(!grammar.highlights.is_empty(), "{}", language.id);
            let text = ropey::Rope::from(sample);
            let mut highlighter = SyntaxHighlighter::new(language.id);
            highlighter.update(None, &text, None);
            assert!(highlighter.tree().is_some(), "{}", language.id);
            assert!(
                !highlighter
                    .styles(&(0..sample.len()), HighlightTheme::default_dark().as_ref())
                    .is_empty(),
                "no highlights for {}",
                language.id,
            );
            if language.id == "tsx" {
                assert!(!highlighter.tree().unwrap().root_node().has_error());
            }
        }
    }

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
