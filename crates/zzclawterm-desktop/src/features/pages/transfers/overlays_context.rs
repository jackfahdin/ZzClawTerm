use rust_i18n::t;

use gpui::Context;
use zzclawterm_transport::SftpFileEntry;
use zzclawterm_ui::ZzClawMenuItem;

use crate::features::ZzClawTermApp;
use crate::models::{TransferBrowserContextTarget, TransferPathPromptKind};

use super::TransferPathPart;

impl ZzClawTermApp {
    pub(in crate::features::pages::transfers) fn transfer_browser_context_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<ZzClawMenuItem> {
        match self.transfer.browser_view().context_target.clone() {
            TransferBrowserContextTarget::CurrentDirectory => {
                self.transfer_browser_current_context_menu_items(cx)
            }
            TransferBrowserContextTarget::ParentDirectory => {
                self.transfer_browser_parent_context_menu_items(cx)
            }
            TransferBrowserContextTarget::Entry(path) => {
                if self.transfer.rename_dialog_is_open() {
                    return Vec::new();
                }
                let Some(entry) = self
                    .transfer
                    .browser_view()
                    .entries
                    .iter()
                    .find(|entry| entry.matches_identity(&path))
                    .cloned()
                else {
                    return Vec::new();
                };
                self.transfer_browser_entry_context_menu_items(entry, cx)
            }
            TransferBrowserContextTarget::Suppressed => Vec::new(),
        }
    }

