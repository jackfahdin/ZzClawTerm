use nyaterm_core::AppRuntime;
use nyaterm_store::{StoreBlockingClient, StoreUiClient};

use super::ai::{AiFeatureState, AiPanel};
use super::assets::StartWorkspaceFeatureState;
use super::commands::CommandFeatureState;
use super::connections::ConnectionFeatureState;
use super::mcp::McpHostFeatureState;
use super::notes::{NotesCatalogEvent, NotesFeatureState, NotesPanel};
use super::pages::connections::panel::ConnectionPanel;
use super::pages::remote::RemotePanels;
use super::pages::settings::panel::SettingsPanel;
use super::pages::transfers::panel::TransferPanel;
use super::panels::SendCommandFeatureState;
use super::recording::RecordingFeatureState;
use super::remote::RemoteOpsFeatureState;
use super::remote_desktop::RemoteDesktopFeatureState;
use super::selects::SelectRegistry;
use super::session::SessionFeatureState;
use super::settings::{SecurityFeatureState, SettingsFeatureState};
use super::shell::ShellFeatureState;
use super::sync::CloudSyncFeatureState;
use super::sync_input::SyncInputFeatureState;
use super::terminal::TerminalFeatureState;
use super::text_inputs::TextInputRegistry;
use super::transfers::TransferFeatureState;
use super::translation::TranslationFeatureState;
use super::tunnels::TunnelFeatureState;
use super::update::UpdateFeatureState;

mod construct;
mod store_runtime;
mod types;

pub(in crate::features) use types::SettingsDraftSnapshot;

pub struct NyaTermApp {
    pub(in crate::features) blocking_jobs: crate::blocking_jobs::BlockingJobScheduler,
    pub(in crate::features) stores: crate::entities::UiStoreHandles,
    pub(in crate::features) store_ui: StoreUiClient,
    pub(in crate::features) store_blocking: StoreBlockingClient,
    pub(in crate::features) runtime: AppRuntime,
    pub(in crate::features) connection_state: ConnectionFeatureState,
    pub(in crate::features) start_workspace: StartWorkspaceFeatureState,
    pub(in crate::features) connection_panel: gpui::Entity<ConnectionPanel>,
    pub(in crate::features) settings_panel: gpui::Entity<SettingsPanel>,
    pub(in crate::features) native_settings_panel: Option<gpui::WeakEntity<SettingsPanel>>,
    pub(in crate::features) transfer_panel: gpui::Entity<TransferPanel>,
    pub(in crate::features) notes: NotesFeatureState,
    pub(in crate::features) notes_panel: gpui::Entity<NotesPanel>,
    /// Real text inputs for the panels that have not been given their own,
    /// keyed by an id the panel picks. See `features::text_inputs`.
    pub(in crate::features) text_inputs: TextInputRegistry,
    /// Component-backed selects keyed by stable feature ids.
    pub(in crate::features) selects: SelectRegistry,
    pub(in crate::features) commands: CommandFeatureState,
    pub(in crate::features) remote_ops: RemoteOpsFeatureState,
    /// The five polling panels. They own their refresh schedules; `remote_ops` stays
    /// the authoritative owner of the data those schedules fetch.
    pub(in crate::features) remote_panels: RemotePanels,
    pub(in crate::features) remote_desktop: RemoteDesktopFeatureState,
    pub(in crate::features) security: SecurityFeatureState,
    pub(in crate::features) settings: SettingsFeatureState,
    pub(in crate::features) ai: AiFeatureState,
    pub(in crate::features) mcp: McpHostFeatureState,
    pub(in crate::features) ai_panel: gpui::Entity<AiPanel>,
    pub(in crate::features) terminal: TerminalFeatureState,
    pub(in crate::features) send_command: SendCommandFeatureState,
    pub(in crate::features) transfer: TransferFeatureState,
    pub(in crate::features) translation: TranslationFeatureState,
    pub(in crate::features) update: UpdateFeatureState,
    pub(in crate::features) cloud_sync: CloudSyncFeatureState,
    pub(in crate::features) session: SessionFeatureState,
    pub(in crate::features) shell: ShellFeatureState,
    pub(in crate::features) sync_input: SyncInputFeatureState,
    pub(in crate::features) recording: RecordingFeatureState,
    pub(in crate::features) tunnel_state: TunnelFeatureState,
    /// Last field by design: test cleanup must run after storage clients and
    /// feature-owned background work have released their Windows file handles.
    #[cfg(test)]
    pub(in crate::features) _test_config_dir: Option<crate::test_support::TestConfigDir>,
}

impl gpui::EventEmitter<NotesCatalogEvent> for NyaTermApp {}

/// Requests that the application shell start its orderly shutdown sequence.
///
/// The shell owns persistence and worker shutdown, while `NyaTermApp` owns the
/// user-facing close confirmation. An event keeps those responsibilities
/// separate without allowing a window-close callback to bypass the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppLifecycleEvent {
    ShutdownRequested,
}

impl gpui::EventEmitter<AppLifecycleEvent> for NyaTermApp {}
