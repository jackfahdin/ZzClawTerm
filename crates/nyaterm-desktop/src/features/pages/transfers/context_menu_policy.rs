#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransferContextMenuNode {
    Action(TransferContextMenuAction),
    Separator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransferContextMenuAction {
    Open,
    Preview,
    OpenInternal,
    OpenExternal,
    Refresh,
    Upload,
    Download,
    SendTo,
    Rename,
    Move,
    Delete,
    AddToFavorites,
    CopyPath,
    CopyName,
    CopyDirectoryPath,
    SendPath,
    SendName,
    SendDirectoryPath,
    Ai,
    Properties,
    GoUp,
    NewFile,
    NewFolder,
    NewSymlink,
}

pub(super) fn transfer_context_action_visible_for_backend(
    action: TransferContextMenuAction,
    backend: nyaterm_transport::FileBrowserBackendKind,
) -> bool {
    backend == nyaterm_transport::FileBrowserBackendKind::Remote
        || !matches!(
            action,
            TransferContextMenuAction::Upload
                | TransferContextMenuAction::Download
                | TransferContextMenuAction::NewSymlink
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TransferEntryMenuCapabilities {
    pub is_directory: bool,
    pub show_open_internal: bool,
    pub show_open_external: bool,
    pub show_preview: bool,
    pub has_ai_actions: bool,
    /// Whether at least one other browsable local or SSH session exists to send the
    /// selection to. Drives the "Send to" submenu after `Download`, matching
    /// the Tauri `openMoveDialog(getContextMenuEntries)` placement.
    pub has_send_targets: bool,
}

pub(super) fn transfer_entry_context_menu_policy(
    capabilities: TransferEntryMenuCapabilities,
) -> Vec<TransferContextMenuNode> {
    use TransferContextMenuAction as Action;
    use TransferContextMenuNode::{Action as Item, Separator};

    let mut items = vec![Item(Action::Open)];
    if capabilities.show_preview {
        items.push(Item(Action::Preview));
    }
    if capabilities.show_open_internal {
        items.push(Item(Action::OpenInternal));
    }
    if capabilities.show_open_external {
        items.push(Item(Action::OpenExternal));
    }
    items.extend([
        Separator,
        Item(Action::Refresh),
        Item(Action::Upload),
        Item(Action::Download),
    ]);
    // Parity: Tauri inserts the "Send to" submenu (and its own separator) right
    // after Download when there is at least one eligible target session.
    if capabilities.has_send_targets {
        items.extend([Separator, Item(Action::SendTo)]);
    }
    items.extend([
        Separator,
        Item(Action::Rename),
        Item(Action::Move),
        Item(Action::Delete),
        Separator,
    ]);
    if capabilities.is_directory {
        items.extend([Item(Action::AddToFavorites), Separator]);
    }
    items.extend([
        Item(Action::CopyPath),
        Item(Action::CopyName),
        Item(Action::CopyDirectoryPath),
        Separator,
        Item(Action::SendPath),
        Item(Action::SendName),
        Item(Action::SendDirectoryPath),
        Separator,
    ]);
    if capabilities.has_ai_actions {
        items.extend([Item(Action::Ai), Separator]);
    }
    items.push(Item(Action::Properties));
    items
}

pub(super) fn transfer_current_directory_context_menu_policy() -> Vec<TransferContextMenuNode> {
    use TransferContextMenuAction as Action;
    use TransferContextMenuNode::{Action as Item, Separator};

    vec![
        Item(Action::Refresh),
        Item(Action::Upload),
        Separator,
        Item(Action::NewFile),
        Item(Action::NewFolder),
        Item(Action::NewSymlink),
        Separator,
        Item(Action::CopyDirectoryPath),
        Item(Action::SendDirectoryPath),
        Separator,
        Item(Action::Properties),
    ]
}

pub(super) fn transfer_parent_directory_context_menu_policy() -> Vec<TransferContextMenuNode> {
    use TransferContextMenuAction as Action;
    use TransferContextMenuNode::{Action as Item, Separator};

    vec![Item(Action::GoUp), Separator, Item(Action::Refresh)]
}

/// Inputs a candidate session contributes when deciding whether it can appear in
/// the "Send to" submenu.
///
/// Kept UI-independent so the eligibility rule can be tested without a live app:
/// a target must be connected (not disconnected), expose a browser backend, and be a
/// session other than the source the selection lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SendToCandidate {
    pub session_id: String,
    pub has_browser_backend: bool,
    pub is_disconnected: bool,
}

pub(super) fn send_to_candidate_is_eligible(
    source_session_id: &str,
    candidate: &SendToCandidate,
) -> bool {
    candidate.session_id != source_session_id
        && candidate.has_browser_backend
        && !candidate.is_disconnected
}

/// Resolve the destination directory for a "Send to" transfer.
///
/// Prefers the target session's own cached browser directory; otherwise falls
/// back to that session's home (or `cwd`). The source session's path must never
/// leak in here, so both fallbacks are explicit target-session inputs.
pub(super) fn send_to_target_directory(
    cached_current_path: Option<&str>,
    target_home_or_cwd: Option<&str>,
) -> String {
    let normalize = |value: &str| -> Option<String> {
        let trimmed = value.trim();
        (!trimmed.is_empty() && trimmed != ".").then(|| trimmed.trim_end_matches('/').to_string())
    };
    cached_current_path
        .and_then(normalize)
        .or_else(|| target_home_or_cwd.and_then(normalize))
        .unwrap_or_else(|| ".".to_string())
}

/// Join a target directory and a source entry name into a destination path.
///
/// The entry name comes from the source listing; the directory is the resolved
/// target directory above. Root is preserved so `/` + `file` becomes `/file`.
#[cfg(test)]
pub(super) fn send_to_destination_path(target_dir: &str, entry_name: &str) -> String {
    let dir = target_dir.trim_end_matches('/');
    match dir {
        "" if target_dir.starts_with('/') => format!("/{entry_name}"),
        "" | "." => entry_name.to_string(),
        dir => format!("{dir}/{entry_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SendToCandidate, TransferContextMenuAction as Action, TransferContextMenuNode as Node,
        TransferEntryMenuCapabilities, send_to_candidate_is_eligible, send_to_destination_path,
        send_to_target_directory, transfer_context_action_visible_for_backend,
        transfer_current_directory_context_menu_policy, transfer_entry_context_menu_policy,
        transfer_parent_directory_context_menu_policy,
    };

    #[test]
    fn local_backend_hides_remote_only_actions() {
        use nyaterm_transport::FileBrowserBackendKind::{Local, Remote};

        for action in [Action::Upload, Action::Download, Action::NewSymlink] {
            assert!(!transfer_context_action_visible_for_backend(action, Local));
            assert!(transfer_context_action_visible_for_backend(action, Remote));
        }
        assert!(transfer_context_action_visible_for_backend(
            Action::Properties,
            Local
        ));
        assert!(transfer_context_action_visible_for_backend(
            Action::SendTo,
            Local
        ));
    }

    #[test]
    fn file_menu_matches_tauri_group_order() {
        assert_eq!(
            transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
                is_directory: false,
                show_open_internal: true,
                show_open_external: false,
                show_preview: false,
                has_ai_actions: true,
                has_send_targets: false,
            }),
            vec![
                Node::Action(Action::Open),
                Node::Action(Action::OpenInternal),
                Node::Separator,
                Node::Action(Action::Refresh),
                Node::Action(Action::Upload),
                Node::Action(Action::Download),
                Node::Separator,
                Node::Action(Action::Rename),
                Node::Action(Action::Move),
                Node::Action(Action::Delete),
                Node::Separator,
                Node::Action(Action::CopyPath),
                Node::Action(Action::CopyName),
                Node::Action(Action::CopyDirectoryPath),
                Node::Separator,
                Node::Action(Action::SendPath),
                Node::Action(Action::SendName),
                Node::Action(Action::SendDirectoryPath),
                Node::Separator,
                Node::Action(Action::Ai),
                Node::Separator,
                Node::Action(Action::Properties),
            ]
        );
    }

    #[test]
    fn preview_action_follows_open_and_only_for_files() {
        let file = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: false,
            show_open_internal: false,
            show_open_external: false,
            show_preview: true,
            has_ai_actions: false,
            has_send_targets: false,
        });
        assert_eq!(file[0], Node::Action(Action::Open));
        assert_eq!(file[1], Node::Action(Action::Preview));

        // A directory never offers preview, so the caller passes show_preview:false.
        let directory = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: true,
            show_open_internal: false,
            show_open_external: false,
            show_preview: false,
            has_ai_actions: false,
            has_send_targets: false,
        });
        assert!(
            !directory.contains(&Node::Action(Action::Preview)),
            "directories must not show a preview action"
        );
    }

    #[test]
    fn directory_menu_inserts_favorite_group_and_external_editor_alternative() {
        let items = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: true,
            show_open_internal: false,
            show_open_external: true,
            show_preview: false,
            has_ai_actions: false,
            has_send_targets: false,
        });
        assert_eq!(items[1], Node::Action(Action::OpenExternal));
        assert!(items.windows(3).any(|group| {
            group
                == [
                    Node::Action(Action::AddToFavorites),
                    Node::Separator,
                    Node::Action(Action::CopyPath),
                ]
        }));
        assert_eq!(items.last(), Some(&Node::Action(Action::Properties)));
    }

    #[test]
    fn preview_action_shows_for_every_file_including_unrenderable_types() {
        // Parity: the preview action is offered for any non-directory file. A
        // type the preview cannot render (e.g. a `.zip`) still gets the action;
        // the window opens and shows the unsupported message. The capability is
        // driven by `show_transfer_preview_menu_entry`, which is `!is_directory`.
        let unrenderable = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: false,
            show_open_internal: false,
            show_open_external: true,
            show_preview: true,
            has_ai_actions: false,
            has_send_targets: false,
        });
        assert_eq!(unrenderable[0], Node::Action(Action::Open));
        assert_eq!(unrenderable[1], Node::Action(Action::Preview));
    }

    #[test]
    fn current_and_parent_directory_menus_match_tauri_groups() {
        assert_eq!(
            transfer_current_directory_context_menu_policy(),
            vec![
                Node::Action(Action::Refresh),
                Node::Action(Action::Upload),
                Node::Separator,
                Node::Action(Action::NewFile),
                Node::Action(Action::NewFolder),
                Node::Action(Action::NewSymlink),
                Node::Separator,
                Node::Action(Action::CopyDirectoryPath),
                Node::Action(Action::SendDirectoryPath),
                Node::Separator,
                Node::Action(Action::Properties),
            ]
        );
        assert_eq!(
            transfer_parent_directory_context_menu_policy(),
            vec![
                Node::Action(Action::GoUp),
                Node::Separator,
                Node::Action(Action::Refresh),
            ]
        );
    }

    #[test]
    fn send_to_submenu_follows_download_with_its_own_separator() {
        let items = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: false,
            show_open_internal: false,
            show_open_external: false,
            show_preview: false,
            has_ai_actions: false,
            has_send_targets: true,
        });
        // Download, then a separator, then Send to, then the Rename/Move/Delete group.
        let download = items
            .iter()
            .position(|node| node == &Node::Action(Action::Download))
            .expect("download action present");
        assert_eq!(items[download + 1], Node::Separator);
        assert_eq!(items[download + 2], Node::Action(Action::SendTo));
        assert_eq!(items[download + 3], Node::Separator);
        assert_eq!(items[download + 4], Node::Action(Action::Rename));
        assert_eq!(items[download + 5], Node::Action(Action::Move));
        assert_eq!(items[download + 6], Node::Action(Action::Delete));
    }

    #[test]
    fn send_to_submenu_absent_without_eligible_targets() {
        let items = transfer_entry_context_menu_policy(TransferEntryMenuCapabilities {
            is_directory: false,
            show_open_internal: false,
            show_open_external: false,
            show_preview: false,
            has_ai_actions: false,
            has_send_targets: false,
        });
        assert!(!items.contains(&Node::Action(Action::SendTo)));
        let download = items
            .iter()
            .position(|node| node == &Node::Action(Action::Download))
            .expect("download action present");
        // Straight to the Rename/Move/Delete group when there is nowhere to send.
        assert_eq!(items[download + 1], Node::Separator);
        assert_eq!(items[download + 2], Node::Action(Action::Rename));
    }

    #[test]
    fn send_to_candidate_excludes_source_and_unbrowsable_sessions() {
        let source = "session-source";
        assert!(!send_to_candidate_is_eligible(
            source,
            &SendToCandidate {
                session_id: source.to_string(),
                has_browser_backend: true,
                is_disconnected: false,
            }
        ));
        assert!(!send_to_candidate_is_eligible(
            source,
            &SendToCandidate {
                session_id: "session-b".to_string(),
                has_browser_backend: false,
                is_disconnected: false,
            }
        ));
        assert!(!send_to_candidate_is_eligible(
            source,
            &SendToCandidate {
                session_id: "session-c".to_string(),
                has_browser_backend: true,
                is_disconnected: true,
            }
        ));
        assert!(send_to_candidate_is_eligible(
            source,
            &SendToCandidate {
                session_id: "session-d".to_string(),
                has_browser_backend: true,
                is_disconnected: false,
            }
        ));
    }

    #[test]
    fn send_to_target_directory_prefers_cache_then_home_never_source() {
        // Cached browser path wins.
        assert_eq!(
            send_to_target_directory(Some("/srv/data/"), Some("/home/bob")),
            "/srv/data"
        );
        // No usable cache falls back to the target session's home/cwd.
        assert_eq!(
            send_to_target_directory(None, Some("/home/bob")),
            "/home/bob"
        );
        assert_eq!(
            send_to_target_directory(Some("."), Some("/home/bob")),
            "/home/bob"
        );
        // Nothing usable at all degrades to the safe relative default, and it is
        // never seeded from a source path (the caller passes only target inputs).
        assert_eq!(send_to_target_directory(Some("   "), None), ".");
        assert_eq!(send_to_target_directory(None, None), ".");
    }

    #[test]
    fn send_to_destination_path_joins_directory_and_entry_name() {
        assert_eq!(
            send_to_destination_path("/srv/data", "file.txt"),
            "/srv/data/file.txt"
        );
        assert_eq!(
            send_to_destination_path("/srv/data/", "file.txt"),
            "/srv/data/file.txt"
        );
        assert_eq!(send_to_destination_path("/", "file.txt"), "/file.txt");
        assert_eq!(send_to_destination_path(".", "file.txt"), "file.txt");
    }
}
