#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::features) enum ProcessDisplayMode {
    Compact,
    Narrow,
    Medium,
    Wide,
}

pub(in crate::features) fn process_display_mode(panel_width: f32) -> ProcessDisplayMode {
    // Tauri getProcessDisplayMode thresholds.
    if panel_width > 0. && panel_width < 320. {
        ProcessDisplayMode::Compact
    } else if panel_width > 0. && panel_width < 430. {
        ProcessDisplayMode::Narrow
    } else if panel_width > 0. && panel_width < 540. {
        ProcessDisplayMode::Medium
    } else {
        ProcessDisplayMode::Wide
    }
}

pub(in crate::features::pages::remote) fn process_row_height_px(mode: ProcessDisplayMode) -> f32 {
    match mode {
        ProcessDisplayMode::Compact => 62.,
        _ => 38.,
    }
}

pub(in crate::features::pages::remote) fn process_details_height_px(
    mode: ProcessDisplayMode,
) -> f32 {
    // Native densified shells (Tauri uses 176/218/274).
    match mode {
        ProcessDisplayMode::Compact => 274.,
        ProcessDisplayMode::Narrow => 218.,
        _ => 176.,
    }
}
