//! Compatibility facade for UI-independent command/connection policy.
pub(crate) use zzclawterm_core::action_links::{
    ActionLinkAction, ActionLinkKind, ActionLinkMatch, ActionLinkPlatform, ActionLinkTarget,
    match_at_offset,
};

/// Plan actions for a known execution platform and localize their labels.
pub(crate) fn actions_for_match_on(
    item: &ActionLinkMatch,
    platform: ActionLinkPlatform,
) -> Vec<ActionLinkAction> {
    let mut actions = zzclawterm_core::action_links::actions_for_match_on(item, platform);
    if item.kind == ActionLinkKind::Mac {
        for action in &mut actions {
            action.label = match action.target {
                ActionLinkTarget::CopyText(_) => rust_i18n::t!("terminal.macCopy").to_string(),
                ActionLinkTarget::PasteText(_) => rust_i18n::t!("terminal.macInsert").to_string(),
                _ => action.label.clone(),
            };
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::{ActionLinkKind, ActionLinkTarget};
    use zzclawterm_core::action_links::{actions_for_match, find_action_links};

    #[test]
    fn bare_host_ports_keep_their_port_actions() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        for host in ["123.123.123.123", "example.com", "localhost"] {
            let value = format!("{host}:123");
            let found = find_action_links(&value, &matchers, true);
            assert_eq!(found.len(), 1);
            let item = &found[0];
            assert_eq!(item.kind, ActionLinkKind::HostPort);
            assert_eq!(item.value, value);
            assert_eq!(item.host.as_deref(), Some(host));
            assert_eq!(item.port.as_deref(), Some("123"));
            let actions = actions_for_match(item);
            let default = actions.iter().find(|action| action.is_default).unwrap();
            assert_eq!(default.id, "curl-http");
            assert_eq!(
                default.target,
                ActionLinkTarget::ExecuteCommand(format!("curl http://{value}"))
            );
            assert!(actions.iter().any(|action| action.id == "nc"));
            assert!(actions.iter().any(|action| action.id == "telnet"));
        }
    }

    #[test]
    fn url_priority_preserves_separate_action_links_in_text_order() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links(
            "192.168.1.1:22 https://example.com:8443/app.zip 10.0.0.1 backup.tar.gz",
            &matchers,
            true,
        );
        let entities = found
            .iter()
            .map(|item| (item.kind, item.value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            entities,
            vec![
                (ActionLinkKind::HostPort, "192.168.1.1:22"),
                (ActionLinkKind::Url, "https://example.com:8443/app.zip"),
                (ActionLinkKind::Ip, "10.0.0.1"),
                (ActionLinkKind::Archive, "backup.tar.gz"),
            ]
        );
    }

    #[test]
    fn detects_ip_and_actions() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("gateway 192.168.1.1 up", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::Ip);
        let actions = actions_for_match(&found[0]);
        assert!(actions.iter().any(|a| a.id == "ping"));
    }

    #[test]
    fn detects_host_port() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("listening on api.example.com:8080", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::HostPort);
    }

    #[test]
    fn rejects_source_location_as_host_port() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("error main.rs:42", &matchers, true);
        assert!(found.is_empty());
    }

    #[test]
    fn detects_archive() {
        use zzclawterm_core::ActionLinksMatcherSettings;
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("download app.tar.gz now", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::Archive);
    }
}
