//! Command risk classification.
//!
//! Split out of `ai.rs` by domain. This decides how dangerous a shell command
//! looks before the agent is allowed to run it, so the pattern lists and the
//! escalation rules are behaviour, not formatting: they are unchanged here.

use super::RiskLevel;

pub fn parse_risk_level_label(value: &str) -> Option<RiskLevel> {
    match value.trim().replace('-', "_").to_ascii_lowercase().as_str() {
        "low" => Some(RiskLevel::Low),
        "medium" | "moderate" => Some(RiskLevel::Medium),
        "high" => Some(RiskLevel::High),
        "critical" | "danger" | "dangerous" => Some(RiskLevel::Critical),
        _ => None,
    }
}

pub fn risk_label(risk: &RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

pub(super) fn max_risk(a: RiskLevel, b: RiskLevel) -> RiskLevel {
    if a >= b { a } else { b }
}

pub fn assess_local_command_risk(command: &str) -> (RiskLevel, String) {
    let assessment = crate::capabilities::assess_command_risk(command);
    (assessment.level, assessment.reason)
}

#[cfg(test)]
mod tests {
    use super::RiskLevel;
    use crate::{assess_agent_command_risk, parse_agent_model_output};

    #[test]
    fn local_agent_risk_overrides_unsafe_model_risk() {
        let parsed = parse_agent_model_output(
            r#"{"thought":"danger","action":"execute_command","command":"rm -rf /","riskLevel":"low","riskReason":"claimed safe"}"#,
        )
        .expect("agent response");
        let assessment = assess_agent_command_risk(&parsed, "rm -rf /");

        assert_eq!(assessment.model_risk, RiskLevel::Low);
        assert_eq!(assessment.local_risk, RiskLevel::Critical);
        assert_eq!(assessment.effective_risk, RiskLevel::Critical);
        assert!(assessment.risk_reason.unwrap().contains("claimed safe"));
    }
}
