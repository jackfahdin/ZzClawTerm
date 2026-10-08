use super::{
    RecentActivationCache, next_recent_after_close, normalize_new_workspace_ui,
    should_enable_startup_screen_lock, workspace_targets_from_order,
};
use zzclawterm_core::{ACTIVATION_QUEUE_CAPACITY, WorkspaceId, WorkspaceUiState};

#[test]
fn activation_ids_are_deduplicated_until_fifo_eviction() {
    let mut cache = RecentActivationCache::default();
    let id = |index: u64| {
        let mut id = [0_u8; 16];
        id[..8].copy_from_slice(&index.to_le_bytes());
        id
    };
    assert!(cache.remember(id(0)));
    assert!(!cache.remember(id(0)));
    for index in 1..=(ACTIVATION_QUEUE_CAPACITY * 4) as u64 {
        assert!(cache.remember(id(index)));
    }
    assert!(!cache.remember(id(1)));
    assert!(cache.remember(id(0)), "the oldest ID was evicted");
}

#[test]
fn closing_recent_workspace_uses_reverse_window_order_not_hash_iteration() {
    let first = WorkspaceId::new();
    let second = WorkspaceId::new();
    let third = WorkspaceId::new();
    let order = [first, second, third];
    assert_eq!(
        next_recent_after_close(Some(third), third, &order, |id| id != third),
        Some(second)
    );
    assert_eq!(
        next_recent_after_close(Some(third), third, &order, |id| id == first),
        Some(first)
    );
    assert_eq!(
        next_recent_after_close(Some(third), third, &order, |_| false),
        None
    );
}

#[test]
fn workspace_target_ordinals_do_not_change_when_source_is_filtered_out() {
    let first = WorkspaceId::new();
    let source = WorkspaceId::new();
    let third = WorkspaceId::new();

    let targets = workspace_targets_from_order(source, &[first, source, third], |_| true);

    assert_eq!(targets.len(), 2);
    assert_eq!((targets[0].workspace_id, targets[0].ordinal), (first, 1));
    assert_eq!((targets[1].workspace_id, targets[1].ordinal), (third, 3));
}

#[test]
fn new_workspace_normalizes_single_owner_settings_page() {
    let ui = WorkspaceUiState {
        current_page: "settings".to_string(),
        ..WorkspaceUiState::default()
    };

    assert_eq!(normalize_new_workspace_ui(ui).current_page, "workspace");
}

#[test]
fn initial_bootstrap_enables_startup_screen_lock_from_persisted_setting() {
    assert!(should_enable_startup_screen_lock(false, true));
    assert!(!should_enable_startup_screen_lock(false, false));
}

#[test]
fn shared_state_refresh_does_not_retrigger_startup_screen_lock() {
    assert!(!should_enable_startup_screen_lock(true, true));
}
