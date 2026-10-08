//! One trust decision per helper generation; persistence shares its invalidation lock.
use crate::VncServerKeyRequest;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Default)]
struct State {
    closed: bool,
    generation: u64,
    epoch: u64,
    request: Option<VncServerKeyRequest>,
    accepted: bool,
    remember: bool,
    authenticated: bool,
}

#[cfg(test)]
mod tests {
    use super::VncTrustState;
    use crate::{VncControlMessage, VncServerKeyRequest, decode_vnc_control, encode_vnc_control};

    fn request(generation: u64) -> VncServerKeyRequest {
        VncServerKeyRequest {
            session_id: "session-a".into(),
            generation,
            request_id: format!("request-{generation}"),
            host: "localhost".into(),
            port: 5900,
            sha256_fingerprint: format!("SHA256:{}", "ab".repeat(32)),
            key_bits: 2048,
        }
    }

    #[test]
    fn persistence_requires_acceptance_authentication_and_remember() {
        for remember in [false, true] {
            let state = VncTrustState::default();
            let request = request(1);
            assert!(state.request(&request));
            assert!(!state.authenticated(&request));
            assert!(state.commit_if_current(&request, |_| ()).is_none());
            assert!(state.respond(&request, true, remember));
            assert!(state.commit_if_current(&request, |_| ()).is_none());
            assert!(state.authenticated(&request));
            assert_eq!(
                state.commit_if_current(&request, |_| 42),
                remember.then_some(42)
            );
            assert!(state.commit_if_current(&request, |_| ()).is_none());
        }
    }

    #[test]
    fn rejection_disconnect_replacement_and_wrong_session_never_commit() {
        let state = VncTrustState::default();
        let first = request(1);
        assert!(state.request(&first));
        let mut wrong = first.clone();
        wrong.session_id = "session-b".into();
        assert!(!state.respond(&wrong, true, true));
        assert!(state.respond(&first, false, true));
        assert!(!state.respond(&first, true, true));
        assert!(!state.authenticated(&first));
        let second = request(2);
        assert!(state.request(&second));
        assert!(state.respond(&second, true, true));
        assert!(state.authenticated(&second));
        state.invalidate(false);
        assert!(
            state
                .commit_if_current(&second, |_| panic!("stale commit"))
                .is_none()
        );
        assert!(!state.request(&second));
        let third = request(3);
        assert!(state.request(&third));
        assert!(!state.authenticated(&second));
        state.invalidate(true);
        assert!(!state.request(&request(4)));
        assert!(!state.respond(&third, true, true));
    }

    #[test]
    fn cancellation_during_storage_work_revokes_the_commit_predicate_without_blocking() {
        let state = VncTrustState::default();
        let request = request(1);
        assert!(state.request(&request));
        assert!(state.respond(&request, true, true));
        assert!(state.authenticated(&request));
        assert_eq!(
            state.commit_if_current(&request, |current| {
                assert!(current());
                state.cancel();
                current()
            }),
            Some(false)
        );
        assert!(!state.is_pending(&request));
    }

    #[test]
    fn trust_challenge_and_decision_roundtrip_through_typed_ipc() {
        let request = request(1);
        for message in [
            VncControlMessage::ServerKeyRequest(request.clone()),
            VncControlMessage::ServerKeyAuthenticated(request.clone()),
            VncControlMessage::ServerKeyResponse {
                session_id: request.session_id,
                generation: 1,
                request_id: request.request_id,
                accept: false,
            },
        ] {
            let packet = encode_vnc_control(&message).unwrap();
            let decoded = decode_vnc_control(&packet).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                serde_json::to_value(message).unwrap()
            );
        }
    }
}

#[derive(Default)]
pub struct VncTrustState(Mutex<State>, AtomicBool, AtomicU64);

impl VncTrustState {
    pub(crate) fn request(&self, request: &VncServerKeyRequest) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        if self.1.load(Ordering::Acquire) || state.closed || request.generation <= state.generation
        {
            return false;
        }
        state.generation = request.generation;
        state.epoch = self.2.load(Ordering::Acquire);
        state.request = Some(request.clone());
        state.accepted = false;
        state.remember = false;
        state.authenticated = false;
        true
    }
    pub fn is_pending(&self, request: &VncServerKeyRequest) -> bool {
        if self.1.load(Ordering::Acquire) {
            return false;
        }
        self.0.try_lock().is_ok_and(|state| {
            !state.closed && !state.accepted && state.request.as_ref() == Some(request)
        })
    }
    pub(crate) fn respond(
        &self,
        request: &VncServerKeyRequest,
        accept: bool,
        remember: bool,
    ) -> bool {
        if self.1.load(Ordering::Acquire) {
            return false;
        }
        let Ok(mut state) = self.0.try_lock() else {
            return false;
        };
        if state.closed || state.accepted || state.request.as_ref() != Some(request) {
            return false;
        }
        state.accepted = accept;
        state.remember = accept && remember;
        if !accept {
            state.request = None;
        }
        true
    }
    pub(crate) fn authenticated(&self, request: &VncServerKeyRequest) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        if self.1.load(Ordering::Acquire)
            || state.closed
            || !state.accepted
            || state.request.as_ref() != Some(request)
        {
            return false;
        }
        state.authenticated = true;
        true
    }
    /// Nonblocking UI cancellation. A commit already linearized before this
    /// cancellation may finish; queued storage work must never start afterwards.
    pub(crate) fn cancel(&self) {
        self.1.store(true, Ordering::Release);
    }

    pub(crate) fn invalidate(&self, closing: bool) {
        self.2.fetch_add(1, Ordering::AcqRel);
        if closing {
            self.cancel();
        }
        if let Ok(mut state) = self.0.lock() {
            state.closed |= closing;
            state.request = None;
            state.authenticated = false;
            state.accepted = false;
            state.remember = false;
        }
    }
    /// Call only on a storage worker. Invalidation and commit are serialized, so
    /// a late worker cannot restore trust after disconnect or replacement.
    pub fn commit_if_current<T>(
        &self,
        request: &VncServerKeyRequest,
        commit: impl FnOnce(&dyn Fn() -> bool) -> T,
    ) -> Option<T> {
        let mut state = self.0.lock().ok()?;
        if self.1.load(Ordering::Acquire)
            || state.closed
            || !state.authenticated
            || !state.remember
            || state.request.as_ref() != Some(request)
        {
            return None;
        }
        let epoch = state.epoch;
        let current = || !self.1.load(Ordering::Acquire) && self.2.load(Ordering::Acquire) == epoch;
        if !current() {
            return None;
        }
        let result = commit(&current);
        state.remember = false;
        Some(result)
    }
}
