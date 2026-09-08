use rust_i18n::t;

use std::borrow::Cow;

use gpui::Context;
use nyaterm_ui::{NyaAppMenuBar, NyaDialogWindowExt as _, NyaMenuItem};

use crate::app_shell::NativeMenuCommand;
use crate::features::NyaTermApp;
use crate::models::{
    ActivityBarEntry, ActivityBarZone, HeaderStatusMode, NavItem, PanelOpenMode, PanelSide,
    SmartSplitMode, TitleMenu,
};

impl NyaTermApp {
    pub(crate) fn set_title_menu_bar(&mut self, menu_bar: gpui::Entity<NyaAppMenuBar>) {
        self.shell.set_title_menu_bar(menu_bar);
    }

    pub(crate) fn title_menu_label(&self, menu: TitleMenu) -> Cow<'static, str> {
        t!(menu.i18n_key())
    }

    pub(crate) fn build_title_menu_items(
        &mut self,
        menu: TitleMenu,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        self.title_menu_items(menu, cx)
    }

    pub(crate) fn prepare_title_menu(&mut self, cx: &mut Context<Self>) {
        self.shell.close_open_tabs_menu();
        self.shell.close_new_session_menu();
        cx.notify();
    }

    pub(crate) fn perform_native_menu_command(
        &mut self,
        command: NativeMenuCommand,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            NativeMenuCommand::NewSession => {
                self.open_connection_editor(None, None, false, window, cx);
            }
            NativeMenuCommand::QuickSwitch => {
                self.open_quick_switch(window, cx);
            }
            NativeMenuCommand::OpenSettings => {
                self.open_page(NavItem::Settings, cx);
                self.shell.set_status("settings opened".to_string());
                cx.notify();
            }
            NativeMenuCommand::ToggleLeftSidebar => {
                self.toggle_left_sidebar(cx);
            }
            NativeMenuCommand::ToggleRightSidebar => {
                self.toggle_right_inspector(cx);
            }
            NativeMenuCommand::ZoomIn => {
                self.zoom_terminal_in(cx);
            }
            NativeMenuCommand::ZoomOut => {
                self.zoom_terminal_out(cx);
            }
            NativeMenuCommand::ResetZoom => {
                self.reset_terminal_font_size(cx);
            }
            NativeMenuCommand::TerminalCopy => {
                self.copy_terminal_selection_or_visible(cx);
            }
            NativeMenuCommand::TerminalPaste => {
                self.paste_from_clipboard(window, cx);
            }
            NativeMenuCommand::TerminalFind => {
                self.open_terminal_search(window, cx);
            }
            NativeMenuCommand::TerminalClear => {
                self.clear_terminal(cx);
            }
            NativeMenuCommand::TerminalSelectAll => {
                self.select_all_terminal(cx);
            }
            NativeMenuCommand::ManageSyncGroups => {
                self.open_sync_groups(window, cx);
            }
        }
    }

    pub(crate) fn open_connection_import_dialog_for_menu(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.open_connection_import_dialog(window, cx);
    }

    pub(crate) fn prompt_encrypted_portable_snapshot_export_for_menu(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_encrypted_portable_snapshot_export(window, cx);
    }

    pub(crate) fn open_documentation_for_menu(&mut self, cx: &mut Context<Self>) {
        self.open_documentation(cx);
    }

    pub(crate) fn open_update_dialog_for_menu(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.open_update_dialog(window, cx);
    }

    pub(crate) fn reveal_log_dir_for_menu(&mut self, cx: &mut Context<Self>) {
        self.reveal_log_dir(cx);
    }

    pub(crate) fn open_about_for_menu(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.open_about(window, cx);
    }

    pub(crate) fn resize_all_known_terminal_surfaces_for_menu(&mut self, cx: &mut Context<Self>) {
        let changed = self.resize_all_known_terminal_surfaces();
        self.shell.set_status(if changed {
            "terminal sizes reset".to_string()
        } else {
            "terminal sizes already current".to_string()
        });
        cx.notify();
    }

    pub(in crate::features) fn title_menu_items(
        &self,
        menu: TitleMenu,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        match menu {
            TitleMenu::File => self.title_file_menu_items(cx),
            TitleMenu::View => self.title_view_menu_items(cx),
            TitleMenu::Terminal => self.title_terminal_menu_items(cx),
            TitleMenu::Help => self.title_help_menu_items(cx),
        }
    }

    fn title_file_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("menu.newSession"))
                .icon("icons/conn/add.svg")
                .shortcut(self.display_shortcut_for("tab.newSession", "Ctrl+Shift+N"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.shell.close_open_tabs_menu();
                    this.shell.close_new_session_menu();
                    this.open_connection_editor(None, None, false, window, cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("settings.importConfig"))
                .icon("icons/import.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.close_nya_dialog(cx);
                    this.open_connection_import_dialog(window, cx);
                })),
            NyaMenuItem::action(t!("settings.exportConfig"))
                .icon("icons/menu/export.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.close_nya_dialog(cx);
                    this.prompt_encrypted_portable_snapshot_export(window, cx);
                })),
        ]
    }

    fn title_view_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        let current_theme = self
            .settings
            .summary()
            .terminal_theme
            .as_deref()
            .unwrap_or("");
        let current_header_status =
            HeaderStatusMode::from_setting(&self.settings.summary().ui_header_status_mode);
        let header_status_visible = self.settings.summary().ui_header_status_visible;
        vec![
            NyaMenuItem::submenu(t!("menu.theme"), self.title_theme_menu_items(cx))
                .icon("icons/menu/palette.svg"),
            NyaMenuItem::submenu(
                t!("menu.terminalTheme"),
                self.title_terminal_theme_menu_items(current_theme, cx),
            )
            .icon("icons/conn/terminal.svg"),
            NyaMenuItem::submenu(t!("menu.language"), self.title_language_menu_items(cx))
                .icon("icons/translation.svg"),
            NyaMenuItem::submenu(
                t!("menu.headerStatus"),
                self.title_header_status_menu_items(
                    current_header_status,
                    header_status_visible,
                    cx,
                ),
            )
            .icon("icons/menu/info.svg"),
            NyaMenuItem::submenu(t!("menu.panels"), self.title_panels_menu_items(cx))
                .icon("icons/menu/sidebar.svg"),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("menu.zoomIn"))
                .icon("icons/menu/zoom-in.svg")
                .shortcut(self.display_shortcut_for("view.zoomIn", "Ctrl+="))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.zoom_terminal_in(cx);
                })),
            NyaMenuItem::action(t!("menu.zoomOut"))
                .icon("icons/menu/zoom-out.svg")
                .shortcut(self.display_shortcut_for("view.zoomOut", "Ctrl+-"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.zoom_terminal_out(cx);
                })),
            NyaMenuItem::action(t!("menu.resetZoom"))
                .icon("icons/menu/reset.svg")
                .shortcut(self.display_shortcut_for("view.resetZoom", "Ctrl+0"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reset_terminal_font_size(cx);
                })),
        ]
    }

    fn title_terminal_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("menu.commandPalette"))
                .icon("icons/fe/search.svg")
                .shortcut(self.display_shortcut_for("tab.quickSwitch", "Ctrl+Shift+S"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_quick_switch(window, cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::submenu(
                t!("menu.terminalDisplay"),
                self.title_terminal_display_menu_items(cx),
            )
            .icon("icons/eye.svg"),
            NyaMenuItem::action(t!("settings.actionLinks"))
                .icon("icons/fe/search.svg")
                .checked(self.settings.summary().terminal_action_links_enabled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_terminal_action_links(cx);
                })),
            NyaMenuItem::action(t!("settings.terminalZoomEnabled"))
                .icon("icons/menu/reset.svg")
                .checked(self.settings.summary().interaction_terminal_zoom_enabled)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_terminal_zoom_enabled(cx);
                })),
            NyaMenuItem::submenu(t!("menu.smartSplit"), self.title_smart_split_menu_items(cx))
                .icon("icons/menu/split.svg"),
            NyaMenuItem::separator(),
            NyaMenuItem::submenu(t!("menu.syncInput"), self.title_sync_input_menu_items(cx))
                .icon("icons/sync.svg"),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("menu.broadcastToAll"))
                .icon("icons/menu/broadcast.svg")
                .checked(self.sync_input.broadcast_to_all())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_broadcast_to_all(cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("menu.clearTerminal"))
                .icon("icons/fe/delete.svg")
                .shortcut(self.display_shortcut_for("terminal.clear", "Ctrl+L"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.clear_terminal(cx);
                })),
            NyaMenuItem::action(t!("menu.refitTerminals"))
                .icon("icons/menu/fit.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.resize_all_known_terminal_surfaces_for_menu(cx);
                })),
            NyaMenuItem::action(t!("menu.unsplit"))
                .icon("icons/menu/fit.svg")
                .disabled(self.shell.workspace_split().is_none())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.unsplit_workspace(cx);
                })),
        ]
    }

    fn title_help_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        let update_label = if self.update.is_pending() {
            t!("updater.checking")
        } else if self.update.info().is_some_and(|info| info.available) {
            t!("updater.newVersionAvailable")
        } else {
            t!("menu.checkForUpdates")
        };

        vec![
            NyaMenuItem::action(t!("menu.documentation"))
                .icon("icons/menu/book.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_documentation(cx);
                })),
            NyaMenuItem::action(update_label)
                .icon("icons/menu/update.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_update_dialog(window, cx);
                })),
            NyaMenuItem::action(t!("menu.viewLogs"))
                .icon("icons/menu/article.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.reveal_log_dir(cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::action(format!("{} NyaTerm", t!("menu.about")))
                .icon("icons/menu/info.svg")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_about(window, cx);
                })),
        ]
    }

    fn title_theme_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        let current = self.settings.summary().theme.as_str();
        crate::theme::APPEARANCE_THEME_IDS
            .iter()
            .map(|&theme| {
                let selected =
                    current == theme || (current == "catppuccin" && theme == "catppuccin-mocha");
                NyaMenuItem::action(crate::theme::appearance_theme_label(theme))
                    .checked(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_appearance_theme(theme, cx);
                    }))
            })
            .collect()
    }

    fn title_terminal_theme_menu_items(
        &self,
        current_theme: &str,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let mut items = vec![
            NyaMenuItem::action(t!("settings.followUiTheme"))
                .checked(current_theme.trim().is_empty())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_terminal_theme(None, cx);
                })),
        ];
        items.extend(crate::theme::APPEARANCE_THEME_IDS.iter().map(|&theme| {
            NyaMenuItem::action(crate::theme::appearance_theme_label(theme))
                .checked(
                    current_theme == theme
                        || (current_theme == "catppuccin" && theme == "catppuccin-mocha"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_terminal_theme(Some(theme), cx);
                }))
        }));
        items
    }

    fn title_language_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        let active = crate::i18n::normalize_locale(&self.settings.summary().language);
        crate::i18n::available_locales()
            .into_iter()
            .map(|locale| {
                let checked = locale == active;
                let label = crate::i18n::locale_display_name(&locale);
                NyaMenuItem::action(label)
                    .checked(checked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_ui_language(&locale, cx);
                    }))
            })
            .collect()
    }

    fn title_header_status_menu_items(
        &self,
        current: HeaderStatusMode,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let mut items = vec![
            NyaMenuItem::action(t!("headerStatus.hidden"))
                .checked(!visible)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_header_status_hide_confirm(window, cx);
                })),
        ];
        items.extend(HeaderStatusMode::ALL.into_iter().map(|mode| {
            NyaMenuItem::action(t!(mode.i18n_key()))
                .checked(visible && current == mode)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_header_status_mode(mode, cx);
                }))
        }));
        items
    }

    fn title_panels_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        let floating = self.shell.panel_open_mode().is_floating();
        let multi_open = self.shell.panel_multi_open();
        vec![
            NyaMenuItem::action(t!("panel.floatingMode"))
                .checked(floating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_panel_open_mode(
                        if floating {
                            PanelOpenMode::Docked
                        } else {
                            PanelOpenMode::Floating
                        },
                        cx,
                    );
                })),
            NyaMenuItem::action(t!("panel.multiOpen"))
                .checked(multi_open)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_panel_multi_open(cx);
                })),
            NyaMenuItem::separator(),
            NyaMenuItem::submenu(
                t!("activityBar.leftSide"),
                self.title_side_panel_menu_items(PanelSide::Left, cx),
            ),
            NyaMenuItem::submenu(
                t!("activityBar.rightSide"),
                self.title_side_panel_menu_items(PanelSide::Right, cx),
            ),
            NyaMenuItem::separator(),
            NyaMenuItem::action(t!("activityBar.resetLayout"))
                .danger()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.confirm_reset_activity_bar_layout(window, cx);
                })),
        ]
    }

    fn title_side_panel_menu_items(
        &self,
        side: PanelSide,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        let zones = match side {
            PanelSide::Left => [ActivityBarZone::LeftTop, ActivityBarZone::LeftBottom],
            PanelSide::Right => [ActivityBarZone::RightTop, ActivityBarZone::RightBottom],
        };
        let entries = zones
            .into_iter()
            .flat_map(|zone| self.shell.activity_bar_layout().zone(zone))
            .filter_map(|id| ActivityBarEntry::from_persistence_id(id))
            .filter(|entry| self.activity_entry_visible(*entry))
            .filter(|entry| match entry {
                ActivityBarEntry::Panel(item) => !item.opens_settings(),
                ActivityBarEntry::Recording => true,
                ActivityBarEntry::QuickCommands
                | ActivityBarEntry::CommandSend
                | ActivityBarEntry::Lock => false,
            })
            .map(|entry| {
                let id = entry.persistence_id().to_string();
                let label = entry
                    .i18n_key()
                    .map(|key| t!(key).to_string())
                    .unwrap_or_else(|| entry.label().to_string());
                let visible = !self.shell.activity_bar_layout().is_hidden(&id);
                (id, label, visible)
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return vec![NyaMenuItem::action(t!("activityBar.noPanels")).disabled(true)];
        }
        entries
            .into_iter()
            .map(|(id, label, visible)| {
                NyaMenuItem::action(label)
                    .checked(visible)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if visible {
                            this.hide_activity_entry(id.clone(), cx);
                        } else {
                            this.show_activity_entry(id.clone(), cx);
                        }
                    }))
            })
            .collect()
    }

    fn title_terminal_display_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("settings.showWorkspacePadding"))
                .checked(self.settings.summary().terminal_show_workspace_padding)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_terminal_workspace_padding(cx);
                })),
            NyaMenuItem::action(t!("settings.showLineNumbers"))
                .checked(self.settings.summary().terminal_show_line_numbers)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_terminal_line_numbers(cx);
                })),
            NyaMenuItem::action(t!("settings.showTimestamps"))
                .checked(self.settings.summary().terminal_show_timestamps)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_terminal_timestamps(cx);
                })),
        ]
    }

    fn title_smart_split_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("menu.autoTile"))
                .icon("icons/view-grid.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_smart_split(SmartSplitMode::Auto, cx);
                })),
            NyaMenuItem::action(t!("menu.tileHorizontally"))
                .icon("icons/menu/horizontal.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_smart_split(SmartSplitMode::Horizontal, cx);
                })),
            NyaMenuItem::action(t!("menu.tileVertically"))
                .icon("icons/menu/vertical.svg")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_smart_split(SmartSplitMode::Vertical, cx);
                })),
        ]
    }

    fn title_sync_input_menu_items(&self, cx: &mut Context<Self>) -> Vec<NyaMenuItem> {
        vec![
            NyaMenuItem::action(t!("menu.manageGroups"))
                .icon("icons/settings.svg")
                .shortcut(self.display_shortcut_for("terminal.manageSyncGroups", "Ctrl+Shift+G"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_sync_groups(window, cx);
                })),
        ]
    }

    #[cfg(test)]
    pub(crate) fn title_menu_items_for_test(
        &self,
        menu: crate::models::TitleMenu,
        cx: &mut Context<Self>,
    ) -> Vec<NyaMenuItem> {
        self.title_menu_items(menu, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gpui::{AppContext as _, TestAppContext};
    use nyaterm_core::{AppRuntime, RuntimeMode, uuid};
    use nyaterm_ui::NyaMenuItem;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::NyaTermApp;
    use crate::models::TitleMenu;

    fn unique_test_dir() -> PathBuf {
        // A uuid rather than a clock reading: these tests run in parallel and
        // Windows' ~15ms clock granularity lets a nanosecond timestamp repeat,
        // which would share one config dir and so one settings database.
        std::env::temp_dir().join(format!(
            "nyaterm-title-menu-{}-{}",
            std::process::id(),
            uuid()
        ))
    }

    fn menu_app(cx: &mut TestAppContext) -> gpui::Entity<NyaTermApp> {
        let root = unique_test_dir();
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.clone(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    #[test]
    fn custom_title_bar_keeps_tauri_visible_top_level_menus_without_edit() {
        let labels = [
            TitleMenu::File,
            TitleMenu::View,
            TitleMenu::Terminal,
            TitleMenu::Help,
        ]
        .map(TitleMenu::label);

        assert_eq!(labels, ["File", "View", "Terminal", "Help"]);
        assert!(!labels.contains(&"Edit"));
    }

    #[test]
    fn title_view_menu_matches_tauri_structure() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        let items = cx.update_entity(&app, |app, cx| {
            app.title_menu_items_for_test(TitleMenu::View, cx)
        });
        let labels = items
            .iter()
            .map(NyaMenuItem::test_label)
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Theme"));
        assert!(labels.contains(&"Terminal Theme"));
        assert!(labels.contains(&"Language"));
        assert!(labels.contains(&"Header Status"));
        assert!(labels.contains(&"Panels"));
        assert!(labels.contains(&"Zoom In"));
        assert!(labels.contains(&"Zoom Out"));
        assert!(labels.contains(&"Reset Zoom"));
    }

    #[test]
    fn title_terminal_menu_matches_tauri_structure_and_default_state() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        let items = cx.update_entity(&app, |app, cx| {
            app.title_menu_items_for_test(TitleMenu::Terminal, cx)
        });
        let labels = items
            .iter()
            .map(NyaMenuItem::test_label)
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Command Palette"));
        assert!(labels.contains(&"Display"));
        assert!(labels.contains(&"Action Links"));
        assert!(labels.contains(&"Enable terminal zoom"));
        assert!(labels.contains(&"Smart Split"));
        assert!(labels.contains(&"Unsplit"));
        assert!(labels.contains(&"Sync Input"));
        assert!(labels.contains(&"Broadcast All"));
        assert!(labels.contains(&"Clear Terminal"));
        assert!(labels.contains(&"Refit Terminal Size"));

        let action_links = items
            .iter()
            .find(|item| item.test_label() == "Action Links")
            .expect("action links item");
        let zoom_enabled = items
            .iter()
            .find(|item| item.test_label() == "Enable terminal zoom")
            .expect("terminal zoom item");
        let unsplit = items
            .iter()
            .find(|item| item.test_label() == "Unsplit")
            .expect("unsplit item");

        assert!(!action_links.test_presentation().4);
        assert!(zoom_enabled.test_presentation().4);
        assert!(unsplit.test_presentation().3);
    }

    #[test]
    fn terminal_display_submenu_has_expected_checks() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        let items = cx.update_entity(&app, |app, cx| {
            app.title_menu_items_for_test(TitleMenu::Terminal, cx)
        });
        let display = items
            .iter()
            .find(|item| item.test_label() == "Display")
            .expect("terminal display");
        let submenu = display.children().expect("terminal display submenu");
        assert_eq!(
            submenu
                .iter()
                .map(NyaMenuItem::test_label)
                .collect::<Vec<_>>(),
            vec!["Show Padding", "Line Numbers", "Show Timestamps"]
        );
        assert!(submenu.iter().all(|item| !item.test_presentation().4));
    }

    #[test]
    fn panels_submenu_exposes_independent_modes_and_dynamic_sides() {
        let mut cx = TestAppContext::single();
        let app = menu_app(&mut cx);
        let items = cx.update_entity(&app, |app, cx| {
            app.title_menu_items_for_test(TitleMenu::View, cx)
        });
        let panels = items
            .iter()
            .find(|item| item.test_label() == "Panels")
            .expect("panels submenu");
        let submenu = panels.children().expect("panels submenu children");
        let labels = submenu
            .iter()
            .map(NyaMenuItem::test_label)
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Floating Panels"));
        assert!(labels.contains(&"Multi Open"));
        assert!(labels.contains(&"Left"));
        assert!(labels.contains(&"Right"));
        assert!(labels.contains(&"Reset Activity Bar Layout"));
        let floating = submenu
            .iter()
            .find(|item| item.test_label() == "Floating Panels")
            .expect("floating entry");
        let multi = submenu
            .iter()
            .find(|item| item.test_label() == "Multi Open")
            .expect("multi-open entry");
        assert!(!floating.test_presentation().4);
        assert!(!multi.test_presentation().4);

        let left = submenu
            .iter()
            .find(|item| item.test_label() == "Left")
            .and_then(NyaMenuItem::children)
            .expect("left panel submenu");
        assert!(left.iter().any(|item| item.test_label() == "Files"));
        assert!(left.iter().any(|item| item.test_label() == "Notes"));
        assert!(left.iter().all(|item| item.test_presentation().4));
    }
}
