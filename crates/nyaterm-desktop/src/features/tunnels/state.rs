use std::sync::Arc;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use nyaterm_core::{ProxyConfig, ProxyGroup, TunnelConfig, TunnelGroup, uuid};
use nyaterm_transport::{SshTunnelInfo, SshTunnelManager};

use crate::features::runtime_jobs::TunnelJobResult;

pub(in crate::features) struct TunnelFeatureState {
    catalog: TunnelCatalogState,
    manager: Arc<SshTunnelManager>,
    tx: UnboundedSender<TunnelJobResult>,
    /// Taken once by `NyaTermApp::start_tunnel_event_drain`, which owns
    /// delivery from then on. `None` afterwards, so a second start is a no-op.
    rx: Option<UnboundedReceiver<TunnelJobResult>>,
    pending: Vec<String>,
}

pub(in crate::features) struct TunnelCatalogState {
    tunnels: Vec<TunnelConfig>,
    tunnel_groups: Vec<TunnelGroup>,
    proxies: Vec<ProxyConfig>,
    proxy_groups: Vec<ProxyGroup>,
}

pub(in crate::features) struct TunnelGroupRemoval {
    groups: Vec<TunnelGroup>,
    tunnels: Vec<TunnelConfig>,
    deleted_tunnel_ids: Vec<String>,
}

impl TunnelGroupRemoval {
    pub(in crate::features) fn groups(&self) -> &[TunnelGroup] {
        &self.groups
    }

    pub(in crate::features) fn tunnels(&self) -> &[TunnelConfig] {
        &self.tunnels
    }
}

pub(in crate::features) struct ProxyGroupRemoval {
    groups: Vec<ProxyGroup>,
    proxies: Vec<ProxyConfig>,
    deleted_proxy_ids: Vec<String>,
}

impl ProxyGroupRemoval {
    pub(in crate::features) fn groups(&self) -> &[ProxyGroup] {
        &self.groups
    }

    pub(in crate::features) fn proxies(&self) -> &[ProxyConfig] {
        &self.proxies
    }
}

impl TunnelCatalogState {
    pub(in crate::features) fn new(
        tunnels: Vec<TunnelConfig>,
        tunnel_groups: Vec<TunnelGroup>,
        proxies: Vec<ProxyConfig>,
        proxy_groups: Vec<ProxyGroup>,
    ) -> Self {
        Self {
            tunnels,
            tunnel_groups,
            proxies,
            proxy_groups,
        }
    }
}

impl TunnelFeatureState {
    pub(in crate::features) fn new(catalog: TunnelCatalogState) -> Self {
        let (tx, rx) = unbounded();
        Self {
            catalog,
            manager: Arc::new(SshTunnelManager::new()),
            tx,
            rx: Some(rx),
            pending: Vec::new(),
        }
    }

    pub(in crate::features) fn job_sender(&self) -> UnboundedSender<TunnelJobResult> {
        self.tx.clone()
    }

    pub(in crate::features) fn tunnels(&self) -> &[TunnelConfig] {
        &self.catalog.tunnels
    }

    pub(in crate::features) fn tunnel_groups(&self) -> &[TunnelGroup] {
        &self.catalog.tunnel_groups
    }

    pub(in crate::features) fn proxies(&self) -> &[ProxyConfig] {
        &self.catalog.proxies
    }

    pub(in crate::features) fn proxy_groups(&self) -> &[ProxyGroup] {
        &self.catalog.proxy_groups
    }

    pub(in crate::features) fn replace_loaded_catalog(
        &mut self,
        tunnels: Vec<TunnelConfig>,
        tunnel_groups: Vec<TunnelGroup>,
        proxies: Vec<ProxyConfig>,
        proxy_groups: Vec<ProxyGroup>,
    ) {
        self.catalog = TunnelCatalogState::new(tunnels, tunnel_groups, proxies, proxy_groups);
    }

    pub(in crate::features) fn has_tunnel_group(&self, group_id: &str) -> bool {
        self.catalog
            .tunnel_groups
            .iter()
            .any(|group| group.id == group_id)
    }

    pub(in crate::features) fn has_proxy_group(&self, group_id: &str) -> bool {
        self.catalog
            .proxy_groups
            .iter()
            .any(|group| group.id == group_id)
    }

