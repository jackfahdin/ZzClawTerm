use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_DYNAMIC_TITLE_CODE_POINTS: usize = 256;

pub fn sanitize_dynamic_title(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len().min(4096));
    let mut whitespace_pending = false;

    for ch in value.nfc() {
        if is_bidi_formatting_control(ch) {
            continue;
        }
        if ch.is_whitespace() {
            whitespace_pending = !normalized.is_empty();
            continue;
        }
        if is_non_printing_control(ch) {
            continue;
        }
        if whitespace_pending {
            normalized.push(' ');
            whitespace_pending = false;
        }
        normalized.push(ch);
    }

    if normalized.is_empty() {
        return None;
    }
    Some(truncate_grapheme_safe(
        &normalized,
        MAX_DYNAMIC_TITLE_CODE_POINTS,
    ))
}

fn is_bidi_formatting_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}'
    )
}

fn is_non_printing_control(ch: char) -> bool {
    matches!(ch, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}')
}

fn truncate_grapheme_safe(value: &str, max_code_points: usize) -> String {
    if value.chars().count() <= max_code_points {
        return value.to_string();
    }
    if max_code_points == 0 {
        return String::new();
    }

    let content_limit = max_code_points.saturating_sub(1);
    let mut result = String::new();
    let mut used = 0;
    for grapheme in value.graphemes(true) {
        let grapheme_len = grapheme.chars().count();
        if used + grapheme_len > content_limit {
            break;
        }
        result.push_str(grapheme);
        used += grapheme_len;
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::{MAX_DYNAMIC_TITLE_CODE_POINTS, sanitize_dynamic_title};

    #[test]
    fn sanitizes_controls_bidi_and_whitespace() {
        assert_eq!(
            sanitize_dynamic_title("  admin\t\u{202e}txt.exe\r\n host\u{0007}  "),
            Some("admin txt.exe host".to_string())
        );
    }

    #[test]
    fn normalizes_to_nfc_and_rejects_empty_titles() {
        assert_eq!(
            sanitize_dynamic_title("Cafe\u{301}"),
            Some("Café".to_string())
        );
        assert_eq!(sanitize_dynamic_title("\u{202e}\n\t"), None);
    }

    #[test]
    fn truncates_without_splitting_a_grapheme() {
        let family = "👨‍👩‍👧‍👦";
        let title = format!("{}{}tail", "a".repeat(247), family);
        let sanitized = sanitize_dynamic_title(&title).unwrap();

        assert!(sanitized.ends_with('…'));
        assert!(sanitized.contains(family));
        assert!(!sanitized.contains("tail"));
        assert!(sanitized.chars().count() <= MAX_DYNAMIC_TITLE_CODE_POINTS);
    }
}
