use std::time::Duration;

/// Large-output protection modes (Tauri XTerminal performanceMode).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TerminalPerformanceMode {
    #[default]
    Normal,
    Overloaded,
}

/// In-pane large-output protection banner (Tauri PerformanceOverlayState).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalPerformanceOverlay {
    Overloaded,
    Recovered,
}

/// Pure presentation state for deciding terminal data-plane and paint work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalPresentation {
    VisibleActive,
    VisibleInactive,
    Background,
}

impl TerminalPresentation {
    pub(crate) fn resolve(is_active: bool, is_visible: bool) -> Self {
        match (is_active, is_visible) {
            (true, true) => Self::VisibleActive,
            (false, true) => Self::VisibleInactive,
            (_, false) => Self::Background,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalWorkPolicy {
    pub(crate) parse_output: bool,
    pub(crate) live_snapshot: bool,
    pub(crate) surface_notify: bool,
    pub(crate) active_decorations: bool,
}

impl TerminalWorkPolicy {
    pub(crate) const fn for_presentation(presentation: TerminalPresentation) -> Self {
        let visible = matches!(
            presentation,
            TerminalPresentation::VisibleActive | TerminalPresentation::VisibleInactive
        );
        Self {
            parse_output: true,
            live_snapshot: visible,
            surface_notify: visible,
            active_decorations: matches!(presentation, TerminalPresentation::VisibleActive),
        }
    }
}

pub(crate) const TERMINAL_OUTPUT_VISIBLE_BACKLOG_CAP: usize = 1_000_000;
pub(crate) const TERMINAL_OUTPUT_VISIBLE_BURST_OVERLOAD: usize = 256 * 1024;
pub(crate) const TERMINAL_UI_OUTPUT_TAIL_CAP: usize = 128 * 1024;
pub(crate) const TERMINAL_PERFORMANCE_RECOVERY_NOTICE: Duration = Duration::from_secs(3);
pub(crate) const TERMINAL_RENDER_DEGRADATION_RECOVERY_CALM: Duration = Duration::from_millis(400);