    pub(in crate::features) fn tunnels_moved_to_group(
        &self,
        tunnel_id: &str,
        group_id: Option<String>,
    ) -> Option<Vec<TunnelConfig>> {
        let mut tunnels = self.catalog.tunnels.clone();
        tunnels
            .iter_mut()
            .find(|tunnel| tunnel.id == tunnel_id)?
            .group_id = group_id;
        Some(tunnels)
    }

    pub(in crate::features) fn proxies_moved_to_group(
        &self,
        proxy_id: &str,
        group_id: Option<String>,
    ) -> Option<Vec<ProxyConfig>> {
        let mut proxies = self.catalog.proxies.clone();
        proxies
            .iter_mut()
            .find(|proxy| proxy.id == proxy_id)?
            .group_id = group_id;
        Some(proxies)
    }

    pub(in crate::features) fn tunnels_without(
        &self,
        tunnel_id: &str,
    ) -> (Vec<TunnelConfig>, bool) {
        let mut tunnels = self.catalog.tunnels.clone();
        let before = tunnels.len();
        tunnels.retain(|tunnel| tunnel.id != tunnel_id);
        let deleted = tunnels.len() != before;
        (tunnels, deleted)
    }

    pub(in crate::features) fn proxies_without(&self, proxy_id: &str) -> (Vec<ProxyConfig>, bool) {
        let mut proxies = self.catalog.proxies.clone();
        let before = proxies.len();
        proxies.retain(|proxy| proxy.id != proxy_id);
        let deleted = proxies.len() != before;
        (proxies, deleted)
    }

    pub(in crate::features) fn tunnels_with_upsert(
        &self,
        tunnel: TunnelConfig,
    ) -> Vec<TunnelConfig> {
        let mut tunnels = self.catalog.tunnels.clone();
        if let Some(existing) = tunnels.iter_mut().find(|existing| existing.id == tunnel.id) {
            *existing = tunnel;
        } else {
            tunnels.push(tunnel);
        }
        tunnels
    }

    pub(in crate::features) fn proxies_with_upsert(&self, proxy: ProxyConfig) -> Vec<ProxyConfig> {
        let mut proxies = self.catalog.proxies.clone();
        if let Some(existing) = proxies.iter_mut().find(|existing| existing.id == proxy.id) {
            *existing = proxy;
        } else {
            proxies.push(proxy);
        }
        proxies
    }

    pub(in crate::features) fn tunnel_groups_with_upsert(
        &self,
        group_id: Option<&str>,
        name: String,
    ) -> Option<Vec<TunnelGroup>> {
        let mut groups = self.catalog.tunnel_groups.clone();
        if let Some(group_id) = group_id {
            groups.iter_mut().find(|group| group.id == group_id)?.name = name;
        } else {
            groups.push(TunnelGroup {
                id: uuid(),
                name,
                sort_order: groups.len() as u32,
            });
        }
        Some(groups)
    }

    pub(in crate::features) fn proxy_groups_with_upsert(
        &self,
        group_id: Option<&str>,
        name: String,
    ) -> Option<Vec<ProxyGroup>> {
        let mut groups = self.catalog.proxy_groups.clone();
        if let Some(group_id) = group_id {
            groups.iter_mut().find(|group| group.id == group_id)?.name = name;
        } else {
            groups.push(ProxyGroup {
                id: uuid(),
                name,
                sort_order: groups.len() as u32,
            });
        }
        Some(groups)
    }

    pub(in crate::features) fn without_tunnel_group(&self, group_id: &str) -> TunnelGroupRemoval {
        let deleted_tunnel_ids = self
            .catalog
            .tunnels
            .iter()
            .filter(|tunnel| tunnel.group_id.as_deref() == Some(group_id))
            .map(|tunnel| tunnel.id.clone())
            .collect();
        let groups = self
            .catalog
            .tunnel_groups
            .iter()
            .filter(|group| group.id != group_id)
            .cloned()
            .collect();
        let tunnels = self
            .catalog
            .tunnels
            .iter()
            .filter(|tunnel| tunnel.group_id.as_deref() != Some(group_id))
            .cloned()
            .collect();
        TunnelGroupRemoval {
            groups,
            tunnels,
            deleted_tunnel_ids,
        }
    }