    pub(in crate::features::pages::transfers) fn transfer_browser_current_context_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<ZzClawMenuItem> {
        use super::context_menu_policy::{
            TransferContextMenuAction as Action, TransferContextMenuNode as Node,
            transfer_context_action_visible_for_backend,
            transfer_current_directory_context_menu_policy,
        };

        let policy = transfer_current_directory_context_menu_policy();
        let local_backend = self.session.active_file_browser_backend()
            == Some(zzclawterm_transport::FileBrowserBackendKind::Local);
        let mut items = Vec::with_capacity(policy.len());
        for node in policy {
            if let Node::Action(action) = node {
                let backend = if local_backend {
                    zzclawterm_transport::FileBrowserBackendKind::Local
                } else {
                    zzclawterm_transport::FileBrowserBackendKind::Remote
                };
                if !transfer_context_action_visible_for_backend(action, backend) {
                    continue;
                }
            }
            let item = match node {
                Node::Separator => ZzClawMenuItem::separator(),
                Node::Action(Action::Refresh) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmRefresh"))
                        .icon("icons/fe/refresh.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_transfer_browser(window, cx);
                            this.defer_transfer_panel_snapshot_flush(cx);
                        }))
                }
                Node::Action(Action::Upload) => ZzClawMenuItem::submenu(
                    t!("fileExplorer.cmUpload"),
                    vec![
                        ZzClawMenuItem::action(t!("fileExplorer.upload"))
                            .icon("icons/fe/upload.svg")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.prompt_transfer_browser_upload_path(
                                    TransferPathPromptKind::UploadFile,
                                    cx,
                                );
                            })),
                        ZzClawMenuItem::action(t!("fileExplorer.uploadFolder"))
                            .icon("icons/fe/upload-folder.svg")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.prompt_transfer_browser_upload_path(
                                    TransferPathPromptKind::UploadDirectory,
                                    cx,
                                );
                            })),
                    ],
                )
                .icon("icons/fe/upload.svg"),
                Node::Action(Action::NewFile) => ZzClawMenuItem::action(t!("fileExplorer.newFile"))
                    .icon("icons/fe/new-file.svg")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_transfer_new_file_dialog(window, cx);
                    })),
                Node::Action(Action::NewFolder) => {
                    ZzClawMenuItem::action(t!("fileExplorer.newFolder"))
                        .icon("icons/fe/new-folder.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_transfer_new_folder_dialog(window, cx);
                        }))
                }
                Node::Action(Action::NewSymlink) => {
                    ZzClawMenuItem::action(t!("fileExplorer.newSymlink"))
                        .icon("icons/conn/symlink.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_transfer_new_symlink_dialog(window, cx);
                        }))
                }
                Node::Action(Action::CopyDirectoryPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmCopyDirPath"))
                        .icon("icons/copy.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_current_transfer_browser_path(cx);
                        }))
                }
                Node::Action(Action::SendDirectoryPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmTerminalDirPath"))
                        .icon("icons/fe/send-path.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send_current_transfer_browser_path_to_terminal(cx);
                        }))
                }
                Node::Action(Action::Properties) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmProperties"))
                        .icon("icons/menu/info.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_current_transfer_browser_properties(window, cx);
                        }))
                }
                // The current-directory policy only emits the nodes handled above;
                // anything else would be a policy/handler drift, so skip it.
                Node::Action(_) => continue,
            };
            items.push(item);
        }
        items
    }

    pub(in crate::features::pages::transfers) fn transfer_browser_parent_context_menu_items(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<ZzClawMenuItem> {
        use super::context_menu_policy::{
            TransferContextMenuAction as Action, TransferContextMenuNode as Node,
            transfer_parent_directory_context_menu_policy,
        };

        let policy = transfer_parent_directory_context_menu_policy();
        let mut items = Vec::with_capacity(policy.len());
        for node in policy {
            let item = match node {
                Node::Separator => ZzClawMenuItem::separator(),
                Node::Action(Action::GoUp) => ZzClawMenuItem::action(t!("fileExplorer.goUp"))
                    .icon("icons/fe/up.svg")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_transfer_parent_directory(window, cx);
                        this.defer_transfer_panel_snapshot_flush(cx);
                    })),
                Node::Action(Action::Refresh) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmRefresh"))
                        .icon("icons/fe/refresh.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_transfer_browser(window, cx);
                            this.defer_transfer_panel_snapshot_flush(cx);
                        }))
                }
                Node::Action(_) => continue,
            };
            items.push(item);
        }
        items
    }

    pub(in crate::features::pages::transfers) fn transfer_browser_entry_context_menu_items(
        &mut self,
        entry: SftpFileEntry,
        cx: &mut Context<Self>,
    ) -> Vec<ZzClawMenuItem> {
        use super::context_menu_policy::{
            TransferContextMenuAction as Action, TransferContextMenuNode as Node,
            TransferEntryMenuCapabilities, transfer_context_action_visible_for_backend,
            transfer_entry_context_menu_policy,
        };

        let ai_actions = self.enabled_transfer_file_ai_actions_for_entry(&entry);
        let send_targets = self.transfer_send_to_targets();
        let policy = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: entry.is_directory(),
            show_open_internal: self.show_transfer_open_internal_menu_entry(&entry),
            show_open_external: self.show_transfer_open_external_menu_entry(&entry),
            show_preview: self.show_transfer_preview_menu_entry(&entry),
            has_ai_actions: !ai_actions.is_empty(),
            has_send_targets: !send_targets.is_empty(),
        });
        let local_backend = self.session.active_file_browser_backend()
            == Some(zzclawterm_transport::FileBrowserBackendKind::Local);
        let mut items = Vec::with_capacity(policy.len());

        for node in policy {
            if let Node::Action(action) = node {
                let backend = if local_backend {
                    zzclawterm_transport::FileBrowserBackendKind::Local
                } else {
                    zzclawterm_transport::FileBrowserBackendKind::Remote
                };
                if !transfer_context_action_visible_for_backend(action, backend) {
                    continue;
                }
            }
            let item = match node {
                Node::Separator => ZzClawMenuItem::separator(),
                Node::Action(Action::Open) => ZzClawMenuItem::action(t!("fileExplorer.cmOpen"))
                    .icon("icons/session/folder-open.svg")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_selected_transfer_default(window, cx);
                    })),
                Node::Action(Action::Preview) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmPreview"))
                        .icon("icons/eye.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_transfer_preview(window, cx);
                        }))
                }
                Node::Action(Action::OpenInternal) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmOpenInternalEditor"))
                        .icon("icons/net/edit.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_transfer_editor(window, cx);
                        }))
                }
                Node::Action(Action::OpenExternal) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmOpenExternalEditor"))
                        .icon("icons/net/edit.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_transfer_external(window, cx);
                        }))
                }
                Node::Action(Action::Refresh) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmRefresh"))
                        .icon("icons/fe/refresh.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_transfer_browser(window, cx);
                            this.defer_transfer_panel_snapshot_flush(cx);
                        }))
                }
                Node::Action(Action::Upload) => ZzClawMenuItem::submenu(
                    t!("fileExplorer.cmUpload"),
                    vec![
                        ZzClawMenuItem::action(t!("fileExplorer.upload"))
                            .icon("icons/fe/upload.svg")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.prompt_transfer_browser_upload_path(
                                    TransferPathPromptKind::UploadFile,
                                    cx,
                                );
                            })),
                        ZzClawMenuItem::action(t!("fileExplorer.uploadFolder"))
                            .icon("icons/fe/upload-folder.svg")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.prompt_transfer_browser_upload_path(
                                    TransferPathPromptKind::UploadDirectory,
                                    cx,
                                );
                            })),
                    ],
                )
                .icon("icons/fe/upload.svg"),
                Node::Action(Action::Download) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmDownload"))
                        .icon("icons/fe/download.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_selected_sftp_download_jobs(window, cx);
                            this.defer_transfer_panel_snapshot_flush(cx);
                        }))
                }
                Node::Action(Action::Rename) => ZzClawMenuItem::action(t!("fileExplorer.cmRename"))
                    .icon("icons/session/rename.svg")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_transfer_rename_dialog(window, cx);
                        this.defer_transfer_panel_snapshot_flush(cx);
                    })),
                Node::Action(Action::Move) => ZzClawMenuItem::action(t!("fileExplorer.cmMove"))
                    .icon("icons/net/move.svg")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_transfer_move_dialog_for_selection(window, cx);
                    })),
                Node::Action(Action::Delete) => ZzClawMenuItem::action(t!("fileExplorer.cmDelete"))
                    .icon("icons/net/delete.svg")
                    .danger()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_selected_transfer_delete_dialog(window, cx);
                    })),
                Node::Action(Action::SendTo) => {
                    let send_items = send_targets
                        .iter()
                        .cloned()
                        .map(|target| {
                            let session_id = target.session_id.clone();
                            let mut item = ZzClawMenuItem::action(target.label.clone())
                                .icon("icons/send.svg")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.start_send_selected_transfers_to_session(
                                        session_id.clone(),
                                        window,
                                        cx,
                                    );
                                    this.defer_transfer_panel_snapshot_flush(cx);
                                }));
                            if let Some(meta) = target.meta {
                                item = item.shortcut(meta);
                            }
                            item
                        })
                        .collect::<Vec<_>>();
                    ZzClawMenuItem::submenu(t!("fileExplorer.cmSendTo"), send_items)
                        .icon("icons/send.svg")
                }
                Node::Action(Action::AddToFavorites) => {
                    let favorite_path = entry.path.clone();
                    ZzClawMenuItem::action(t!("fileExplorer.addToFavorites"))
                        .icon("icons/fe/star.svg")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.add_transfer_browser_favorite_path(favorite_path.clone(), cx);
                            this.defer_transfer_panel_snapshot_flush(cx);
                        }))
                }
                Node::Action(Action::CopyPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmCopyPath"))
                        .icon("icons/copy.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_selected_transfer_path(TransferPathPart::Full, cx);
                        }))
                }
                Node::Action(Action::CopyName) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmCopyName"))
                        .icon("icons/copy.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_selected_transfer_path(TransferPathPart::Name, cx);
                        }))
                }
                Node::Action(Action::CopyDirectoryPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmCopyDirPath"))
                        .icon("icons/copy.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_selected_transfer_path(TransferPathPart::Directory, cx);
                        }))
                }
                Node::Action(Action::SendPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmTerminalPath"))
                        .icon("icons/fe/send-path.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send_selected_transfer_path_to_terminal(
                                TransferPathPart::Full,
                                cx,
                            );
                        }))
                }
                Node::Action(Action::SendName) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmTerminalName"))
                        .icon("icons/fe/send-path.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send_selected_transfer_path_to_terminal(
                                TransferPathPart::Name,
                                cx,
                            );
                        }))
                }
                Node::Action(Action::SendDirectoryPath) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmTerminalDirPath"))
                        .icon("icons/fe/send-path.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send_selected_transfer_path_to_terminal(
                                TransferPathPart::Directory,
                                cx,
                            );
                        }))
                }
                Node::Action(Action::Ai) => {
                    let ai_items = ai_actions
                        .iter()
                        .cloned()
                        .map(|action| {
                            let ai_entry = entry.clone();
                            ZzClawMenuItem::action(action.name.clone()).on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.start_transfer_file_ai_action(
                                        ai_entry.clone(),
                                        action.clone(),
                                        window,
                                        cx,
                                    );
                                },
                            ))
                        })
                        .collect();
                    ZzClawMenuItem::submenu("AI", ai_items).icon("icons/ai.svg")
                }
                Node::Action(Action::Properties) => {
                    ZzClawMenuItem::action(t!("fileExplorer.cmProperties"))
                        .icon("icons/menu/info.svg")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_transfer_properties(window, cx);
                        }))
                }
                Node::Action(
                    Action::GoUp | Action::NewFile | Action::NewFolder | Action::NewSymlink,
                ) => continue,
            };
            items.push(item);
        }
        items
    }
}
