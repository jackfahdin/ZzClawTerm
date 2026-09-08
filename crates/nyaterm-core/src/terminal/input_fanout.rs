pub fn terminal_input_fanout_status(
    action: &str,
    byte_count: usize,
    synced: usize,
    failed: usize,
) -> String {
    if synced > 0 || failed > 0 {
        if failed == 0 {
            format!("{action} {byte_count} byte(s) + synced {synced} peer(s)")
        } else {
            format!("{action} {byte_count} byte(s), synced {synced} peer(s), {failed} failed")
        }
    } else {
        format!("{action} {byte_count} byte(s)")
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_input_fanout_status;

    #[test]
    fn fanout_status_omits_sync_when_no_peer_was_attempted() {
        assert_eq!(
            terminal_input_fanout_status("sent", 4, 0, 0),
            "sent 4 byte(s)"
        );
    }

    #[test]
    fn fanout_status_reports_successful_sync_peers() {
        assert_eq!(
            terminal_input_fanout_status("sent", 8, 2, 0),
            "sent 8 byte(s) + synced 2 peer(s)"
        );
    }

    #[test]
    fn fanout_status_reports_partial_peer_failures() {
        assert_eq!(
            terminal_input_fanout_status("pasted", 16, 1, 3),
            "pasted 16 byte(s), synced 1 peer(s), 3 failed"
        );
    }
}