    pub(in crate::features) fn without_proxy_group(&self, group_id: &str) -> ProxyGroupRemoval {
        let deleted_proxy_ids = self
            .catalog
            .proxies
            .iter()
            .filter(|proxy| proxy.group_id.as_deref() == Some(group_id))
            .map(|proxy| proxy.id.clone())
            .collect();
        let groups = self
            .catalog
            .proxy_groups
            .iter()
            .filter(|group| group.id != group_id)
            .cloned()
            .collect();
        let proxies = self
            .catalog
            .proxies
            .iter()
            .filter(|proxy| proxy.group_id.as_deref() != Some(group_id))
            .cloned()
            .collect();
        ProxyGroupRemoval {
            groups,
            proxies,
            deleted_proxy_ids,
        }
    }

    pub(in crate::features) fn commit_tunnels(&mut self, tunnels: Vec<TunnelConfig>) {
        self.catalog.tunnels = tunnels;
    }

    pub(in crate::features) fn commit_proxies(&mut self, proxies: Vec<ProxyConfig>) {
        self.catalog.proxies = proxies;
    }

    pub(in crate::features) fn commit_tunnel_groups(&mut self, groups: Vec<TunnelGroup>) {
        self.catalog.tunnel_groups = groups;
    }

    pub(in crate::features) fn commit_proxy_groups(&mut self, groups: Vec<ProxyGroup>) {
        self.catalog.proxy_groups = groups;
    }

    pub(in crate::features) fn commit_tunnel_group_removal(
        &mut self,
        removal: TunnelGroupRemoval,
    ) -> Vec<String> {
        self.catalog.tunnel_groups = removal.groups;
        self.catalog.tunnels = removal.tunnels;
        removal.deleted_tunnel_ids
    }

    pub(in crate::features) fn commit_proxy_group_removal(
        &mut self,
        removal: ProxyGroupRemoval,
    ) -> Vec<String> {
        self.catalog.proxy_groups = removal.groups;
        self.catalog.proxies = removal.proxies;
        removal.deleted_proxy_ids
    }

    pub(in crate::features) fn open_tunnels(&self) -> Vec<SshTunnelInfo> {
        self.manager.list().unwrap_or_default()
    }

    pub(in crate::features) fn open_count(&self) -> usize {
        self.manager
            .list()
            .map(|tunnels| tunnels.len())
            .unwrap_or(0)
    }

    pub(in crate::features) fn is_open(&self, tunnel_id: &str) -> bool {
        self.manager.is_open(tunnel_id).unwrap_or(false)
    }

    pub(in crate::features) fn close_now(&self, tunnel_id: &str) -> anyhow::Result<()> {
        self.manager.close(tunnel_id)
    }

    pub(in crate::features) fn manager_for_job(&self) -> Arc<SshTunnelManager> {
        Arc::clone(&self.manager)
    }

    pub(in crate::features) fn is_pending(&self, tunnel_id: &str) -> bool {
        self.pending.iter().any(|id| id == tunnel_id)
    }

    pub(in crate::features) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(in crate::features) fn begin_job(&mut self, tunnel_id: String) -> bool {
        if self.is_pending(&tunnel_id) {
            return false;
        }
        self.pending.push(tunnel_id);
        true
    }

    pub(in crate::features) fn finish_job(&mut self, tunnel_id: &str) {
        self.pending.retain(|id| id != tunnel_id);
    }

    pub(in crate::features) fn take_event_receiver(
        &mut self,
    ) -> Option<UnboundedReceiver<TunnelJobResult>> {
        self.rx.take()
    }
}

#[cfg(test)]
mod tests {
    use nyaterm_core::{ProxyConfig, ProxyGroup, TunnelConfig, TunnelGroup};

    use crate::features::runtime_jobs::TunnelJobResult;

    use super::{TunnelCatalogState, TunnelFeatureState};

