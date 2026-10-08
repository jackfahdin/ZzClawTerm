use super::errors::format_rdp_error;
use crate::features::ZzClawTermApp;
use crate::features::remote_desktop::state::RdpCertificatePrompt;
use gpui::Context;
use zzclawterm_remote_desktop::CertificateDecision;
use zzclawterm_remote_desktop::CertificateMatchState;
use zzclawterm_remote_desktop::CertificatePromptReason;
use zzclawterm_remote_desktop::RdpCertificatePolicy;
use zzclawterm_remote_desktop::RdpCertificateRequest;
use zzclawterm_remote_desktop::RdpCertificateResponse;
use zzclawterm_remote_desktop::evaluate_certificate_match;
use zzclawterm_store::RdpCertificateMetadata;
use zzclawterm_store::RdpKnownHostCheck;
use zzclawterm_store::StoreDomain;
use zzclawterm_store::store_request;

impl ZzClawTermApp {
    pub(super) fn handle_vnc_key_request(
        &mut self,
        request: zzclawterm_remote_desktop::VncServerKeyRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(metadata) = self.session.metadata(&request.session_id) else {
            return;
        };
        let crate::models::SessionLaunchConfig::Vnc(config) = &metadata.launch_config else {
            return;
        };
        if config.host != request.host || config.port != request.port {
            let _ = self
                .remote_desktop
                .vnc_manager
                .respond_server_key(&request, false, false);
            return;
        }
        let host = request.host.clone();
        let port = request.port;
        let rejected = request.clone();
        let submitted = self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.load_vnc_known_host(&host, port)
            }),
            move |this, event, cx| {
                let Some(trust) = this
                    .remote_desktop
                    .vnc_manager
                    .trust_state(&request.session_id)
                else {
                    return;
                };
                if !trust.is_pending(&request)
                    || !this
                        .remote_desktop
                        .sessions
                        .contains_key(&request.session_id)
                {
                    return;
                }
                match event.outcome {
                    Ok(record)
                        if record.as_ref().is_some_and(|record| {
                            record
                                .sha256_fingerprint
                                .eq_ignore_ascii_case(&request.sha256_fingerprint)
                        }) =>
                    {
                        let _ = this
                            .remote_desktop
                            .vnc_manager
                            .respond_server_key(&request, true, false);
                    }
                    Ok(record) => {
                        if let Some(session) =
                            this.remote_desktop.sessions.get_mut(&request.session_id)
                        {
                            session.vnc_key_request =
                                Some((request, record.map(|record| record.sha256_fingerprint)));
                        }
                    }
                    Err(_) => {
                        let _ = this
                            .remote_desktop
                            .vnc_manager
                            .respond_server_key(&request, false, false);
                    }
                }
                cx.notify();
            },
            cx,
        );
        if !submitted {
            let _ = self
                .remote_desktop
                .vnc_manager
                .respond_server_key(&rejected, false, false);
        }
    }

    pub(in crate::features::remote_desktop) fn resolve_vnc_key_request(
        &mut self,
        request: &zzclawterm_remote_desktop::VncServerKeyRequest,
        accept: bool,
        remember: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.remote_desktop.sessions.get_mut(&request.session_id) else {
            return;
        };
        if !session
            .vnc_key_request
            .as_ref()
            .is_some_and(|(pending, _)| pending == request)
        {
            return;
        }
        let previous = session
            .vnc_key_request
            .take()
            .and_then(|(_, previous)| previous);
        session.vnc_trust_previous = previous;
        let _ = self
            .remote_desktop
            .vnc_manager
            .respond_server_key(request, accept, remember);
        cx.notify();
    }

    pub(super) fn handle_rdp_certificate_request(
        &mut self,
        session_id: &str,
        request: RdpCertificateRequest,
        cx: &mut Context<Self>,
    ) {
        let policy = self
            .session
            .metadata(session_id)
            .and_then(|metadata| match &metadata.launch_config {
                crate::models::SessionLaunchConfig::Rdp(config) => Some(config.certificate_policy),
                _ => None,
            })
            .unwrap_or(RdpCertificatePolicy::Prompt);
        if policy == RdpCertificatePolicy::Insecure {
            self.apply_rdp_certificate_check(
                session_id,
                request,
                policy,
                RdpKnownHostCheck::UnknownHost,
                cx,
            );
            return;
        }
        let host = request.host.clone();
        let port = request.port;
        let fingerprint = request.sha256_fingerprint.clone();
        let request_id = request.request_id.clone();
        let failure_request_id = request_id.clone();
        let session_id = session_id.to_string();
        let submitted = self.submit_store_request(
            0,
            store_request(StoreDomain::Security, move |store| {
                store.check_rdp_known_host(&host, port, &fingerprint)
            }),
            move |this, event, cx| match event.outcome {
                Ok(check) if this.remote_desktop.sessions.contains_key(&session_id) => {
                    this.apply_rdp_certificate_check(&session_id, request, policy, check, cx);
                }
                Ok(_) => {}
                Err(error) => {
                    this.shell
                        .set_status(format!("RDP certificate verification failed: {error}"));
                    let _ = this
                        .remote_desktop
                        .manager
                        .respond_certificate(&request_id, RdpCertificateResponse::Reject);
                    cx.notify();
                }
            },
            cx,
        );
        if !submitted {
            let _ = self
                .remote_desktop
                .manager
                .respond_certificate(&failure_request_id, RdpCertificateResponse::Reject);
        }
    }

    pub(super) fn apply_rdp_certificate_check(
        &mut self,
        session_id: &str,
        request: RdpCertificateRequest,
        policy: RdpCertificatePolicy,
        check: RdpKnownHostCheck,
        cx: &mut Context<Self>,
    ) {
        let match_state = match check {
            RdpKnownHostCheck::Match => CertificateMatchState::Match,
            RdpKnownHostCheck::UnknownHost => CertificateMatchState::FirstUse,
            RdpKnownHostCheck::Changed {
                remembered_fingerprint,
            } => CertificateMatchState::Changed {
                remembered_fingerprint,
            },
        };
        let expected_previous_fingerprint = match &match_state {
            CertificateMatchState::Changed {
                remembered_fingerprint,
            } => Some(remembered_fingerprint.clone()),
            CertificateMatchState::FirstUse | CertificateMatchState::Match => None,
        };
        let evaluation =
            evaluate_certificate_match(policy, match_state, &request.sha256_fingerprint);
        match evaluation.decision {
            CertificateDecision::Accept => {
                let _ = self
                    .remote_desktop
                    .manager
                    .respond_certificate(&request.request_id, RdpCertificateResponse::TrustOnce);
            }
            CertificateDecision::AcceptAndRemember => {
                self.persist_rdp_certificate_and_respond(
                    request,
                    expected_previous_fingerprint,
                    cx,
                );
            }
            CertificateDecision::Reject => {
                let _ = self
                    .remote_desktop
                    .manager
                    .respond_certificate(&request.request_id, RdpCertificateResponse::Reject);
            }
            CertificateDecision::Prompt => {
                let Some(reason) = evaluation.prompt_reason else {
                    let _ = self
                        .remote_desktop
                        .manager
                        .respond_certificate(&request.request_id, RdpCertificateResponse::Reject);
                    return;
                };
                if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
                    session.certificate_request = Some(RdpCertificatePrompt { request, reason });
                }
            }
        }
    }

    pub(in crate::features) fn resolve_rdp_certificate(
        &mut self,
        session_id: &str,
        response: RdpCertificateResponse,
        cx: &mut Context<Self>,
    ) {
        let prompt = self
            .remote_desktop
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.certificate_request.take());
        let Some(prompt) = prompt else {
            return;
        };
        let expected_previous_fingerprint = match &prompt.reason {
            CertificatePromptReason::FirstUse => None,
            CertificatePromptReason::Changed {
                previous_fingerprint,
                ..
            } => Some(previous_fingerprint.clone()),
        };
        if response == RdpCertificateResponse::TrustAndRemember {
            self.persist_rdp_certificate_and_respond(
                prompt.request,
                expected_previous_fingerprint,
                cx,
            );
            return;
        }
        let response = if matches!(prompt.reason, CertificatePromptReason::Changed { .. })
            && response == RdpCertificateResponse::TrustOnce
        {
            RdpCertificateResponse::Reject
        } else {
            response
        };
        if let Err(error) = self
            .remote_desktop
            .manager
            .respond_certificate(&prompt.request.request_id, response)
        {
            self.shell.set_status(format_rdp_error(&error));
        }
    }

    pub(super) fn persist_rdp_certificate_and_respond(
        &mut self,
        request: RdpCertificateRequest,
        expected_previous_fingerprint: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let request_id = request.request_id.clone();
        let failure_request_id = request_id.clone();
        let host = request.host;
        let port = request.port;
        let fingerprint = request.sha256_fingerprint;
        let metadata = RdpCertificateMetadata {
            subject: request.subject,
            issuer: request.issuer,
            valid_from: request.valid_from,
            valid_to: request.valid_to,
        };
        let submitted = self.submit_store_request(
            0,
            zzclawterm_store::store_mutation(StoreDomain::Security, move |store| {
                store.replace_rdp_known_host_if_matches(
                    &host,
                    port,
                    expected_previous_fingerprint.as_deref(),
                    &fingerprint,
                    metadata,
                )
            }),
            move |this, event, cx| {
                let response = match event.outcome {
                    Ok(true) => RdpCertificateResponse::TrustAndRemember,
                    Ok(false) => {
                        this.shell.set_status(
                            "RDP certificate changed again before confirmation; connection rejected"
                                .to_string(),
                        );
                        RdpCertificateResponse::Reject
                    }
                    Err(error) => {
                        this.shell.set_status(format!(
                            "RDP certificate could not be remembered: {error}"
                        ));
                        RdpCertificateResponse::Reject
                    }
                };
                if let Err(error) = this
                    .remote_desktop
                    .manager
                    .respond_certificate(&request_id, response)
                {
                    this.shell.set_status(format_rdp_error(&error));
                }
                cx.notify();
            },
            cx,
        );
        if !submitted {
            let _ = self
                .remote_desktop
                .manager
                .respond_certificate(&failure_request_id, RdpCertificateResponse::Reject);
        }
    }
}
