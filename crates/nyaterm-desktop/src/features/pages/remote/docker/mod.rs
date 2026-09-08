use rust_i18n::t;

use std::borrow::Cow;

use gpui::Rgba;

use crate::features::formatting::docker_state_label;
use crate::theme::ThemePalette;

#[derive(Clone)]
pub(in crate::features::pages::remote) struct DockerLabels {
    pub no_session: Cow<'static, str>,
    pub error: Cow<'static, str>,
    pub unavailable: Cow<'static, str>,
    pub no_matches: Cow<'static, str>,
    pub logs: Cow<'static, str>,
    pub enter: Cow<'static, str>,
    pub start: Cow<'static, str>,
    pub stop: Cow<'static, str>,
    pub restart: Cow<'static, str>,
    pub kill: Cow<'static, str>,
    pub delete: Cow<'static, str>,
    pub confirm_action_title: Cow<'static, str>,
    pub networks: Cow<'static, str>,
    pub remove_image: Cow<'static, str>,
    pub remove_volume: Cow<'static, str>,
    pub remove_network: Cow<'static, str>,
    pub up: Cow<'static, str>,
    pub down: Cow<'static, str>,
    pub loading_services: Cow<'static, str>,
    pub service_load_failed: Cow<'static, str>,
    pub no_services: Cow<'static, str>,
    pub no_containers: Cow<'static, str>,
    pub not_created: Cow<'static, str>,
    pub retry: Cow<'static, str>,
    pub loading: Cow<'static, str>,
    pub container_details: Cow<'static, str>,
    pub identity: Cow<'static, str>,
    pub container_name: Cow<'static, str>,
    pub container_id: Cow<'static, str>,
    pub image: Cow<'static, str>,
    pub status: Cow<'static, str>,
    pub created_at: Cow<'static, str>,
    pub size: Cow<'static, str>,
    pub started_at: Cow<'static, str>,
    pub finished_at: Cow<'static, str>,
    pub restart_count: Cow<'static, str>,
    pub entrypoint: Cow<'static, str>,
    pub command: Cow<'static, str>,
    pub networking: Cow<'static, str>,
    pub ports: Cow<'static, str>,
    pub io: Cow<'static, str>,
    pub net_io: Cow<'static, str>,
    pub block_io: Cow<'static, str>,
    pub mounts: Cow<'static, str>,
    pub cpu: Cow<'static, str>,
    pub memory: Cow<'static, str>,
    pub pids: Cow<'static, str>,
    pub copy: Cow<'static, str>,
    pub refresh: Cow<'static, str>,
    pub close: Cow<'static, str>,
    pub state_created: Cow<'static, str>,
    pub state_dead: Cow<'static, str>,
    pub state_exited: Cow<'static, str>,
    pub state_paused: Cow<'static, str>,
    pub state_removing: Cow<'static, str>,
    pub state_restarting: Cow<'static, str>,
    pub state_running: Cow<'static, str>,
    pub state_unknown: Cow<'static, str>,
}

impl DockerLabels {
    pub fn confirm_description(&self, action: &str, target: &str) -> String {
        t!(
            "dockerManager.confirmActionDesc",
            action = action,
            target = target
        )
        .to_string()
    }

    pub fn volume_driver_label(&self, driver: &str) -> String {
        t!("dockerManager.volumeDriver", driver = driver).to_string()
    }

    pub fn state_label(&self, state: &str) -> Cow<'static, str> {
        let normalized = state.trim().to_ascii_lowercase();
        let legacy_label = docker_state_label(state);
        if normalized == "running" || normalized == "up" || legacy_label == "running" {
            self.state_running.clone()
        } else if normalized == "exited" || normalized == "stopped" || normalized == "down" {
            self.state_exited.clone()
        } else if normalized == "created" || legacy_label == "created" {
            self.state_created.clone()
        } else if normalized == "dead" || legacy_label == "dead" {
            self.state_dead.clone()
        } else if normalized == "paused" || legacy_label == "paused" {
            self.state_paused.clone()
        } else if normalized == "removing" {
            self.state_removing.clone()
        } else if normalized == "restarting" || legacy_label == "restart" {
            self.state_restarting.clone()
        } else {
            self.state_unknown.clone()
        }
    }
}

#[derive(Clone)]
pub(in crate::features::pages::remote) struct DockerRenderContext {
    pub palette: ThemePalette,
    pub menu_bg: Rgba,
    pub labels: DockerLabels,
}

mod compose;
mod containers;
mod controls;
mod details;
mod resources;

pub(super) use compose::{DockerComposePanelState, docker_compose_panel};
pub(super) use containers::{DockerContainersPanelState, docker_containers_panel};
pub(super) use controls::{DockerTabBarLabels, docker_overview_strip, docker_tab_bar};
pub(super) use details::docker_details_panel;
pub(super) use resources::{docker_images_panel, docker_networks_panel, docker_volumes_panel};
