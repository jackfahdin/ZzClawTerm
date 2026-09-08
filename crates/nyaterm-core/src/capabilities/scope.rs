use std::collections::HashSet;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySession {
    pub id: String,
    pub owner_window_label: Option<String>,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityScope {
    Explicit {
        session_ids: HashSet<String>,
        default_session_id: Option<String>,
    },
    CurrentWindow {
        owner_window_label: String,
    },
    AllSessions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityScopeSnapshot {
    pub session_ids: HashSet<String>,
    pub default_session_id: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CapabilityScopeError {
    #[error("session '{0}' is not live or is outside the current capability scope")]
    SessionUnavailable(String),
    #[error("no default session is available; provide sessionId explicitly")]
    MissingDefaultSession,
}

impl CapabilityScope {
    pub fn explicit(
        session_ids: impl IntoIterator<Item = String>,
        default_session_id: Option<String>,
    ) -> Self {
        let session_ids = session_ids.into_iter().collect::<HashSet<_>>();
        let default_session_id = default_session_id.filter(|id| session_ids.contains(id));
        Self::Explicit {
            session_ids,
            default_session_id,
        }
    }

    pub fn current_window(owner_window_label: impl Into<String>) -> Self {
        Self::CurrentWindow {
            owner_window_label: owner_window_label.into(),
        }
    }

    /// Resolve against the current host session inventory. Callers must do this
    /// again for every capability invocation; snapshots are not authorization
    /// tokens and must not outlive the host state used to produce them.
    pub fn resolve(&self, sessions: &[CapabilitySession]) -> CapabilityScopeSnapshot {
        match self {
            Self::Explicit {
                session_ids,
                default_session_id,
            } => {
                let live_ids = sessions
                    .iter()
                    .filter(|session| session.live)
                    .map(|session| session.id.as_str())
                    .collect::<HashSet<_>>();
                let session_ids = session_ids
                    .iter()
                    .filter(|id| live_ids.contains(id.as_str()))
                    .cloned()
                    .collect::<HashSet<_>>();
                let default_session_id = default_session_id
                    .as_ref()
                    .filter(|id| session_ids.contains(*id))
                    .cloned();
                CapabilityScopeSnapshot {
                    session_ids,
                    default_session_id,
                }
            }
            Self::CurrentWindow { owner_window_label } => {
                dynamic_snapshot(sessions.iter().filter(|session| {
                    session.live
                        && session.owner_window_label.as_deref()
                            == Some(owner_window_label.as_str())
                }))
            }
            Self::AllSessions => dynamic_snapshot(sessions.iter().filter(|session| session.live)),
        }
    }
}

impl CapabilityScopeSnapshot {
    pub fn require(&self, session_id: &str) -> Result<(), CapabilityScopeError> {
        if self.session_ids.contains(session_id) {
            Ok(())
        } else {
            Err(CapabilityScopeError::SessionUnavailable(
                session_id.to_string(),
            ))
        }
    }

    pub fn resolve_session(&self, requested: Option<&str>) -> Result<String, CapabilityScopeError> {
        if let Some(id) = requested.map(str::trim).filter(|id| !id.is_empty()) {
            self.require(id)?;
            return Ok(id.to_string());
        }
        self.default_session_id
            .clone()
            .ok_or(CapabilityScopeError::MissingDefaultSession)
    }
}

fn dynamic_snapshot<'a>(
    sessions: impl Iterator<Item = &'a CapabilitySession>,
) -> CapabilityScopeSnapshot {
    let session_ids = sessions
        .map(|session| session.id.clone())
        .collect::<HashSet<_>>();
    let default_session_id = (session_ids.len() == 1)
        .then(|| session_ids.iter().next().cloned())
        .flatten();
    CapabilityScopeSnapshot {
        session_ids,
        default_session_id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{CapabilityScope, CapabilityScopeError, CapabilitySession};

    fn session(id: &str, owner: &str, live: bool) -> CapabilitySession {
        CapabilitySession {
            id: id.to_string(),
            owner_window_label: Some(owner.to_string()),
            live,
        }
    }

    #[test]
    fn explicit_scope_is_frozen_but_drops_closed_sessions() {
        let scope =
            CapabilityScope::explicit(["a".to_string(), "b".to_string()], Some("a".to_string()));
        let initial = scope.resolve(&[session("a", "main", true), session("b", "main", true)]);
        assert_eq!(initial.resolve_session(None).unwrap(), "a");

        let changed = scope.resolve(&[
            session("a", "main", false),
            session("b", "main", true),
            session("c", "main", true),
        ]);
        assert_eq!(changed.session_ids, HashSet::from(["b".to_string()]));
        assert_eq!(
            changed.resolve_session(None),
            Err(CapabilityScopeError::MissingDefaultSession)
        );
        assert!(changed.resolve_session(Some("c")).is_err());
    }

    #[test]
    fn dynamic_scopes_include_only_current_live_sessions() {
        let sessions = [
            session("a", "main", true),
            session("b", "other", true),
            session("closed", "main", false),
        ];
        let current = CapabilityScope::current_window("main").resolve(&sessions);
        assert_eq!(current.session_ids, HashSet::from(["a".to_string()]));
        assert_eq!(current.default_session_id.as_deref(), Some("a"));

        let all = CapabilityScope::AllSessions.resolve(&sessions);
        assert_eq!(
            all.session_ids,
            HashSet::from(["a".to_string(), "b".to_string()])
        );
        assert!(all.default_session_id.is_none());
    }
}
