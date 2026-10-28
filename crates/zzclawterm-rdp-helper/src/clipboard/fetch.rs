use std::time::{Duration, Instant};

use ironrdp_cliprdr::pdu::ClipboardFormatId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FetchKind {
    Text,
    FileList,
}

#[derive(Debug, Clone, Copy)]
struct FetchRequest {
    generation: u64,
    kind: FetchKind,
    format: ClipboardFormatId,
}

#[derive(Debug)]
struct PendingFetch {
    request: FetchRequest,
    sent_at: Instant,
    timeout_reported: bool,
}

/// Serializes remote `FormatData` fetches.
///
/// CLIPRDR matches a `FormatDataResponse` to the requests in send order and the
/// response carries no format of its own. Asking for the file list of every new
/// `FormatList` while an earlier request is still unanswered makes a late reply
/// look like the answer to the newest one, so the previous clipboard's files win.
/// Keep one request in flight, drop replies aimed at a superseded list, and send
/// only the newest queued request once the current one is settled.
#[derive(Debug, Default)]
pub(super) struct RemoteFetchState {
    generation: u64,
    pending: Option<PendingFetch>,
    latest: Option<FetchRequest>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct FetchReply {
    pub accepted: bool,
    pub next: Option<ClipboardFormatId>,
}

impl RemoteFetchState {
    /// Opens a new remote clipboard generation and returns the format to request
    /// now, or `None` when an earlier request is still waiting for its reply.
    pub fn begin_format_list(
        &mut self,
        desired: Option<(FetchKind, ClipboardFormatId)>,
    ) -> Option<ClipboardFormatId> {
        self.generation = self.generation.wrapping_add(1);
        let Some((kind, format)) = desired else {
            self.latest = None;
            return None;
        };
        let request = FetchRequest {
            generation: self.generation,
            kind,
            format,
        };
        if self.pending.is_some() {
            self.latest = Some(request);
            None
        } else {
            self.pending = Some(PendingFetch {
                request,
                sent_at: Instant::now(),
                timeout_reported: false,
            });
            Some(format)
        }
    }

    /// Settles the oldest outstanding request with the reply that just arrived.
    pub fn finish_reply(&mut self, kind: FetchKind, is_error: bool) -> FetchReply {
        let accepted = self.pending.take().is_some_and(|pending| {
            !is_error
                && pending.request.generation == self.generation
                && pending.request.kind == kind
        });
        let next = self.latest.take().map(|request| self.start(request));
        FetchReply { accepted, next }
    }

    /// Reports a timeout once, retaining the outstanding request until its reply.
    /// FormatDataResponse has no request identifier: reusing this slot on timeout
    /// would let a late response masquerade as the answer to the newest request.
    /// If no response ever arrives, the channel/session must be reset to resume.
    pub fn expire(&mut self, timeout: Duration) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.timeout_reported || pending.sent_at.elapsed() < timeout {
            return false;
        }
        pending.timeout_reported = true;
        true
    }

    fn start(&mut self, request: FetchRequest) -> ClipboardFormatId {
        self.pending = Some(PendingFetch {
            request,
            sent_at: Instant::now(),
            timeout_reported: false,
        });
        request.format
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ironrdp_cliprdr::pdu::ClipboardFormatId;

    use super::{FetchKind, RemoteFetchState};

    fn format(id: u32) -> ClipboardFormatId {
        ClipboardFormatId::new(id)
    }

    #[test]
    fn a_new_format_list_queues_until_the_outstanding_reply_arrives() {
        let mut state = RemoteFetchState::default();
        assert_eq!(
            state.begin_format_list(Some((FetchKind::FileList, format(1)))),
            Some(format(1))
        );
        assert_eq!(
            state.begin_format_list(Some((FetchKind::FileList, format(2)))),
            None
        );

        let stale = state.finish_reply(FetchKind::FileList, false);
        assert!(!stale.accepted);
        assert_eq!(stale.next, Some(format(2)));

        let current = state.finish_reply(FetchKind::FileList, false);
        assert!(current.accepted);
        assert_eq!(current.next, None);
    }

    #[test]
    fn the_newest_list_replaces_queued_intermediate_lists() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::FileList, format(1))));
        state.begin_format_list(Some((FetchKind::FileList, format(2))));
        state.begin_format_list(Some((FetchKind::FileList, format(3))));

        let reply = state.finish_reply(FetchKind::FileList, false);
        assert!(!reply.accepted);
        assert_eq!(reply.next, Some(format(3)));
    }

    #[test]
    fn replies_to_a_kind_we_did_not_ask_for_are_not_accepted() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::FileList, format(1))));

        let reply = state.finish_reply(FetchKind::Text, false);
        assert!(!reply.accepted);
        assert_eq!(reply.next, None);
    }

    #[test]
    fn errors_are_never_accepted_but_still_settle_the_request() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::Text, format(1))));

        let reply = state.finish_reply(FetchKind::Text, true);
        assert!(!reply.accepted);
        assert_eq!(reply.next, None);
    }

    #[test]
    fn a_timeout_keeps_the_old_request_until_its_late_reply_is_drained() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::FileList, format(1))));
        state.begin_format_list(Some((FetchKind::FileList, format(2))));

        assert!(state.expire(Duration::ZERO));
        let stale = state.finish_reply(FetchKind::FileList, false);
        assert!(
            !stale.accepted,
            "a late reply must not become the new clipboard"
        );
        assert_eq!(stale.next, Some(format(2)));
        assert!(state.finish_reply(FetchKind::FileList, false).accepted);
    }

    #[test]
    fn missing_replies_keep_only_the_latest_list_and_report_timeout_once() {
        let mut state = RemoteFetchState::default();
        assert!(!state.expire(Duration::ZERO));
        state.begin_format_list(Some((FetchKind::FileList, format(1))));
        assert!(!state.expire(Duration::MAX));
        assert!(state.expire(Duration::ZERO));
        for id in 2..10_000 {
            assert_eq!(
                state.begin_format_list(Some((FetchKind::FileList, format(id)))),
                None
            );
            assert!(!state.expire(Duration::ZERO));
        }
        let stale = state.finish_reply(FetchKind::FileList, false);
        assert!(!stale.accepted);
        assert_eq!(stale.next, Some(format(9_999)));
        assert!(state.expire(Duration::ZERO));
    }

    #[test]
    fn a_late_unsuperseded_reply_still_belongs_to_its_original_request() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::Text, format(1))));
        assert!(state.expire(Duration::ZERO));
        let reply = state.finish_reply(FetchKind::Text, false);
        assert!(reply.accepted);
        assert_eq!(reply.next, None);
        assert!(!state.expire(Duration::ZERO));
    }

    #[test]
    fn a_format_list_without_a_wanted_format_does_not_send_an_old_queued_request() {
        let mut state = RemoteFetchState::default();
        state.begin_format_list(Some((FetchKind::FileList, format(1))));
        state.begin_format_list(Some((FetchKind::FileList, format(2))));
        state.begin_format_list(None);

        let reply = state.finish_reply(FetchKind::FileList, false);
        assert!(!reply.accepted);
        assert_eq!(reply.next, None);
        assert!(!state.finish_reply(FetchKind::FileList, false).accepted);
    }
}