    #[test]
    fn tunnel_state_owns_job_channel_and_pending_lifecycle() {
        let mut tunnels = TunnelFeatureState::new(TunnelCatalogState::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
        assert!(tunnels.begin_job("tunnel-1".to_string()));
        assert!(!tunnels.begin_job("tunnel-1".to_string()));

        assert!(tunnels.is_pending("tunnel-1"));
        assert_eq!(tunnels.pending_count(), 1);

        let mut rx = tunnels
            .take_event_receiver()
            .expect("tunnel events should be owned by the state");
        tunnels
            .job_sender()
            .unbounded_send(TunnelJobResult {
                tunnel_id: "tunnel-1".to_string(),
                result: Err("failed".to_string()),
            })
            .expect("tunnel event channel should stay connected");
        let event = rx.try_recv().expect("the tunnel event should be queued");
        tunnels.finish_job(&event.tunnel_id);

        assert_eq!(tunnels.pending_count(), 0);
        assert!(!tunnels.is_pending("tunnel-1"));
    }

    #[test]
    fn catalog_candidates_do_not_mutate_until_committed() {
        let mut tunnels = TunnelFeatureState::new(TunnelCatalogState::new(
            vec![TunnelConfig {
                id: "tunnel-1".to_string(),
                name: "Original tunnel".to_string(),
                group_id: Some("group-1".to_string()),
                ..TunnelConfig::default()
            }],
            vec![TunnelGroup {
                id: "group-1".to_string(),
                name: "Original group".to_string(),
                sort_order: 0,
            }],
            vec![ProxyConfig {
                id: "proxy-1".to_string(),
                name: "Original proxy".to_string(),
                group_id: Some("proxy-group-1".to_string()),
                ..ProxyConfig::default()
            }],
            vec![ProxyGroup {
                id: "proxy-group-1".to_string(),
                name: "Original proxy group".to_string(),
                sort_order: 0,
            }],
        ));

        let moved = tunnels
            .tunnels_moved_to_group("tunnel-1", None)
            .expect("tunnel should exist");
        assert_eq!(tunnels.tunnels()[0].group_id.as_deref(), Some("group-1"));
        tunnels.commit_tunnels(moved);
        assert!(tunnels.tunnels()[0].group_id.is_none());

        let renamed = tunnels
            .proxy_groups_with_upsert(Some("proxy-group-1"), "Renamed proxy group".to_string())
            .expect("proxy group should exist");
        assert_eq!(tunnels.proxy_groups()[0].name, "Original proxy group");
        tunnels.commit_proxy_groups(renamed);
        assert_eq!(tunnels.proxy_groups()[0].name, "Renamed proxy group");
        assert!(
            tunnels
                .tunnel_groups_with_upsert(Some("missing"), "Ignored".to_string())
                .is_none()
        );
    }

    #[test]
    fn removing_groups_returns_and_commits_their_members_together() {
        let mut tunnels = TunnelFeatureState::new(TunnelCatalogState::new(
            vec![TunnelConfig {
                id: "tunnel-1".to_string(),
                group_id: Some("group-1".to_string()),
                ..TunnelConfig::default()
            }],
            vec![TunnelGroup {
                id: "group-1".to_string(),
                name: "Group".to_string(),
                sort_order: 0,
            }],
            vec![ProxyConfig {
                id: "proxy-1".to_string(),
                group_id: Some("proxy-group-1".to_string()),
                ..ProxyConfig::default()
            }],
            vec![ProxyGroup {
                id: "proxy-group-1".to_string(),
                name: "Proxy group".to_string(),
                sort_order: 0,
            }],
        ));

        let removal = tunnels.without_tunnel_group("group-1");
        assert_eq!(removal.deleted_tunnel_ids, ["tunnel-1"]);
        assert_eq!(tunnels.tunnels().len(), 1);
        let deleted_tunnel_ids = tunnels.commit_tunnel_group_removal(removal);
        assert_eq!(deleted_tunnel_ids, ["tunnel-1"]);
        assert!(tunnels.tunnel_groups().is_empty());
        assert!(tunnels.tunnels().is_empty());

        let removal = tunnels.without_proxy_group("proxy-group-1");
        assert_eq!(removal.deleted_proxy_ids, ["proxy-1"]);
        let deleted_proxy_ids = tunnels.commit_proxy_group_removal(removal);
        assert_eq!(deleted_proxy_ids, ["proxy-1"]);
        assert!(tunnels.proxy_groups().is_empty());
        assert!(tunnels.proxies().is_empty());
    }
}
