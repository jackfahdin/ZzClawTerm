//! Transport-neutral inputs for monitoring-derived asset patches.
//!
//! `nyaterm-core` must not depend on `nyaterm-transport`, so the desktop layer
//! flattens `RemoteStats`, `RemoteGpuOverview`, and `RemoteNpuOverview` into
//! these small structs before calling the pure mapping functions in
//! [`super::monitoring`]. The field selection matches exactly what the Tauri
//! `buildAssetPatchFrom*` helpers read from their equivalents.

/// The subset of a remote-stats snapshot that seeds an asset patch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetStatsSnapshot {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    /// `memory.used + memory.available` in bytes, matching the Tauri mapping.
    pub memory_total_bytes: u64,
    pub disks: Vec<AssetDiskSnapshot>,
}

/// One disk row from a remote-stats snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetDiskSnapshot {
    /// Device path (`/dev/nvme0n1`); falls back to the mount point.
    pub device: String,
    /// Mount point, used only when the device path is unknown.
    pub mount: String,
    pub total_bytes: u64,
}

/// One accelerator (GPU or NPU) device from a monitoring overview.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetAcceleratorSnapshot {
    pub vendor: String,
    pub model: String,
    /// Device memory in mebibytes; converted to bytes during mapping.
    pub memory_total_mb: u64,
}
