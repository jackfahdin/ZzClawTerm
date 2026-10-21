use std::time::{Duration, Instant};

use gpui::{
    App, AppContext as _, Entity, KeyDownEvent, Keystroke, TestAppContext, VisualTestContext,
};
use zzclawterm_core::test_support::TestTempDir;
use zzclawterm_transport::RemoteFilePath;

use crate::features::ZzClawTermApp;
use crate::features::file_manager::pane::{PaneEndpoint, PaneId};
use crate::features::file_manager::workspace::{FileManagerSeed, FileManagerWorkspace};
use crate::features::settings::SettingsPersistenceDomain;
use crate::features::test_support::{app_with_visible_local_session, session_focus_test_window};

fn workspace(
    cx: &mut TestAppContext,
    app: &Entity<ZzClawTermApp>,
    path: &str,
) -> Entity<FileManagerWorkspace> {
    let workspace = cx.new(|cx| {
        FileManagerWorkspace::new(
            app.clone(),
            "file-manager-reentrancy".into(),
            FileManagerSeed {
                left: PaneEndpoint::Local,
                left_path: Some(RemoteFilePath::new(path)),
                right: PaneEndpoint::Local,
                right_path: Some(RemoteFilePath::new(path)),
            },
            cx,
        )
    });
    app.update(cx, |app, _| {
        app.file_manager
            .insert("file-manager-reentrancy".into(), workspace.clone());
    });
    workspace
}

fn wait_for(cx: &mut VisualTestContext, description: &str, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if cx.read(&condition) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_listings(cx: &mut VisualTestContext, workspace: &Entity<FileManagerWorkspace>) {
    wait_for(cx, "directory listings", |cx| {
        [PaneId::Left, PaneId::Right]
            .into_iter()
            .all(|id| !workspace.read(cx).pane(id).loading)
    });
    cx.read(|cx| {
        for id in [PaneId::Left, PaneId::Right] {
            assert!(workspace.read(cx).pane(id).error.is_none());
        }
    });
}

#[gpui::test]
fn navigation_releases_workspace_before_app_updates_the_same_pane(cx: &mut TestAppContext) {
    // Directory listings use real blocking workers, which wake the test executor.
    cx.executor().allow_parking();
    let root = TestTempDir::new("zzclawterm-file-manager-navigation-reentrancy");
    let app = app_with_visible_local_session(cx, root.path(), "file-manager-test");
    std::fs::write(root.path().join("navigation.txt"), "fixture").unwrap();
    let path = root.path().to_string_lossy().into_owned();
    let workspace = workspace(cx, &app, &path);
    let cx = session_focus_test_window(cx, &app);

    for pane_id in [PaneId::Left, PaneId::Right] {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                let id = workspace.id.clone();
                let path = RemoteFilePath::new(path.clone());
                workspace.dispatch_app(window, cx, move |app, _, cx| {
                    app.navigate_file_manager_pane(id, pane_id, path, cx);
                });
                assert_eq!(workspace.pane(pane_id).generation, 0);
            });
        });
        cx.read(|cx| {
            let pane = workspace.read(cx).pane(pane_id);
            assert_eq!(pane.generation, 1);
            assert_eq!(pane.path.display_path, path);
        });
    }
    // Finish both replies before GPUI teardown drops their foreground tasks.
    wait_for_listings(cx, &workspace);
    cx.read(|cx| {
        for id in [PaneId::Left, PaneId::Right] {
            assert!(
                workspace
                    .read(cx)
                    .pane(id)
                    .entries
                    .iter()
                    .any(|entry| entry.name == "navigation.txt")
            );
        }
    });
}

#[gpui::test]
fn copy_and_move_shortcuts_can_read_the_originating_workspace(cx: &mut TestAppContext) {
    // Even an empty same-endpoint move refreshes both panes on blocking workers.
    cx.executor().allow_parking();
    let root = TestTempDir::new("zzclawterm-file-manager-shortcut-reentrancy");
    let app = app_with_visible_local_session(cx, root.path(), "file-manager-test");
    let workspace = workspace(cx, &app, &root.path().to_string_lossy());
    let cx = session_focus_test_window(cx, &app);

    // Both commands consult the workspace even when no files are selected.
    for key in ["f5", "f6"] {
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.handle_key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse(key).unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
            });
        });
        cx.read(|cx| assert!(app.read(cx).transfer.transfer_jobs().is_empty()));
    }
    wait_for_listings(cx, &workspace);
}

#[gpui::test]
fn layout_save_reads_the_workspace_after_its_listener_finishes(cx: &mut TestAppContext) {
    // Saving settings wakes the executor from the real database worker.
    cx.executor().allow_parking();
    let root = TestTempDir::new("zzclawterm-file-manager-layout-reentrancy");
    let app = app_with_visible_local_session(cx, root.path(), "file-manager-test");
    let workspace = workspace(cx, &app, &root.path().to_string_lossy());
    let cx = session_focus_test_window(cx, &app);

    cx.update(|window, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.split_percent = 65;
            workspace.set_active(PaneId::Right);
            workspace.dispatch_app(window, cx, |app, _, cx| {
                app.persist_file_manager_settings(cx);
            });
        });
    });
    wait_for(cx, "file-manager settings save", |cx| {
        !app.read(cx)
            .settings
            .dirty_persistence_domains()
            .contains(&SettingsPersistenceDomain::FileExplorer)
    });
    cx.read(|cx| {
        let settings = &app.read(cx).settings.summary().ui_file_manager;
        assert_eq!(settings.split_percent, 65);
        assert_eq!(
            settings.active_pane,
            zzclawterm_core::FileManagerPaneSide::Right
        );
    });
}
