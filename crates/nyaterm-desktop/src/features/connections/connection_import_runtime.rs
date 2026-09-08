use rust_i18n::t;

use gpui::{Context, IntoElement, PathPromptOptions, SharedString, Window};
use nyaterm_store::StoreDomain;
use nyaterm_ui::NyaDialogWindowExt as _;
use zeroize::Zeroize as _;

use crate::features::{NyaTermApp, runtime_jobs::await_blocking_job};
use crate::models::ConnectionImportSource;

enum ConnectionImportResult {
    Imported(usize),
    Cancelled,
    Failed {
        source: ConnectionImportSource,
        auto_termius: bool,
        error: String,
    },
    Closed,
}

impl ConnectionImportSource {
    fn prompt_label(self) -> &'static str {
        match self {
            Self::NyatermBackup => "Import NyaTerm backup",
            Self::Xshell => "Import Xshell .xts sessions",
            Self::MobaXterm => "Import MobaXterm .mxtsessions sessions",
            Self::WindTerm => "Import WindTerm .sessions file",
            Self::SecureCrt => "Import SecureCRT .xml sessions",
            Self::FinalShell => "Import FinalShell conn directory",
            Self::Termius => "Import Termius IndexedDB directory",
            Self::Electerm => "Import Electerm bookmarks JSON",
            Self::NyatermJson => "Import NyaTerm sessions JSON",
        }
    }

    fn selecting_status(self) -> &'static str {
        match self {
            Self::NyatermBackup => "selecting NyaTerm backup",
            Self::Xshell => "selecting Xshell session import file",
            Self::MobaXterm => "selecting MobaXterm session import file",
            Self::WindTerm => "selecting WindTerm session import file",
            Self::SecureCrt => "selecting SecureCRT session XML",
            Self::FinalShell => "selecting FinalShell conn directory",
            Self::Termius => "selecting Termius IndexedDB directory",
            Self::Electerm => "selecting Electerm bookmarks JSON",
            Self::NyatermJson => "selecting NyaTerm session JSON",
        }
    }

    fn uses_directory_picker(self) -> bool {
        matches!(self, Self::FinalShell | Self::Termius)
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_connection_import_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connection_state.import_path_prompt_active()
            || self.settings.config_path_prompt_active()
        {
            self.shell
                .set_status("connection import picker is already open".to_string());
            cx.notify();
            return;
        }

        self.shell
            .set_status("select a connection import source".to_string());
        self.open_content_dialog(
            t!("settings.importConfig").to_string(),
            480.,
            |app, _, cx| app.connection_import_dialog_content(cx).into_any_element(),
            |_, _| {},
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn select_connection_import_source(
        &mut self,
        source: ConnectionImportSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_nya_dialog(cx);
        if source == ConnectionImportSource::NyatermBackup {
            self.prompt_encrypted_portable_snapshot_import(window, cx);
            return;
        }
        if source == ConnectionImportSource::Termius {
            self.import_termius_sessions(None, true, cx);
            return;
        }
        self.prompt_connection_session_import(source, cx);
    }

    fn prompt_connection_session_import(
        &mut self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) {
        if self.connection_state.import_path_prompt_active() {
            self.shell
                .set_status("connection import picker is already open".to_string());
            cx.notify();
            return;
        }

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !source.uses_directory_picker(),
            directories: source.uses_directory_picker(),
            multiple: false,
            prompt: Some(SharedString::from(source.prompt_label())),
        });
        let store = self.store_blocking_client();
        let scheduler = self.blocking_jobs.clone();
        self.connection_state.begin_import_path_prompt(source);
        self.shell.set_status(source.selecting_status().to_string());

        cx.spawn(async move |this, cx| {
            let result = match receiver.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => {
                        let task = scheduler.submit_task("connection-import", move |_| {
                            match prepare_connection_source(source, Some(path.as_path())).and_then(
                                |prepared| {
                                    store
                                        .request_fn(StoreDomain::Connections, move |database| {
                                            database.commit_session_import(prepared)
                                        })
                                        .map_err(|error| error.to_string())
                                },
                            ) {
                                Ok(count) => ConnectionImportResult::Imported(count),
                                Err(error) => ConnectionImportResult::Failed {
                                    source,
                                    auto_termius: false,
                                    error,
                                },
                            }
                        });
                        match await_blocking_job(task).await {
                            Ok(result) => result,
                            Err(error) => ConnectionImportResult::Failed {
                                source,
                                auto_termius: false,
                                error,
                            },
                        }
                    }
                    None => ConnectionImportResult::Cancelled,
                },
                Ok(Ok(None)) => ConnectionImportResult::Cancelled,
                Ok(Err(error)) => ConnectionImportResult::Failed {
                    source,
                    auto_termius: false,
                    error: error.to_string(),
                },
                Err(_) => ConnectionImportResult::Closed,
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_connection_import_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn import_termius_sessions(
        &mut self,
        indexed_db_path: Option<std::path::PathBuf>,
        auto_termius: bool,
        cx: &mut Context<Self>,
    ) {
        if self.connection_state.import_path_prompt_active() {
            self.shell
                .set_status("connection import picker is already open".to_string());
            cx.notify();
            return;
        }

        let store = self.store_blocking_client();
        let scheduler = self.blocking_jobs.clone();
        self.connection_state
            .begin_import_path_prompt(ConnectionImportSource::Termius);
        self.shell.set_status(if auto_termius {
            "importing Termius sessions".to_string()
        } else {
            "importing Termius sessions from selected directory".to_string()
        });

        cx.spawn(async move |this, cx| {
            let task =
                scheduler.submit_task("termius-import", move |_| match prepare_connection_source(
                    ConnectionImportSource::Termius,
                    indexed_db_path.as_deref(),
                )
                .and_then(|prepared| {
                    store
                        .request_fn(StoreDomain::Connections, move |database| {
                            database.commit_session_import(prepared)
                        })
                        .map_err(|error| error.to_string())
                }) {
                    Ok(count) => ConnectionImportResult::Imported(count),
                    Err(error) => ConnectionImportResult::Failed {
                        source: ConnectionImportSource::Termius,
                        auto_termius,
                        error,
                    },
                });
            let result = match await_blocking_job(task).await {
                Ok(result) => result,
                Err(error) => ConnectionImportResult::Failed {
                    source: ConnectionImportSource::Termius,
                    auto_termius,
                    error,
                },
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_connection_import_result(result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_connection_import_result(
        &mut self,
        result: ConnectionImportResult,
        cx: &mut Context<Self>,
    ) {
        self.connection_state.finish_import_path_prompt();
        match result {
            ConnectionImportResult::Imported(count) => {
                self.refresh_store_from_runtime_and_sync_theme(cx);
                self.connection_state.expand_all_catalog_groups();
                let message = t!("savedConnections.importSuccess", count = count);
                self.shell.set_status(message.clone());
                self.settings.update_store_status(message, true);
            }
            ConnectionImportResult::Cancelled => {
                self.shell
                    .set_status("connection import cancelled".to_string());
            }
            ConnectionImportResult::Failed {
                source,
                auto_termius,
                error,
            } => {
                if source == ConnectionImportSource::Termius
                    && auto_termius
                    && error.contains("Termius IndexedDB directory was not found")
                {
                    self.shell
                        .set_status("select Termius IndexedDB directory".to_string());
                    self.prompt_connection_session_import(ConnectionImportSource::Termius, cx);
                    return;
                }
                let message = t!("savedConnections.importFailed", error = error);
                self.shell.set_status(message.clone());
                self.settings.update_store_status(message, false);
            }
            ConnectionImportResult::Closed => {
                self.shell
                    .set_status("connection import picker closed".to_string());
            }
        }
        cx.notify();
    }
}

fn prepare_connection_source(
    source: ConnectionImportSource,
    path: Option<&std::path::Path>,
) -> Result<nyaterm_core::PreparedSessionImport, String> {
    match source {
        ConnectionImportSource::Termius => {
            let mut local_key = load_termius_local_key_secret()?;
            let result = nyaterm_core::prepare_termius_session_import(path, &local_key)
                .map_err(|error| error.to_string());
            local_key.zeroize();
            result
        }
        _ => {
            let path = path.ok_or_else(|| "connection import path was not selected".to_string())?;
            nyaterm_core::prepare_session_import(path).map_err(|error| error.to_string())
        }
    }
}

fn load_termius_local_key_secret() -> Result<Vec<u8>, String> {
    let mut errors = Vec::new();

    #[cfg(target_os = "windows")]
    {
        match keyring::Entry::new_with_target("Termius/localKey", "Termius", "localKey")
            .and_then(|entry| entry.get_secret())
        {
            Ok(secret) => return Ok(secret),
            Err(error) => errors.push(format!(
                "target=Termius/localKey service=Termius user=localKey: {error}"
            )),
        }
    }

    for (service, user) in [("Termius/localKey", "localKey"), ("Termius", "localKey")] {
        match keyring::Entry::new(service, user).and_then(|entry| entry.get_secret()) {
            Ok(secret) => return Ok(secret),
            Err(error) => errors.push(format!("{service}/{user}: {error}")),
        }
    }

    Err(format!(
        "Cannot read Termius localKey from the system keychain. Tried {}",
        errors.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::ConnectionImportSource;

    #[test]
    fn connection_import_sources_choose_expected_picker_kind() {
        assert!(!ConnectionImportSource::Xshell.uses_directory_picker());
        assert!(!ConnectionImportSource::SecureCrt.uses_directory_picker());
        assert!(!ConnectionImportSource::Electerm.uses_directory_picker());
        assert!(ConnectionImportSource::FinalShell.uses_directory_picker());
        assert!(ConnectionImportSource::Termius.uses_directory_picker());
        assert_eq!(
            ConnectionImportSource::Termius.prompt_label(),
            "Import Termius IndexedDB directory"
        );
    }
}
