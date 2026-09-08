#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalMouseReportEligibility {
    pub session_id_empty: bool,
    pub disconnected: bool,
    pub mouse_reporting: bool,
    pub motion: bool,
    pub mouse_drag_reporting: bool,
}

pub fn terminal_mouse_report_should_send(eligibility: TerminalMouseReportEligibility) -> bool {
    if eligibility.session_id_empty || eligibility.disconnected || !eligibility.mouse_reporting {
        return false;
    }
    if eligibility.motion && !eligibility.mouse_drag_reporting {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{TerminalMouseReportEligibility, terminal_mouse_report_should_send};

    fn eligible() -> TerminalMouseReportEligibility {
        TerminalMouseReportEligibility {
            session_id_empty: false,
            disconnected: false,
            mouse_reporting: true,
            motion: false,
            mouse_drag_reporting: false,
        }
    }

    #[test]
    fn mouse_report_requires_live_reporting_session() {
        assert!(terminal_mouse_report_should_send(eligible()));
        assert!(!terminal_mouse_report_should_send(
            TerminalMouseReportEligibility {
                session_id_empty: true,
                ..eligible()
            }
        ));
        assert!(!terminal_mouse_report_should_send(
            TerminalMouseReportEligibility {
                disconnected: true,
                ..eligible()
            }
        ));
        assert!(!terminal_mouse_report_should_send(
            TerminalMouseReportEligibility {
                mouse_reporting: false,
                ..eligible()
            }
        ));
    }

    #[test]
    fn motion_report_requires_drag_reporting() {
        assert!(!terminal_mouse_report_should_send(
            TerminalMouseReportEligibility {
                motion: true,
                mouse_drag_reporting: false,
                ..eligible()
            }
        ));
        assert!(terminal_mouse_report_should_send(
            TerminalMouseReportEligibility {
                motion: true,
                mouse_drag_reporting: true,
                ..eligible()
            }
        ));
    }
}
