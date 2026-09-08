pub(in crate::features::pages::transfers) fn editor_content_preview(
    content: &str,
    query: &str,
    active_match: usize,
) -> String {
    const MAX_PREVIEW_CHARS: usize = 16_000;
    if query.trim().is_empty() {
        let mut output = content.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        if content.chars().count() > MAX_PREVIEW_CHARS {
            output.push_str("\n\n-- preview truncated; full content is kept for saving --");
        }
        return output;
    }

    let matches = editor_search_matches(content, query);
    let Some(&byte_index) = matches.get(active_match.min(matches.len().saturating_sub(1))) else {
        return content.chars().take(MAX_PREVIEW_CHARS).collect();
    };
    let start = content[..byte_index]
        .char_indices()
        .rev()
        .nth(320)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let end = content[byte_index..]
        .char_indices()
        .nth(960)
        .map(|(index, _)| byte_index + index)
        .unwrap_or(content.len());
    let mut output = String::new();
    if start > 0 {
        output.push_str("-- search preview --\n");
    }
    output.push_str(&content[start..end]);
    if end < content.len() {
        output.push_str("\n\n-- preview truncated; full content is kept for saving --");
    }
    output
}

pub(in crate::features::pages::transfers) fn editor_search_matches(
    content: &str,
    query: &str,
) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    content
        .match_indices(query)
        .map(|(index, _)| index)
        .take(10_000)
        .collect()
}

pub(in crate::features::pages::transfers) fn format_permissions_octal(mode: u32) -> String {
    format!("{:04o}", mode & 0o7777)
}
