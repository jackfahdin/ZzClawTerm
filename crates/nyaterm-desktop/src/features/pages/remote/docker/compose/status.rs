use gpui::rgb;

use crate::theme::ThemePalette;

pub(in crate::features::pages::remote) fn compose_status_label(status: &str) -> &'static str {
    let lower = status.trim().to_ascii_lowercase();
    if lower.is_empty() || lower == "-" {
        "—"
    } else if lower.contains("running") || lower == "up" {
        "running"
    } else if lower.contains("exited") || lower.contains("stopped") || lower.contains("down") {
        "stopped"
    } else if lower.contains("created") {
        "created"
    } else if lower.contains("paused") {
        "paused"
    } else if lower.contains("not created") {
        "not created"
    } else {
        "status"
    }
}

pub(in crate::features::pages::remote) fn compose_status_color(
    palette: ThemePalette,
    status: &str,
) -> gpui::Hsla {
    match status {
        "running" => rgb(palette.success).into(),
        "stopped" => rgb(0xfca5a5).into(),
        "created" | "paused" => rgb(0xfbbf24).into(),
        "not created" => rgb(palette.text_muted).into(),
        _ => rgb(palette.text_muted).into(),
    }
}
