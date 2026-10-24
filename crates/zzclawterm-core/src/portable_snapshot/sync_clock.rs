use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Causal history of a configuration snapshot. Time-of-day is never used to
/// decide which device's edit supersedes another device's edit.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotSyncClock {
    pub counters: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_revision: Option<String>,
}

impl SnapshotSyncClock {
    pub fn strictly_dominates(&self, other: &Self) -> bool {
        other
            .counters
            .iter()
            .all(|(device, count)| self.counters.get(device).copied().unwrap_or(0) >= *count)
            && self
                .counters
                .iter()
                .any(|(device, count)| *count > other.counters.get(device).copied().unwrap_or(0))
    }

    pub fn merge(&mut self, other: &Self) {
        for (device, count) in &other.counters {
            self.counters
                .entry(device.clone())
                .and_modify(|current| *current = (*current).max(*count))
                .or_insert(*count);
        }
        if self.legacy_revision.is_none() {
            self.legacy_revision.clone_from(&other.legacy_revision);
        }
    }
}
