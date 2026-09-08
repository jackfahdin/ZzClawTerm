use crate::RdpCertificatePolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CertificateDecision {
    Accept,
    AcceptAndRemember,
    Prompt,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertificateMatchState {
    FirstUse,
    Match,
    Changed { remembered_fingerprint: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CertificatePromptReason {
    FirstUse,
    Changed {
        previous_fingerprint: String,
        presented_fingerprint: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateEvaluation {
    pub decision: CertificateDecision,
    pub prompt_reason: Option<CertificatePromptReason>,
}

pub fn evaluate_certificate_match(
    policy: RdpCertificatePolicy,
    state: CertificateMatchState,
    presented_fingerprint: &str,
) -> CertificateEvaluation {
    let decision = match (policy, &state) {
        (RdpCertificatePolicy::Insecure, _) => CertificateDecision::Accept,
        (RdpCertificatePolicy::TrustOnFirstUse, CertificateMatchState::Match) => {
            CertificateDecision::Accept
        }
        (RdpCertificatePolicy::TrustOnFirstUse, CertificateMatchState::FirstUse) => {
            CertificateDecision::AcceptAndRemember
        }
        (RdpCertificatePolicy::TrustOnFirstUse, CertificateMatchState::Changed { .. }) => {
            CertificateDecision::Reject
        }
        (
            RdpCertificatePolicy::Strict | RdpCertificatePolicy::RejectChanged,
            CertificateMatchState::Match,
        ) => CertificateDecision::Accept,
        (RdpCertificatePolicy::Strict | RdpCertificatePolicy::RejectChanged, _) => {
            CertificateDecision::Reject
        }
        (RdpCertificatePolicy::Prompt, CertificateMatchState::Match) => CertificateDecision::Accept,
        (RdpCertificatePolicy::Prompt, CertificateMatchState::FirstUse) => {
            CertificateDecision::Prompt
        }
        (RdpCertificatePolicy::Prompt, CertificateMatchState::Changed { .. }) => {
            CertificateDecision::Prompt
        }
    };
    let prompt_reason = if decision == CertificateDecision::Prompt {
        match state {
            CertificateMatchState::FirstUse => Some(CertificatePromptReason::FirstUse),
            CertificateMatchState::Changed {
                remembered_fingerprint,
            } => Some(CertificatePromptReason::Changed {
                previous_fingerprint: remembered_fingerprint,
                presented_fingerprint: presented_fingerprint.to_string(),
            }),
            CertificateMatchState::Match => None,
        }
    } else {
        None
    };
    CertificateEvaluation {
        decision,
        prompt_reason,
    }
}

pub fn evaluate_certificate(
    policy: RdpCertificatePolicy,
    remembered_fingerprint: Option<&str>,
    presented_fingerprint: &str,
) -> CertificateDecision {
    let state = match remembered_fingerprint {
        None => CertificateMatchState::FirstUse,
        Some(remembered) if remembered.eq_ignore_ascii_case(presented_fingerprint) => {
            CertificateMatchState::Match
        }
        Some(remembered) => CertificateMatchState::Changed {
            remembered_fingerprint: remembered.to_string(),
        },
    };
    evaluate_certificate_match(policy, state, presented_fingerprint).decision
}

#[cfg(test)]
mod tests {
    use crate::{
        CertificateDecision, CertificateMatchState, CertificatePromptReason, RdpCertificatePolicy,
        evaluate_certificate, evaluate_certificate_match,
    };

    #[test]
    fn prompt_distinguishes_first_use_from_changed_certificates() {
        let first_use = evaluate_certificate_match(
            RdpCertificatePolicy::Prompt,
            CertificateMatchState::FirstUse,
            "sha256:new",
        );
        assert_eq!(first_use.decision, CertificateDecision::Prompt);
        assert_eq!(
            first_use.prompt_reason,
            Some(CertificatePromptReason::FirstUse)
        );

        let changed = evaluate_certificate_match(
            RdpCertificatePolicy::Prompt,
            CertificateMatchState::Changed {
                remembered_fingerprint: "sha256:old".to_string(),
            },
            "sha256:new",
        );
        assert_eq!(changed.decision, CertificateDecision::Prompt);
        assert_eq!(
            changed.prompt_reason,
            Some(CertificatePromptReason::Changed {
                previous_fingerprint: "sha256:old".to_string(),
                presented_fingerprint: "sha256:new".to_string(),
            })
        );
    }

    #[test]
    fn changed_certificates_are_rejected_by_non_prompt_secure_policies() {
        for policy in [
            RdpCertificatePolicy::TrustOnFirstUse,
            RdpCertificatePolicy::Strict,
            RdpCertificatePolicy::RejectChanged,
        ] {
            let evaluation = evaluate_certificate_match(
                policy,
                CertificateMatchState::Changed {
                    remembered_fingerprint: "sha256:old".to_string(),
                },
                "sha256:new",
            );
            assert_eq!(evaluation.decision, CertificateDecision::Reject);
            assert_eq!(evaluation.prompt_reason, None);
        }
    }

    #[test]
    fn enforces_certificate_policies() {
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::TrustOnFirstUse, None, "a"),
            CertificateDecision::AcceptAndRemember
        );
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::TrustOnFirstUse, Some("a"), "b"),
            CertificateDecision::Reject
        );
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::Strict, Some("a"), "a"),
            CertificateDecision::Accept
        );
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::Strict, None, "a"),
            CertificateDecision::Reject
        );
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::Prompt, None, "a"),
            CertificateDecision::Prompt
        );
        assert_eq!(
            evaluate_certificate(RdpCertificatePolicy::Insecure, Some("a"), "b"),
            CertificateDecision::Accept
        );
    }
}
