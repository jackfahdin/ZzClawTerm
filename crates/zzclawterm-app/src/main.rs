// Release builds are a pure GUI app: without this the console subsystem makes
// Windows attach a cmd window that mirrors the log output on every launch.
// Debug builds keep the console so `cargo run` still shows tracing output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod single_instance;

use anyhow::Context as _;
use gpui::{App, AppContext};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use zzclawterm_app::assets;
use zzclawterm_core::app_identity::AppFlavor;
use zzclawterm_core::{ActivationRequest, AppRuntime, LOG_FILE_PREFIX, LOG_FILE_SUFFIX};
use zzclawterm_desktop::{AppShellStartup, DesktopController, DesktopControllerGlobal};

use single_instance::{SingleInstanceOutcome, acquire};

fn main() -> anyhow::Result<()> {
    if zzclawterm_desktop::run_update_helper_if_requested() {
        return Ok(());
    }
    zzclawterm_desktop::schedule_update_cleanup();
    let runtime = AppRuntime::resolve().context("resolve zzclawterm runtime")?;
    runtime
        .ensure_directories()
        .context("prepare runtime directories")?;
    let initial_activation = ActivationRequest::from_os_args(
        *uuid::Uuid::new_v4().as_bytes(),
        std::env::args_os().skip(1),
    );
    let mut instance_owner = match acquire(runtime.config_dir(), initial_activation.clone())? {
        SingleInstanceOutcome::Owner(owner) => owner,
        SingleInstanceOutcome::Forwarded => return Ok(()),
    };
    let activation_tx = instance_owner.activation_sender();
    let activation_rx = instance_owner.take_activation_receiver();
    let _log_guard = init_tracing(&runtime);
    zzclawterm_desktop::preload_i18n()
        .map_err(anyhow::Error::msg)
        .context("preload translation catalogs")?;

    let application = gpui_platform::application().with_assets(assets::ZzClawTermAssets);
    let open_url_tx = activation_tx.clone();
    application.on_open_urls(move |urls| {
        let request = ActivationRequest::from_os_args(
            *uuid::Uuid::new_v4().as_bytes(),
            urls.into_iter().map(std::ffi::OsString::from),
        );
        let _ = open_url_tx.try_send(request);
    });
    application.on_reopen(move |cx| {
        let request =
            ActivationRequest::from_os_args(*uuid::Uuid::new_v4().as_bytes(), std::iter::empty());
        let _ = activation_tx.try_send(request);
        cx.activate(true);
    });

    application.run(move |cx: &mut App| {
        let flavor = AppFlavor::current();
        cx.set_app_identity(flavor.application_identifier(), flavor.display_name());
        gpui_kit::init(cx);
        cx.set_quit_mode(gpui::QuitMode::Explicit);
        zzclawterm_desktop::init(cx);
        let startup = AppShellStartup::prepare(&runtime);
        let controller = cx.new(|cx| DesktopController::new(runtime.clone(), startup, cx));
        cx.set_global(DesktopControllerGlobal(controller.clone()));
        controller
            .update(cx, |controller, cx| {
                controller.launch(initial_activation, activation_rx, cx)
            })
            .expect("failed to open ZzClawTerm windows");

        cx.activate(true);
    });

    Ok(())
}

fn init_tracing(runtime: &AppRuntime) -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("zzclawterm=info,zzclawterm_core=info,zzclawterm_transport=info,warn")
    });
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .build(runtime.log_dir())
        .ok();

    if let Some(file_appender) = file_appender {
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().compact())
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(file_writer),
            )
            .try_init()
            .ok();
        Some(guard)
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .try_init()
            .ok();
        None
    }
}
