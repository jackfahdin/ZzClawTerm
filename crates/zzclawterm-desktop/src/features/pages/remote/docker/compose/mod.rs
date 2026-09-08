mod menus;
mod project;
mod service;
mod status;

pub(in crate::features::pages::remote) use project::{
    DockerComposePanelState, docker_compose_panel,
};
